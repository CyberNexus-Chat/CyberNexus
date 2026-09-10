// /v1/responses HTTP handler.
//
// Accepts POST /v1/responses (or /responses), reads ~/.echobird/codex.json
// for the active model / API key / upstream base_url, translates the
// Codex Responses-API request into a Chat Completions request, forwards
// it to the upstream provider, then translates the response back to
// Responses API SSE / JSON.
//
// Per-request relay read: the file is fetched fresh every time so
// EchoBird model switches take effect without restarting Codex or the
// proxy. config.toml's base_url is permanently `http://127.0.0.1:53682/v1`
// (see `apply_codex` in tool_config_manager/codex.rs), so Codex's view never
// changes either.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
    routing::post,
    Router,
};
use futures_util::{Stream, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::config_manager::{default_relay_dir, is_openai, read_echobird_relay, RELAY_FILENAME};
use super::protocol_converter::responses_to_chat;
use super::session_store::SessionStore;
use super::stream_handler::{
    chat_error_to_responses_error, chat_to_responses_non_stream, chat_usage_to_responses_usage,
    SseEvent, StreamState,
};

/// Cap on how many bytes of upstream error body we accumulate before
/// truncating. Error envelopes are small (a JSON `{ "error": ... }`);
/// we never want a misbehaving upstream pushing us into unbounded growth.
const UPSTREAM_ERROR_BODY_CAP: usize = 16 * 1024;

/// Maximum size of an inbound request body the proxy will buffer before
/// returning 413 Payload Too Large. Axum's default is 2 MiB, which is
/// not enough for modern Codex flows where `computer_use` tool results
/// can include screenshots and large HTML snapshots that push a single
/// turn's payload well past 2 MiB. 64 MiB gives 3-6x headroom over the
/// worst observed real-world request while still bounding memory.
const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Max wait for the upstream to deliver the FIRST chunk of a streaming
/// response. Guards against "upstream accepted the request but never
/// started replying", which would otherwise pin a reqwest connection +
/// spawned tokio task forever.
///
/// Only the first read is bounded. Once the first byte arrives the
/// stream runs free - thinking models (grok / o-series / DeepSeek-R1 /
/// QwQ) legitimately go silent for many minutes between chunks while
/// they reason, and a per-chunk gap timeout there was the #1 cause of
/// mid-turn disconnects: the proxy would `fail()` the stream on a long
/// thinking pause, Codex would receive `response.failed`, and the turn
/// would collapse in the UI ("chat a bit then it folds"). Dead
/// connections mid-stream are detected by TCP keepalive (see the http
/// client builder), which surfaces as a stream error rather than a
/// silent gap.
const UPSTREAM_FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub struct AppState {
    pub(crate) sessions: SessionStore,
    pub(crate) http_client: reqwest::Client,
}

pub async fn run(port: u16) -> Result<(), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let http_client = reqwest::Client::builder()
        // No global timeout: streaming responses can run for many minutes.
        // We rely on TCP-level disconnect detection instead - both at
        // connect time (connect_timeout) and mid-stream (tcp_keepalive:
        // the OS probes idle connections and surfaces a dead peer as a
        // stream error, which is how we detect stalls WITHOUT capping
        // inter-chunk gaps that thinking models legitimately stretch).
        .connect_timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("reqwest client build failed: {e}"))?;

    let state = AppState {
        sessions: SessionStore::new(),
        http_client,
    };

    let app = Router::new()
        .route("/v1/responses", post(handle_responses))
        .route("/responses", post(handle_responses))
        .route("/v1/responses/compact", post(handle_compact))
        .route("/responses/compact", post(handle_compact))
        // Claude Desktop 3P profile — Anthropic Messages API route, sibling
        // to Codex's /v1/responses. Same server, different handler.
        .route(
            "/v1/messages",
            post(crate::services::anthropic_proxy::handle_messages),
        )
        // Claude Code 3P route — same Anthropic Messages API, but a distinct
        // path so it reads its own relay (claudecode.json) and stays
        // independent of Claude Desktop. apply_claudecode points Claude Code's
        // ANTHROPIC_BASE_URL at http://127.0.0.1:<port>/claudecode, and the
        // Anthropic SDK appends /v1/messages → this route.
        .route(
            "/claudecode/v1/messages",
            post(crate::services::anthropic_proxy::handle_messages_claudecode),
        )
        // Raise request-body buffer cap above axum's 2 MiB default so
        // computer_use / large-context turns don't 413 inside the proxy
        // before the upstream sees them. See MAX_REQUEST_BODY_BYTES doc.
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            // Most common cause: another EchoBird instance is still
            // running and holding the port. We log + bail so Tauri
            // keeps starting; the proxy is reachable via that other
            // instance's listener.
            return Err(format!("bind 127.0.0.1:{port} failed: {e}"));
        }
    };

    log::info!("[CodexProxy] listening on 127.0.0.1:{port}");
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("serve failed: {e}"))
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Read the original Codex client User-Agent from the inbound request, if any.
fn client_ua_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Pass the original Codex client User-Agent through to the upstream. Some
/// relay stations / providers gate by UA (only accept the official Codex
/// client UA); we forward the real incoming UA rather than spoof a fixed
/// string, so it always matches the user's actual Codex version. No-op when
/// the client sent no UA.
fn forward_client_ua(
    req: reqwest::RequestBuilder,
    client_ua: Option<&str>,
) -> reqwest::RequestBuilder {
    match client_ua {
        Some(ua) if !ua.is_empty() => req.header(header::USER_AGENT, ua),
        _ => req,
    }
}

async fn handle_responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_ua = client_ua_from(&headers);
    // 1) Parse the request body.
    let mut req_body: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "message": e.to_string(), "code": "invalid_json" } })),
            )
                .into_response();
        }
    };

    let want_stream = req_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // 2) Read the relay file fresh. EchoBird's apply_codex writes this
    //    JSON whenever the user picks a model; we never cache it.
    let relay = match read_relay_or_error(&state.sessions, want_stream) {
        Ok(r) => r,
        Err(resp) => return *resp,
    };
    let RelayConfig {
        base_url,
        api_key,
        real_model_id,
        responses_passthrough,
    } = relay;

    // Capture the model id Codex put in the request — we'll mirror it
    // back in the SSE response (symmetric model-id deception). The real
    // provider's model id only exists between us and the upstream;
    // Codex never sees it.
    let client_model = req_body
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Opt-in tracer (ECHOBIRD_CODEX_TRACE=1) — no-op when disabled. The
    // failing request exists nowhere but this process (model-id + protocol
    // deception hide it from both Codex and the upstream), so capture the
    // boundaries here. The body clone only happens when tracing is on.
    let trace = super::trace::RequestTrace::start("responses");
    if trace.enabled() {
        trace.write_json(
            "1-codex-request",
            &json!({
                "codex_sees_model": client_model.as_deref(),
                "real_upstream_model": real_model_id.as_deref(),
                "want_stream": want_stream,
                "body": req_body.clone(),
            }),
        );
    }

    // 3) Pre-substitute req_body["model"] with the real upstream model id
    //    BEFORE calling responses_to_chat, so provider-specific shaping
    //    logic in the translator (e.g. minimax_merge for MiniMax's
    //    non-standard system-role handling) detects the actual provider,
    //    not Codex's display name like "gpt-5.5". Without this swap the
    //    is_minimax check inside responses_to_chat was always false.
    if let Some(real) = real_model_id.as_deref() {
        let current = req_body.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if !real.is_empty() && real != current {
            log::info!("[CodexProxy] Model ID rewrite: {current} → {real}");
            req_body["model"] = Value::String(real.to_string());
        }
    }

    // Responses passthrough: the upstream natively speaks Responses, so
    // forward the (model-id-rewritten) request to its /responses endpoint
    // verbatim instead of translating down to Chat. Preserves reasoning /
    // tool-call fidelity the Chat round-trip would otherwise flatten.
    if responses_passthrough {
        return forward_responses_passthrough(
            &state,
            req_body,
            &base_url,
            &api_key,
            client_model,
            client_ua.as_deref(),
            &trace,
        )
        .await;
    }

    // Remote-compaction-v2 interception (Bridge path only). Codex 0.x
    // inlines compaction into a regular /v1/responses request tagged with a
    // `compaction_trigger` input item, then demands exactly one
    // `{"type":"compaction"}` output item back (codex-rs compact_remote_v2.rs
    // `collect_compaction_output`). Translating it down to Chat yields
    // message (+ reasoning) items and zero compaction items, so Codex aborts
    // with "expected exactly one compaction output item". We synthesize the
    // single item it expects. Passthrough upstreams speak native Responses
    // and handle the trigger themselves, so this sits AFTER that branch.
    if request_has_compaction_trigger(&req_body) {
        return handle_inline_compaction(&state, req_body, client_ua.as_deref()).await;
    }

    // 4) Translate Responses → Chat. The translator uses SessionStore
    //    for previous_response_id replay + reasoning recovery.
    let chat_body = responses_to_chat(&req_body, &state.sessions);

    // 5) Build the upstream URL. Users sometimes enter the bare host
    //    without /v1; auto-add when missing so the forward lands on the
    //    standard OpenAI-compat endpoint.
    let upstream_url = normalize_upstream_url(&base_url);

    // 6) Forward. The request messages we just translated also need to
    //    persist alongside the assistant turn for future replays.
    let request_messages: Vec<Value> = chat_body
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let is_stream = chat_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let accept_header = if is_stream {
        "text/event-stream"
    } else {
        "application/json"
    };

    let upstream_req = forward_client_ua(
        state
            .http_client
            .post(&upstream_url)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
            .header(header::ACCEPT, accept_header),
        client_ua.as_deref(),
    )
    .json(&chat_body);

    if trace.enabled() {
        trace.write_json(
            "2-upstream-request",
            &json!({
                "url": upstream_url,
                "headers": { "authorization": "[REDACTED]", "accept": accept_header },
                "body": chat_body.clone(),
            }),
        );
    }

    let upstream_resp = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) => {
            log::error!("[CodexProxy] Upstream connect error: {e}");
            trace.write_json(
                "3-upstream-error",
                &json!({ "kind": "connect_error", "message": e.to_string() }),
            );
            trace.write_summary(
                real_model_id.as_deref(),
                &upstream_url,
                None,
                "connect_error",
                want_stream,
            );
            let body = json!({
                "error": {
                    "message": e.to_string(),
                    "code": "connect_error",
                }
            })
            .to_string();
            let envelope = chat_error_to_responses_error(502, Some(&body), Some(&state.sessions));
            return error_response(envelope, 502, is_stream);
        }
    };

    let status = upstream_resp.status();
    if !status.is_success() {
        // Drain the body (capped) so we can surface the upstream's error
        // verbatim. Codex renders the JSON envelope's `error.message`
        // field — users see e.g. "Invalid API key" instead of a bare 401.
        let body_text = read_capped_body(upstream_resp).await;
        log::error!(
            "[CodexProxy] Upstream {}: {}",
            status.as_u16(),
            body_text.chars().take(500).collect::<String>()
        );
        trace.write_text("3-upstream-error.txt", &body_text);
        trace.write_summary(
            real_model_id.as_deref(),
            &upstream_url,
            Some(status.as_u16()),
            "upstream_error",
            want_stream,
        );
        let envelope =
            chat_error_to_responses_error(status.as_u16(), Some(&body_text), Some(&state.sessions));
        return error_response(envelope, passthrough_status(status.as_u16()), is_stream);
    }

    // ZDR / stateless mode — Codex's request `store: false` means we
    // must not persist the conversation under our response id. Default
    // true matches OpenAI's default behavior.
    let store = req_body
        .get("store")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    trace.write_summary(
        real_model_id.as_deref(),
        &upstream_url,
        Some(status.as_u16()),
        "ok",
        want_stream,
    );

    if is_stream {
        stream_response(
            upstream_resp,
            request_messages,
            client_model,
            state.sessions.clone(),
            store,
        )
    } else {
        non_stream_response(
            upstream_resp,
            request_messages,
            client_model,
            state.sessions.clone(),
            store,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// /v1/responses/compact — server-side conversation compaction.
// ---------------------------------------------------------------------------
//
// OpenAI's Responses API exposes a `compact` endpoint that returns an
// opaque `encrypted_content` blob the model server can later decrypt to
// recover the original "latent state" of a conversation using far fewer
// tokens (see https://developers.openai.com/api/docs/guides/compaction).
// Third-party providers (DeepSeek, MiMo, etc.) have no such mechanism —
// no encryption, no decoder, no stateful continuation.
//
// To make Codex's "压缩此线程" menu button work against third-party
// upstreams, we translate the compaction request into a plain
// "summarize this conversation" Chat Completions call. The resulting
// summary text rides back as `encrypted_content`. When Codex echoes it
// in the next /v1/responses request, `process_input_items` reads the
// text field back out and uses it as a system-message body instead of
// the generic placeholder.
//
// Tradeoffs vs OpenAI's native compaction:
//   • Quality depends on the upstream model's summarization
//   • Not as token-efficient as a true encrypted latent blob
//   • But: Codex's compact button no longer 404s, and the user retains
//     a real summary of pre-compaction context for the next turn
async fn handle_compact(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_ua = client_ua_from(&headers);
    let req_body: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "message": e.to_string(), "code": "invalid_json" } })),
            )
                .into_response();
        }
    };

    let (summary, chat_usage) =
        match generate_compaction_summary(&state, req_body, false, client_ua.as_deref()).await {
            Ok(pair) => pair,
            Err(resp) => return resp,
        };

    // Build the compaction-shape Responses-API envelope. The summary
    // text rides as `encrypted_content` — see process_input_items
    // in protocol_converter.rs for the read-back path.
    let response_id = state.sessions.new_response_id();
    let envelope = json!({
        "id": response_id,
        "object": "response.compaction",
        "status": "completed",
        "output": [
            {
                "type": "compaction",
                "encrypted_content": summary,
            }
        ],
        "usage": chat_usage,
    });

    (StatusCode::OK, Json(envelope)).into_response()
}

/// Run a one-shot "summarize the conversation" Chat Completions call and
/// return `(summary_text, chat_usage)`. Shared by the legacy
/// `/v1/responses/compact` endpoint and the remote-compaction-v2 inline
/// path (`handle_inline_compaction`).
///
/// The upstream call is always non-streaming, tool-free, and length-capped:
/// compaction must be one self-contained summary, never a tool-calling turn.
/// `usage` comes back in raw Chat Completions shape — the legacy endpoint
/// passes it through, the inline path converts it to Responses shape.
///
/// `is_stream` picks how upstream failures render: the legacy endpoint wants
/// a JSON error body, the inline-v2 path an SSE one. On error returns the
/// fully-rendered `Response` for the caller to return verbatim.
async fn generate_compaction_summary(
    state: &AppState,
    mut req_body: Value,
    is_stream: bool,
    client_ua: Option<&str>,
) -> Result<(String, Value), Response> {
    let relay = match read_relay_or_error(&state.sessions, is_stream) {
        Ok(r) => r,
        Err(resp) => return Err(*resp),
    };
    let RelayConfig {
        base_url,
        api_key,
        real_model_id,
        responses_passthrough: _,
    } = relay;

    // Pre-substitute model id with real upstream model BEFORE conversion,
    // same reason as handle_responses: provider-specific shaping inside
    // responses_to_chat needs to see the real provider.
    if let Some(real) = real_model_id.as_deref() {
        let current = req_body.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if !real.is_empty() && real != current {
            log::info!("[CodexProxy] (compact) Model ID rewrite: {current} → {real}");
            req_body["model"] = Value::String(real.to_string());
        }
    }

    // Translate input → Chat messages (same path as /v1/responses). The
    // remote-compaction-v2 `compaction_trigger` sentinel carries no content
    // and is dropped by the translator.
    let mut chat_body = responses_to_chat(&req_body, &state.sessions);

    // Append a summarization instruction so the upstream model produces
    // a summary instead of a chat response. Goes as the LAST user
    // message so the model treats the prior conversation as input.
    let summary_prompt = "Please write a concise summary of the conversation above. Preserve key decisions, pending tasks, code changes, file paths, and any important context the user will need to continue this work in a follow-up turn. Output only the summary, no preamble.";
    if let Some(messages) = chat_body.get_mut("messages").and_then(|v| v.as_array_mut()) {
        messages.push(json!({ "role": "user", "content": summary_prompt }));
    }

    // Compaction must be one-shot — disable streaming, drop tools so
    // the model can't decide to call shell etc., cap output length.
    chat_body["stream"] = Value::Bool(false);
    chat_body.as_object_mut().map(|o| o.remove("tools"));
    chat_body.as_object_mut().map(|o| o.remove("tool_choice"));
    // responses_to_chat injects stream_options.include_usage when the source
    // request was streaming; compaction forces stream=false, and OpenAI-spec
    // rejects stream_options unless stream is true (strict upstreams 400). Drop it.
    chat_body
        .as_object_mut()
        .map(|o| o.remove("stream_options"));
    chat_body["max_tokens"] = json!(2048);

    let upstream_url = normalize_upstream_url(&base_url);
    let upstream_req = forward_client_ua(
        state
            .http_client
            .post(&upstream_url)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
            .header(header::ACCEPT, "application/json"),
        client_ua,
    )
    .json(&chat_body);

    let upstream_resp = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) => {
            log::error!("[CodexProxy] (compact) Upstream connect error: {e}");
            let body = json!({
                "error": { "message": e.to_string(), "code": "connect_error" }
            })
            .to_string();
            let envelope = chat_error_to_responses_error(502, Some(&body), Some(&state.sessions));
            return Err(error_response(envelope, 502, is_stream));
        }
    };

    let status = upstream_resp.status();
    if !status.is_success() {
        let body_text = read_capped_body(upstream_resp).await;
        log::error!(
            "[CodexProxy] (compact) Upstream {}: {}",
            status.as_u16(),
            body_text.chars().take(500).collect::<String>()
        );
        let envelope =
            chat_error_to_responses_error(status.as_u16(), Some(&body_text), Some(&state.sessions));
        return Err(error_response(
            envelope,
            passthrough_status(status.as_u16()),
            is_stream,
        ));
    }

    let resp_json: Value = match upstream_resp.json().await {
        Ok(v) => v,
        Err(e) => {
            log::error!("[CodexProxy] (compact) Upstream JSON parse failed: {e}");
            let envelope = chat_error_to_responses_error(
                502,
                Some(&format!(
                    r#"{{"error":{{"message":"{e}","code":"parse_error"}}}}"#
                )),
                Some(&state.sessions),
            );
            return Err(error_response(envelope, 502, is_stream));
        }
    };

    // Extract the summary text from the upstream response.
    let summary = resp_json
        .get("choices")
        .and_then(|v| v.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if summary.is_empty() {
        log::warn!("[CodexProxy] (compact) Upstream returned empty summary content");
    }

    let chat_usage = resp_json.get("usage").cloned().unwrap_or(Value::Null);
    Ok((summary, chat_usage))
}

/// True when this `/v1/responses` request is actually a remote-compaction-v2
/// turn: Codex marks it by appending a bare `{"type":"compaction_trigger"}`
/// item to `input`. Codex 0.x retired the standalone
/// `/v1/responses/compact` endpoint in favor of this inline form.
fn request_has_compaction_trigger(req_body: &Value) -> bool {
    req_body
        .get("input")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(|v| v.as_str()) == Some("compaction_trigger"))
        })
        .unwrap_or(false)
}

/// Handle a remote-compaction-v2 inline request. Codex streams a normal
/// `/v1/responses` call (tagged with `compaction_trigger`) and then requires
/// the response to contain EXACTLY ONE `{"type":"compaction"}` output item —
/// see `collect_compaction_output` in codex-rs `compact_remote_v2.rs`, which
/// aborts with "expected exactly one compaction output item" when the count
/// isn't 1. A normal Chat round-trip yields message (+ reasoning) items and
/// zero compaction items, so we intercept and synthesize the single
/// compaction item Codex expects, carrying a plain-text summary in
/// `encrypted_content` (read back by `process_input_items`). Crucially we
/// emit NO reasoning item — that would make the count 2 and trip the same
/// assertion (the exact failure third-party bridges hit with thinking models).
async fn handle_inline_compaction(
    state: &AppState,
    req_body: Value,
    client_ua: Option<&str>,
) -> Response {
    let (summary, chat_usage) =
        match generate_compaction_summary(state, req_body, true, client_ua).await {
            Ok(pair) => pair,
            Err(resp) => return resp,
        };

    let response_id = state.sessions.new_response_id();
    // Match Codex's `ResponseItem::Compaction` shape EXACTLY — `type` +
    // `encrypted_content`, nothing else. collect_compaction_output
    // deserializes this item; a stray `id` risks a deny_unknown_fields reject
    // (dropping the count back to 0), and Codex never keys compaction items
    // by id anyway.
    let compaction_item = json!({
        "type": "compaction",
        "encrypted_content": summary,
    });
    let usage = chat_usage_to_responses_usage(Some(&chat_usage));
    let events = build_compaction_sse_events(&response_id, compaction_item, usage);
    let stream = tokio_stream::iter(
        events
            .into_iter()
            .map(|e| Ok::<Event, Infallible>(sse_event_to_axum(&e))),
    );
    Sse::new(stream).into_response()
}

/// SSE sequence for an inline-compaction response: created → in_progress →
/// output_item.done(compaction) → completed. Codex's
/// `collect_compaction_output` only consumes `OutputItemDone` + `Completed`,
/// but the created/in_progress preamble matches the happy-path opening
/// contract its stream client expects (mirrors `build_error_sse_events`).
fn build_compaction_sse_events(
    response_id: &str,
    compaction_item: Value,
    usage: Value,
) -> Vec<SseEvent> {
    vec![
        SseEvent::new(
            "response.created",
            json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                    "output": [],
                },
            }),
        ),
        SseEvent::new(
            "response.in_progress",
            json!({
                "type": "response.in_progress",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                },
            }),
        ),
        SseEvent::new(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": compaction_item.clone(),
            }),
        ),
        SseEvent::new(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "completed",
                    "output": [compaction_item],
                    "usage": usage,
                },
            }),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct RelayConfig {
    base_url: String,
    api_key: String,
    real_model_id: Option<String>,
    /// `responsesPassthrough` from the relay file — when true the upstream
    /// natively speaks Responses, so the handler forwards verbatim to
    /// `/responses` instead of translating to Chat. Codex-only.
    responses_passthrough: bool,
}

/// Pull the relay file. On miss / malformed / missing required fields,
/// return a fully-rendered error response (SSE or JSON) instead of a
/// RelayConfig. We box the Err variant because axum's `Response` is
/// ~hundreds of bytes — `Result<small, big>` triggers clippy's
/// `result_large_err` lint, and boxing keeps the happy-path size small.
fn read_relay_or_error(
    sessions: &SessionStore,
    want_stream: bool,
) -> Result<RelayConfig, Box<Response>> {
    let relay_dir = match default_relay_dir() {
        Some(d) => d,
        None => {
            let envelope = chat_error_to_responses_error(
                503,
                Some(
                    &json!({
                        "error": {
                            "message": "Could not resolve home directory to read EchoBird relay file.",
                            "code": "no_home_dir",
                        }
                    })
                    .to_string(),
                ),
                Some(sessions),
            );
            return Err(Box::new(error_response(envelope, 503, want_stream)));
        }
    };
    let relay_path = relay_dir.join(RELAY_FILENAME);

    let parsed = match read_echobird_relay(&relay_path) {
        Some(v) => v,
        None => {
            let envelope = chat_error_to_responses_error(
                503,
                Some(
                    &json!({
                        "error": {
                            "message": "No active model configured in EchoBird. Open EchoBird and select a model.",
                            "code": "no_active_model",
                        }
                    })
                    .to_string(),
                ),
                Some(sessions),
            );
            return Err(Box::new(error_response(envelope, 503, want_stream)));
        }
    };

    let base_url = parsed
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let api_key = parsed
        .get("apiKey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if base_url.is_empty() || api_key.is_empty() {
        let envelope = chat_error_to_responses_error(
            503,
            Some(
                &json!({
                    "error": {
                        "message": "EchoBird relay file is missing baseUrl or apiKey — re-apply a model.",
                        "code": "incomplete_relay",
                    }
                })
                .to_string(),
            ),
            Some(sessions),
        );
        return Err(Box::new(error_response(envelope, 503, want_stream)));
    }

    let real_model_id = parsed
        .get("actualModel")
        .and_then(|v| v.as_str())
        .or_else(|| parsed.get("modelName").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let responses_passthrough = parsed
        .get("responsesPassthrough")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(RelayConfig {
        base_url,
        api_key,
        real_model_id,
        responses_passthrough,
    })
}

/// Strip a trailing slash from the user's `baseUrl` and auto-append `/v1`
/// when no `/v<n>` version segment is already present (many users enter the
/// bare host). Returns the versioned base WITHOUT an endpoint suffix —
/// shared by the Chat (`normalize_upstream_url`) and Responses-passthrough
/// (`normalize_responses_url`) endpoint builders.
fn ensure_versioned_base(base_url: &str) -> String {
    let mut base = base_url.trim_end_matches('/').to_string();
    // Look for `/v<digit>` at the end. If absent, append `/v1`.
    let has_version = base
        .rsplit('/')
        .next()
        .map(|seg| seg.starts_with('v') && seg[1..].chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false);
    if !has_version {
        base.push_str("/v1");
    }
    base
}

/// Build the final Chat Completions POST URL from the user's `baseUrl`.
fn normalize_upstream_url(base_url: &str) -> String {
    let base = ensure_versioned_base(base_url);
    // OpenAI's official endpoint accepts our request shape verbatim;
    // log for visibility but no behavior change.
    if is_openai(&base) {
        log::debug!("[CodexProxy] Routing to official OpenAI endpoint");
    }
    format!("{base}/chat/completions")
}

/// Build the upstream's native Responses POST URL — used by the passthrough
/// path, which forwards verbatim rather than translating to Chat.
fn normalize_responses_url(base_url: &str) -> String {
    format!("{}/responses", ensure_versioned_base(base_url))
}

/// Map upstream HTTP status onto a sensible client-facing status. We
/// keep 4xx/5xx codes verbatim so logs and any non-Codex consumer see
/// the truth; anything outside that range collapses to 502.
fn passthrough_status(code: u16) -> u16 {
    if !(400..=599).contains(&code) {
        502
    } else {
        code
    }
}

/// Drain a response body up to UPSTREAM_ERROR_BODY_CAP bytes. Used on
/// non-200 upstream responses so we can extract `error.message`.
async fn read_capped_body(resp: reqwest::Response) -> String {
    let mut buf = Vec::with_capacity(2048);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(_) => break,
        };
        let remaining = UPSTREAM_ERROR_BODY_CAP.saturating_sub(buf.len());
        if remaining == 0 {
            break;
        }
        let take = chunk.len().min(remaining);
        buf.extend_from_slice(&chunk[..take]);
        if buf.len() >= UPSTREAM_ERROR_BODY_CAP {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Wrap a Responses-shape error envelope into an HTTP response, choosing
/// between an SSE single-event stream (stream clients) or JSON (non-stream).
///
/// SSE path emits three events back-to-back: `response.created`,
/// `response.in_progress`, `response.failed`. The created+in_progress
/// preamble matches what the happy path sends before any output, so
/// Codex's stream client sees the same opening contract whether the
/// turn ends in success or failure. The body flows over axum's `Sse`
/// adapter (chunked transfer encoding) — not a fixed-Content-Length
/// String — so the connection lifecycle matches a real streaming turn.
fn error_response(envelope: Value, http_status: u16, is_stream: bool) -> Response {
    let status_code = StatusCode::from_u16(http_status).unwrap_or(StatusCode::BAD_GATEWAY);
    if is_stream {
        let response_id = envelope
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("resp_err")
            .to_string();
        let events = build_error_sse_events(&envelope, &response_id);
        // Always 200 OK on the SSE channel; the error sits in the
        // payload. Codex inspects the JSON envelope, not the HTTP code.
        let stream = tokio_stream::iter(
            events
                .into_iter()
                .map(|e| Ok::<Event, Infallible>(sse_event_to_axum(&e))),
        );
        Sse::new(stream).into_response()
    } else {
        (status_code, Json(envelope)).into_response()
    }
}

/// Build the three-event SSE envelope for the error path. Matches the
/// happy-path opening contract (response.created → response.in_progress)
/// so Codex's stream client doesn't sit waiting for the missing preamble
/// before parsing the failure.
fn build_error_sse_events(envelope: &Value, response_id: &str) -> Vec<SseEvent> {
    vec![
        SseEvent::new(
            "response.created",
            json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                    "output": [],
                },
            }),
        ),
        SseEvent::new(
            "response.in_progress",
            json!({
                "type": "response.in_progress",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                },
            }),
        ),
        SseEvent::new(
            "response.failed",
            json!({
                "type": "response.failed",
                "response": envelope,
            }),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Streaming path
// ---------------------------------------------------------------------------

/// Outcome of pulling one chunk from the upstream stream with the
/// disconnect-safe timeout policy in `next_chunk_with_policy` applied.
#[derive(Debug)]
enum NextChunkResult<T, E> {
    /// A chunk arrived.
    Item(T),
    /// The stream surfaced an error mid-read. With TCP keepalive enabled
    /// this is how we learn about dead peers - the OS probe fails and
    /// reqwest reports it here instead of the stream hanging silently.
    Error(E),
    /// Clean end of stream.
    Eof,
    /// Only yielded on the first read: the upstream never delivered any
    /// bytes within `UPSTREAM_FIRST_BYTE_TIMEOUT`. Never produced for
    /// subsequent reads - see `next_chunk_with_policy`.
    FirstByteTimeout,
}

/// Pull the next item from `stream` with the disconnect policy:
///
/// - **First chunk** (`is_first = true`): bounded by `first_byte_timeout`
///   (callers pass `UPSTREAM_FIRST_BYTE_TIMEOUT` in production), so a
///   totally unresponsive upstream can't pin a task + connection forever.
/// - **Subsequent chunks** (`is_first = false`): no timeout. A
///   stalled-but-alive connection is detected by TCP keepalive on the
///   http client, which surfaces here as `Error`. This deliberately does
///   NOT cap inter-chunk gaps: thinking models pause for many minutes
///   between chunks while reasoning, and capping those gaps was the #1
///   cause of mid-turn disconnects (the stream got `fail()`ed on a long
///   thinking pause and Codex collapsed the turn - "chat a bit then it
///   folds").
///
/// Generic over the item/error types so the loop logic is unit-testable
/// without a real `reqwest` response.
async fn next_chunk_with_policy<S, T, E>(
    stream: &mut S,
    is_first: bool,
    first_byte_timeout: Duration,
) -> NextChunkResult<T, E>
where
    S: Stream<Item = Result<T, E>> + Unpin,
{
    if is_first {
        match tokio::time::timeout(first_byte_timeout, stream.next()).await {
            Ok(Some(Ok(item))) => NextChunkResult::Item(item),
            Ok(Some(Err(e))) => NextChunkResult::Error(e),
            Ok(None) => NextChunkResult::Eof,
            Err(_elapsed) => NextChunkResult::FirstByteTimeout,
        }
    } else {
        match stream.next().await {
            Some(Ok(item)) => NextChunkResult::Item(item),
            Some(Err(e)) => NextChunkResult::Error(e),
            None => NextChunkResult::Eof,
        }
    }
}

/// Start an async task that consumes the upstream byte stream, drives a
/// StreamState through it, and pipes the emitted SseEvents into a
/// channel. Returns the channel as an SSE response.
fn stream_response(
    upstream_resp: reqwest::Response,
    request_messages: Vec<Value>,
    client_model: Option<String>,
    sessions: SessionStore,
    store: bool,
) -> Response {
    // Channel capacity is small — SSE events are tiny and the consumer
    // (axum's writer) drains them as fast as the TCP socket allows.
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let mut state = StreamState::new(&sessions, client_model, request_messages);
        state.set_store(store);
        state.start();
        // Drain initial response.created / response.in_progress events.
        if !forward_events(&mut state, &tx).await {
            return;
        }

        let mut bytes_stream = upstream_resp.bytes_stream();
        // Carry-over buffer for bytes that landed mid-codepoint. TCP
        // chunks can split a multi-byte UTF-8 codepoint, so a naive
        // `from_utf8` on each chunk would fail on Chinese/emoji content
        // at packet boundaries. We feed only the validated prefix and
        // carry the trailing partial bytes into the next chunk.
        let mut carry: Vec<u8> = Vec::new();
        let mut is_first = true;
        loop {
            match next_chunk_with_policy(&mut bytes_stream, is_first, UPSTREAM_FIRST_BYTE_TIMEOUT)
                .await
            {
                NextChunkResult::Item(b) => {
                    is_first = false;
                    let bytes: &[u8] = if carry.is_empty() {
                        &b
                    } else {
                        carry.extend_from_slice(&b);
                        carry.as_slice()
                    };
                    let (valid_prefix, rest): (&str, &[u8]) = match std::str::from_utf8(bytes) {
                        Ok(s) => (s, &[][..]),
                        Err(e) => {
                            let up_to = e.valid_up_to();
                            let prefix = unsafe {
                                // Safe: `valid_up_to` returns a guaranteed UTF-8 prefix length.
                                std::str::from_utf8_unchecked(&bytes[..up_to])
                            };
                            if let Some(_invalid_len) = e.error_len() {
                                // A truly invalid sequence — not a boundary split.
                                // SSE bodies must be UTF-8 per spec; surface as error.
                                state.fail(
                                    "Upstream sent non-UTF-8 bytes",
                                    "upstream_encoding_error",
                                );
                                let _ = forward_events(&mut state, &tx).await;
                                return;
                            }
                            // Trailing bytes form an incomplete codepoint — keep them.
                            (prefix, &bytes[up_to..])
                        }
                    };
                    if !valid_prefix.is_empty() {
                        state.feed_chunk(valid_prefix);
                    }
                    // Reset carry to whatever didn't validate yet.
                    let leftover = rest.to_vec();
                    carry = leftover;
                    if !forward_events(&mut state, &tx).await {
                        return;
                    }
                }
                NextChunkResult::Error(e) => {
                    state.fail(
                        &format!("Upstream stream error: {e}"),
                        "upstream_stream_error",
                    );
                    let _ = forward_events(&mut state, &tx).await;
                    return;
                }
                NextChunkResult::Eof => {
                    // Clean EOF — break out so finish() runs below.
                    break;
                }
                NextChunkResult::FirstByteTimeout => {
                    // Upstream accepted the request but never started
                    // streaming. Close the stream so Codex gets a
                    // response.failed instead of hanging forever.
                    state.fail(
                        &format!(
                            "Upstream did not start streaming within {}s",
                            UPSTREAM_FIRST_BYTE_TIMEOUT.as_secs()
                        ),
                        "upstream_first_byte_timeout",
                    );
                    let _ = forward_events(&mut state, &tx).await;
                    return;
                }
            }
        }

        // Upstream closed cleanly. Drive `finish()` so we persist
        // history + emit response.completed (or response.incomplete).
        state.finish(&sessions);
        let _ = forward_events(&mut state, &tx).await;
    });

    let stream = ReceiverStream::new(rx);
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

/// Pull pending events out of `state` and shove them down the channel.
/// Returns false if the receiver has dropped (client disconnected).
async fn forward_events(
    state: &mut StreamState,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> bool {
    for ev in state.take_events() {
        let axum_event = sse_event_to_axum(&ev);
        if tx.send(Ok(axum_event)).await.is_err() {
            return false;
        }
    }
    true
}

fn sse_event_to_axum(ev: &SseEvent) -> Event {
    // axum's Event::default().data() expects a String. We serialize the
    // Value ourselves rather than letting axum stringify because
    // serde_json::to_string preserves field order on Map values.
    Event::default()
        .event(ev.event.clone())
        .data(ev.data.to_string())
}

/// Stream-as-trait export so we can name the return type in error_response.
#[allow(dead_code)]
fn _assert_event_stream_is_stream<S>(_: S)
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
}

// ---------------------------------------------------------------------------
// Non-stream path
// ---------------------------------------------------------------------------

async fn non_stream_response(
    upstream_resp: reqwest::Response,
    request_messages: Vec<Value>,
    client_model: Option<String>,
    sessions: SessionStore,
    store: bool,
) -> Response {
    let body_text = match upstream_resp.text().await {
        Ok(t) => t,
        Err(e) => {
            log::error!("[CodexProxy] Upstream body read failed: {e}");
            let envelope =
                chat_error_to_responses_error(502, Some(&e.to_string()), Some(&sessions));
            return (StatusCode::BAD_GATEWAY, Json(envelope)).into_response();
        }
    };
    let chat_resp: Value = match serde_json::from_str(&body_text) {
        Ok(v) => v,
        Err(e) => {
            log::error!("[CodexProxy] Upstream response not valid JSON: {e}");
            let envelope = chat_error_to_responses_error(
                502,
                Some(
                    &json!({
                        "error": {
                            "message": format!("Upstream returned non-JSON body: {e}"),
                            "code": "upstream_invalid_json",
                        }
                    })
                    .to_string(),
                ),
                Some(&sessions),
            );
            return (StatusCode::BAD_GATEWAY, Json(envelope)).into_response();
        }
    };
    let resp = chat_to_responses_non_stream(
        &chat_resp,
        request_messages,
        &sessions,
        client_model.as_deref(),
        store,
    );
    (StatusCode::OK, Json(resp)).into_response()
}

// ---------------------------------------------------------------------------
// Responses passthrough path
// ---------------------------------------------------------------------------
//
// When the relay file flags `responsesPassthrough: true`, the upstream
// natively speaks the Responses protocol. We forward the request (model id
// already rewritten by the caller) to the upstream's `/responses` endpoint
// verbatim and relay the SSE / JSON straight back, only swapping the model
// id back to Codex's display id. No Chat round-trip → reasoning and
// tool-call shapes survive intact (the whole reason this mode exists).

async fn forward_responses_passthrough(
    state: &AppState,
    mut req_body: Value,
    base_url: &str,
    api_key: &str,
    client_model: Option<String>,
    client_ua: Option<&str>,
    trace: &super::trace::RequestTrace,
) -> Response {
    // Codex thinks it's gpt-5.5 (model-id deception), so it emits gpt-family
    // reasoning efforts — `minimal` / `xhigh` — that most third-party Responses
    // endpoints reject (their enum caps at the legacy {low,medium,high}; e.g.
    // MiMo 400s on `xhigh`). Clamp to the nearest legacy bucket before
    // forwarding. Mirrors the normalization the Chat translator already does.
    let clamped_effort = req_body
        .get("reasoning")
        .and_then(|r| r.get("effort"))
        .and_then(|e| e.as_str())
        .and_then(|effort| match effort {
            "minimal" => Some("low"),
            "xhigh" => Some("high"),
            _ => None,
        });
    if let Some(c) = clamped_effort {
        req_body["reasoning"]["effort"] = Value::String(c.to_string());
    }

    let is_stream = req_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let upstream_url = normalize_responses_url(base_url);
    let accept_header = if is_stream {
        "text/event-stream"
    } else {
        "application/json"
    };

    let upstream_req = forward_client_ua(
        state
            .http_client
            .post(&upstream_url)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
            .header(header::ACCEPT, accept_header),
        client_ua,
    )
    .json(&req_body);

    if trace.enabled() {
        trace.write_json(
            "2-upstream-request",
            &json!({
                "url": upstream_url,
                "wire": "responses-passthrough",
                "headers": { "authorization": "[REDACTED]", "accept": accept_header },
                "body": req_body.clone(),
            }),
        );
    }

    let upstream_resp = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) => {
            log::error!("[CodexProxy] Passthrough upstream connect error: {e}");
            trace.write_json(
                "3-upstream-error",
                &json!({ "kind": "connect_error", "message": e.to_string() }),
            );
            trace.write_summary(
                req_body.get("model").and_then(|v| v.as_str()),
                &upstream_url,
                None,
                "connect_error",
                is_stream,
            );
            let body = json!({ "error": { "message": e.to_string(), "code": "connect_error" } })
                .to_string();
            let envelope = chat_error_to_responses_error(502, Some(&body), Some(&state.sessions));
            return error_response(envelope, 502, is_stream);
        }
    };

    let status = upstream_resp.status();
    if !status.is_success() {
        let body_text = read_capped_body(upstream_resp).await;
        log::error!(
            "[CodexProxy] Passthrough upstream {}: {}",
            status.as_u16(),
            body_text.chars().take(500).collect::<String>()
        );
        trace.write_text("3-upstream-error.txt", &body_text);
        trace.write_summary(
            req_body.get("model").and_then(|v| v.as_str()),
            &upstream_url,
            Some(status.as_u16()),
            "upstream_error",
            is_stream,
        );
        let envelope =
            chat_error_to_responses_error(status.as_u16(), Some(&body_text), Some(&state.sessions));
        return error_response(envelope, passthrough_status(status.as_u16()), is_stream);
    }

    trace.write_summary(
        req_body.get("model").and_then(|v| v.as_str()),
        &upstream_url,
        Some(status.as_u16()),
        "ok",
        is_stream,
    );

    if is_stream {
        stream_passthrough(upstream_resp, client_model)
    } else {
        non_stream_passthrough(upstream_resp, client_model).await
    }
}

/// Relay an upstream Responses SSE stream straight back to Codex, rewriting
/// only the model id inside each `data:` JSON line (real → Codex display id)
/// so Codex never sees the real upstream model. All other frames pass
/// through verbatim. We emit raw `text/event-stream` bytes via
/// `Body::from_stream` rather than axum's `Sse` adapter: the upstream
/// already supplies the SSE framing, and re-wrapping through `Sse` would
/// double-encode every event.
fn stream_passthrough(upstream_resp: reqwest::Response, client_model: Option<String>) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(32);

    tokio::spawn(async move {
        let mut bytes_stream = upstream_resp.bytes_stream();
        // UTF-8 carry for codepoints split across TCP chunks; line buffer so
        // we only rewrite whole `data:` lines (a model id could straddle a
        // chunk boundary otherwise).
        let mut carry: Vec<u8> = Vec::new();
        let mut line_buf = String::new();
        let mut is_first = true;
        loop {
            match next_chunk_with_policy(&mut bytes_stream, is_first, UPSTREAM_FIRST_BYTE_TIMEOUT)
                .await
            {
                NextChunkResult::Item(b) => {
                    is_first = false;
                    let bytes: &[u8] = if carry.is_empty() {
                        &b
                    } else {
                        carry.extend_from_slice(&b);
                        carry.as_slice()
                    };
                    let (valid_prefix, rest): (&str, &[u8]) = match std::str::from_utf8(bytes) {
                        Ok(s) => (s, &[][..]),
                        Err(e) => {
                            let up_to = e.valid_up_to();
                            // Safe: valid_up_to() is a guaranteed UTF-8 prefix length.
                            let prefix = unsafe { std::str::from_utf8_unchecked(&bytes[..up_to]) };
                            if e.error_len().is_some() {
                                let _ = tx
                                    .send(Ok(Bytes::from(sse_failed_event(
                                        "Upstream sent non-UTF-8 bytes",
                                        "upstream_encoding_error",
                                    ))))
                                    .await;
                                return;
                            }
                            (prefix, &bytes[up_to..])
                        }
                    };
                    line_buf.push_str(valid_prefix);
                    carry = rest.to_vec();
                    while let Some(nl) = line_buf.find('\n') {
                        let line: String = line_buf.drain(..=nl).collect();
                        let out = rewrite_sse_model_line(&line, client_model.as_deref());
                        if tx.send(Ok(Bytes::from(out))).await.is_err() {
                            return;
                        }
                    }
                }
                NextChunkResult::Error(e) => {
                    log::error!("[CodexProxy] Passthrough stream error: {e}");
                    let _ = tx
                        .send(Ok(Bytes::from(sse_failed_event(
                            &format!("Upstream stream error: {e}"),
                            "upstream_stream_error",
                        ))))
                        .await;
                    return;
                }
                NextChunkResult::Eof => {
                    if !line_buf.is_empty() {
                        let out = rewrite_sse_model_line(&line_buf, client_model.as_deref());
                        let _ = tx.send(Ok(Bytes::from(out))).await;
                    }
                    break;
                }
                NextChunkResult::FirstByteTimeout => {
                    log::error!("[CodexProxy] Passthrough upstream never started streaming");
                    let _ = tx
                        .send(Ok(Bytes::from(sse_failed_event(
                            &format!(
                                "Upstream did not start streaming within {}s",
                                UPSTREAM_FIRST_BYTE_TIMEOUT.as_secs()
                            ),
                            "upstream_first_byte_timeout",
                        ))))
                        .await;
                    return;
                }
            }
        }
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Non-streaming passthrough: read the upstream Responses JSON, swap the
/// model id back to Codex's display id, and return it as-is.
async fn non_stream_passthrough(
    upstream_resp: reqwest::Response,
    client_model: Option<String>,
) -> Response {
    let body_text = match upstream_resp.text().await {
        Ok(t) => t,
        Err(e) => {
            log::error!("[CodexProxy] Passthrough body read failed: {e}");
            let envelope = chat_error_to_responses_error(502, Some(&e.to_string()), None);
            return (StatusCode::BAD_GATEWAY, Json(envelope)).into_response();
        }
    };
    let mut v: Value = match serde_json::from_str(&body_text) {
        Ok(v) => v,
        // Upstream returned non-JSON on a 2xx — pass it through untouched
        // rather than masking it; the model-id swap is best-effort.
        Err(_) => {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                body_text,
            )
                .into_response();
        }
    };
    if let Some(client) = client_model.as_deref() {
        swap_model_to_client(&mut v, client);
    }
    (StatusCode::OK, Json(v)).into_response()
}

/// Force `model` and `response.model` (when present, string-typed and not
/// already equal) to `client` — the symmetric model-id deception, so Codex
/// only ever sees its own display id, never the real upstream id. Returns
/// whether anything changed.
fn swap_model_to_client(v: &mut Value, client: &str) -> bool {
    let mut changed = false;
    if let Some(m) = v.get_mut("model") {
        if m.is_string() && m.as_str() != Some(client) {
            *m = Value::String(client.to_string());
            changed = true;
        }
    }
    if let Some(resp) = v.get_mut("response") {
        if let Some(m) = resp.get_mut("model") {
            if m.is_string() && m.as_str() != Some(client) {
                *m = Value::String(client.to_string());
                changed = true;
            }
        }
    }
    changed
}

/// Rewrite the model id inside one SSE line. Only `data:` lines carrying a
/// JSON object are touched; `event:` lines, blank separators and
/// `data: [DONE]` pass through unchanged. Always returns a `\n`-terminated
/// line so the SSE framing stays intact.
fn rewrite_sse_model_line(line: &str, client_model: Option<&str>) -> String {
    let Some(client) = client_model else {
        return line.to_string();
    };
    let content = line.trim_end_matches(['\r', '\n']);
    let Some(rest) = content.strip_prefix("data:") else {
        return line.to_string();
    };
    let payload = rest.trim_start();
    if payload.is_empty() || payload == "[DONE]" {
        return line.to_string();
    }
    let Ok(mut v) = serde_json::from_str::<Value>(payload) else {
        return line.to_string();
    };
    if !swap_model_to_client(&mut v, client) {
        return line.to_string();
    }
    format!("data: {v}\n")
}

/// Build a standalone `response.failed` SSE event (used for mid-stream
/// passthrough errors, where the `response.created` preamble already went
/// out with the upstream's first frames).
fn sse_failed_event(message: &str, code: &str) -> String {
    format!(
        "event: response.failed\ndata: {}\n\n",
        json!({
            "type": "response.failed",
            "response": { "error": { "message": message, "code": code } }
        })
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_compaction_trigger_in_input() {
        let with = json!({
            "input": [
                { "type": "message", "role": "user", "content": "x" },
                { "type": "compaction_trigger" },
            ],
        });
        assert!(request_has_compaction_trigger(&with));

        let without = json!({
            "input": [{ "type": "message", "role": "user", "content": "x" }],
        });
        assert!(!request_has_compaction_trigger(&without));

        // Missing or non-array input must not panic or false-positive.
        assert!(!request_has_compaction_trigger(&json!({})));
        assert!(!request_has_compaction_trigger(&json!({ "input": "hi" })));
    }

    #[test]
    fn compaction_sse_carries_exactly_one_compaction_item() {
        // Mirrors codex-rs `collect_compaction_output`: it counts output
        // items of type "compaction" across the stream and aborts unless the
        // count is exactly 1. A reasoning item anywhere would reproduce the
        // upstream third-party-bridge bug (got 0 from 2 output items).
        let item = json!({
            "type": "compaction",
            "encrypted_content": "summary text",
        });
        let usage = json!({ "input_tokens": 10, "output_tokens": 5 });
        let events = build_compaction_sse_events("resp_1", item, usage);

        // Exactly one OutputItemDone, carrying the compaction summary.
        let done: Vec<_> = events
            .iter()
            .filter(|e| e.event == "response.output_item.done")
            .collect();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].data["item"]["type"], "compaction");
        assert_eq!(done[0].data["item"]["encrypted_content"], "summary text");

        // Completed envelope holds the same single compaction item, no reasoning.
        let completed: Vec<_> = events
            .iter()
            .filter(|e| e.event == "response.completed")
            .collect();
        assert_eq!(completed.len(), 1);
        let output = completed[0].data["response"]["output"].as_array().unwrap();
        let compaction_count = output
            .iter()
            .filter(|it| it["type"] == "compaction")
            .count();
        assert_eq!(compaction_count, 1);
        assert!(!output.iter().any(|it| it["type"] == "reasoning"));

        // Opening contract present so Codex's stream client initializes.
        assert!(events.iter().any(|e| e.event == "response.created"));
    }

    #[test]
    fn normalize_url_appends_v1_when_missing() {
        assert_eq!(
            normalize_upstream_url("https://api.deepseek.com"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_url_strips_trailing_slash() {
        assert_eq!(
            normalize_upstream_url("https://api.deepseek.com/"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_url_preserves_existing_v1() {
        assert_eq!(
            normalize_upstream_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_url_preserves_existing_v2() {
        assert_eq!(
            normalize_upstream_url("https://api.example.com/v2"),
            "https://api.example.com/v2/chat/completions"
        );
    }

    #[test]
    fn normalize_url_does_not_double_append_v1() {
        // Trailing slash on /v1 was historically a source of /v1//v1.
        assert_eq!(
            normalize_upstream_url("https://api.deepseek.com/v1/"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    // ---- Responses passthrough ----

    #[test]
    fn normalize_responses_url_appends_v1_and_endpoint() {
        assert_eq!(
            normalize_responses_url("https://api.example.com"),
            "https://api.example.com/v1/responses"
        );
    }

    #[test]
    fn normalize_responses_url_preserves_existing_version() {
        assert_eq!(
            normalize_responses_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/responses"
        );
        // Trailing slash must not double the version segment.
        assert_eq!(
            normalize_responses_url("https://api.example.com/v1/"),
            "https://api.example.com/v1/responses"
        );
    }

    #[test]
    fn swap_model_swaps_top_level_and_nested() {
        let mut v = json!({
            "model": "qwen-real",
            "response": { "model": "qwen-real", "id": "resp_1" }
        });
        assert!(swap_model_to_client(&mut v, "gpt-5.5"));
        assert_eq!(v["model"], "gpt-5.5");
        assert_eq!(v["response"]["model"], "gpt-5.5");
        // Unrelated fields untouched.
        assert_eq!(v["response"]["id"], "resp_1");
    }

    #[test]
    fn swap_model_is_noop_when_already_client_or_absent() {
        let mut already = json!({ "model": "gpt-5.5" });
        assert!(!swap_model_to_client(&mut already, "gpt-5.5"));
        let mut absent = json!({ "type": "response.output_text.delta", "delta": "hi" });
        assert!(!swap_model_to_client(&mut absent, "gpt-5.5"));
    }

    #[test]
    fn rewrite_sse_line_swaps_model_in_data() {
        let line = "data: {\"type\":\"response.created\",\"response\":{\"model\":\"qwen-real\"}}\n";
        let out = rewrite_sse_model_line(line, Some("gpt-5.5"));
        assert!(out.contains("\"model\":\"gpt-5.5\""), "got: {out}");
        assert!(!out.contains("qwen-real"), "got: {out}");
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn rewrite_sse_line_passes_event_done_and_blank_through() {
        // event: lines, the [DONE] sentinel and blank separators are verbatim.
        assert_eq!(
            rewrite_sse_model_line("event: response.created\n", Some("gpt-5.5")),
            "event: response.created\n"
        );
        assert_eq!(
            rewrite_sse_model_line("data: [DONE]\n", Some("gpt-5.5")),
            "data: [DONE]\n"
        );
        assert_eq!(rewrite_sse_model_line("\n", Some("gpt-5.5")), "\n");
    }

    #[test]
    fn rewrite_sse_line_leaves_unrelated_data_and_no_client_untouched() {
        // No model field → returned verbatim so deltas keep their exact shape.
        let delta = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n";
        assert_eq!(rewrite_sse_model_line(delta, Some("gpt-5.5")), delta);
        // No client model → nothing to swap to → verbatim.
        let modelled = "data: {\"model\":\"qwen-real\"}\n";
        assert_eq!(rewrite_sse_model_line(modelled, None), modelled);
    }

    #[test]
    fn passthrough_status_keeps_4xx_5xx() {
        assert_eq!(passthrough_status(401), 401);
        assert_eq!(passthrough_status(429), 429);
        assert_eq!(passthrough_status(500), 500);
        assert_eq!(passthrough_status(503), 503);
    }

    #[test]
    fn passthrough_status_clamps_unknown() {
        assert_eq!(passthrough_status(200), 502);
        assert_eq!(passthrough_status(0), 502);
        assert_eq!(passthrough_status(900), 502);
    }

    #[test]
    fn error_response_stream_returns_sse_envelope() {
        let envelope = json!({ "id": "resp_x", "status": "failed" });
        let resp = error_response(envelope, 503, true);
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert!(ct.starts_with("text/event-stream"), "got: {ct}");
    }

    #[test]
    fn error_response_non_stream_returns_json_with_status() {
        let envelope = json!({ "id": "resp_x", "status": "failed" });
        let resp = error_response(envelope, 503, false);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert!(ct.starts_with("application/json"), "got: {ct}");
    }

    #[test]
    fn build_error_sse_events_emits_three_event_preamble_then_failure() {
        let envelope = json!({
            "id": "resp_abc",
            "object": "response",
            "status": "failed",
            "error": { "code": "no_active_model", "message": "no model" },
            "output": [],
        });
        let events = build_error_sse_events(&envelope, "resp_abc");
        assert_eq!(events.len(), 3, "expected 3 events, got {}", events.len());
        assert_eq!(events[0].event, "response.created");
        assert_eq!(events[1].event, "response.in_progress");
        assert_eq!(events[2].event, "response.failed");
        // Preamble events must carry status=in_progress (matches happy path)
        assert_eq!(events[0].data["response"]["status"], "in_progress");
        assert_eq!(events[1].data["response"]["status"], "in_progress");
        // The failed event carries the caller-supplied envelope verbatim
        assert_eq!(events[2].data["response"], envelope);
        // All three reference the same response_id so Codex can stitch them
        assert_eq!(events[0].data["response"]["id"], "resp_abc");
        assert_eq!(events[1].data["response"]["id"], "resp_abc");
    }

    #[test]
    fn sse_event_to_axum_preserves_event_name() {
        let ev = SseEvent::new("response.created", json!({"hello":"world"}));
        let axum = sse_event_to_axum(&ev);
        // Event doesn't expose its name accessor publicly; assert
        // round-trip via the wire format.
        let formatted = format!("{axum:?}");
        assert!(formatted.contains("response.created"), "got: {formatted}");
    }

    // ---- Disconnect-safe streaming timeout policy ----
    //
    // Regression coverage for the "chat a bit then it folds" root cause:
    // the old per-chunk gap timeout (300s) killed thinking-model turns
    // that paused between chunks. The new policy bounds only the first
    // byte; subsequent gaps are unbounded (dead peers surface via TCP
    // keepalive -> stream error).

    #[tokio::test]
    async fn first_byte_timeout_fires_when_upstream_never_responds() {
        // A stream that never yields - simulates an upstream that accepted
        // the request but never sends the first byte. We pass a tiny
        // timeout (real wall-clock, not the 120s production value) so the
        // test resolves in milliseconds.
        let (_tx, rx) = mpsc::channel::<Result<Bytes, String>>(1);
        let mut stream = ReceiverStream::new(rx);
        let res = next_chunk_with_policy(&mut stream, true, Duration::from_millis(50)).await;
        assert!(
            matches!(res, NextChunkResult::FirstByteTimeout),
            "expected FirstByteTimeout, got {res:?}"
        );
    }

    #[tokio::test]
    async fn subsequent_chunk_survives_long_thinking_pause() {
        // Core regression: after the first chunk, the upstream goes silent
        // past the first-byte timeout (thinking models do this between
        // reasoning and answer), then resumes. The gap must NOT time out -
        // that was the "chat a bit then it folds" root cause.
        let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(4);
        let timeout = Duration::from_millis(50);
        tokio::spawn(async move {
            // First byte arrives promptly.
            let _ = tx.send(Ok(Bytes::from("data: {\"a\":1}\n\n"))).await;
            // Pause that exceeds the first-byte timeout - proves subsequent
            // reads ignore it. (200ms >> 50ms; production thinking pauses
            // are minutes.)
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = tx.send(Ok(Bytes::from("data: {\"b\":2}\n\n"))).await;
        });
        let mut stream = ReceiverStream::new(rx);

        // First chunk - arrives immediately, is_first=true.
        match next_chunk_with_policy(&mut stream, true, timeout).await {
            NextChunkResult::Item(b) => assert_eq!(b, Bytes::from("data: {\"a\":1}\n\n")),
            other => panic!("expected first Item, got {other:?}"),
        }

        // Second chunk - after a 200ms pause that exceeds the 50ms timeout.
        // is_first=false -> no timeout. Old per-chunk code would fail here;
        // the new policy must hand back the item so the turn survives.
        match next_chunk_with_policy(&mut stream, false, timeout).await {
            NextChunkResult::Item(b) => assert_eq!(b, Bytes::from("data: {\"b\":2}\n\n")),
            other => panic!("expected Item after thinking pause, got {other:?}"),
        }
    }
}
