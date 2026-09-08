// Agent Loop — the core ReAct loop (Reason → Act → Observe → Repeat)
// Messages are streamed to the frontend via Tauri events.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::agent_tools;
use super::auto_fix;
use super::datalog::DatalogWriter;
use super::json_repair::repair_tool_args;
use super::llm_client::*;
use crate::commands::ssh_commands::SSHPool;

// ── Constants ──

const MAX_TOOL_LOOPS: usize = 150; // Runaway backstop, NOT a task budget. Raised 25 → 50 → 150
                                   // because heavy install/patch flows (e.g. the Claude Desktop Chinese
                                   // patch: locate app → download ZIP → run installer → poll its log
                                   // across Start-Sleep waits → verify) legitimately burn dozens of tool
                                   // calls. Context stays bounded by MAX_CONTEXT_BYTES regardless, so a
                                   // high cap is cheap; we keep it finite so a looping model still stops.
                                   // Byte-based context limit: keep recent messages whose total size fits within this budget.
                                   // 30-message count limit was not safe when tool results are large (e.g. 8KB each × 30 = 240KB+).
const MAX_CONTEXT_BYTES: usize = 300_000; // ~300KB (~80k tokens), up from 150KB so long install/patch flows keep more history. Fits 128k-context models with headroom; kept finite because Mother also runs on small-context local models.

// ── Types emitted to frontend ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking")]
    Thinking { text: String },
    #[serde(rename = "tool_call_start")]
    ToolCallStart { id: String, name: String },
    #[serde(rename = "tool_call_args")]
    ToolCallArgs { id: String, args: String },
    #[serde(rename = "tool_result")]
    ToolResult {
        id: String,
        output: String,
        success: bool,
    },
    #[serde(rename = "done")]
    Done {},
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "state")]
    StateChange { state: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub message: String,
    pub model_id: String,
    pub base_url: String, // OpenAI-compatible URL
    pub api_key: String,
    pub model_name: String,
    pub provider: String, // initial preferred protocol: "openai" or "anthropic"
    /// Optional Anthropic-compatible URL. When present the agent always tries Anthropic
    /// first and falls back to OpenAI (`base_url`) on 400 / tool-unsupported errors.
    pub anthropic_url: Option<String>,
    pub server_ids: Vec<String>, // selected SSH servers
    pub skills: Vec<String>,     // skill descriptions
    /// UI locale ("en" or "zh-Hans"). Used to hint the agent's response language.
    pub locale: Option<String>,
}

// ── Session State (kept in memory for continuous operation) ──

pub struct AgentSession {
    pub id: String,
    pub messages: Vec<Message>,
    pub running: bool,
    pub cancel_token: CancellationToken,
    /// Ring buffer of recent tool-call hashes for loop detection.
    /// Cleared at the start of each user turn — loops only count within-turn.
    pub recent_calls: std::collections::VecDeque<u64>,
}

/// Maximum repeats of the same (tool, args) within the recent-calls window
/// before the agent loop short-circuits the call with a "you're in a loop"
/// synthetic tool result. 3rd repeat trips it.
const LOOP_REPEAT_THRESHOLD: usize = 3;
/// Ring buffer size for `recent_calls`. 8 is enough to catch tight loops
/// without keeping arbitrary history around.
const RECENT_CALLS_CAPACITY: usize = 8;

impl Default for AgentSession {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentSession {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            running: false,
            cancel_token: CancellationToken::new(),
            recent_calls: std::collections::VecDeque::with_capacity(RECENT_CALLS_CAPACITY),
        }
    }

    /// Cancel current operation and create a fresh token for next run
    pub fn cancel(&mut self) {
        self.cancel_token.cancel();
        self.running = false;
    }

    /// Prepare for a new run
    pub fn prepare_run(&mut self) {
        if self.cancel_token.is_cancelled() {
            self.cancel_token = CancellationToken::new();
        }
        self.running = true;
        // New turn — drop history of the previous turn's calls so a tool
        // legitimately re-used across turns doesn't get flagged.
        self.recent_calls.clear();
    }

    /// Record a tool call and return Some(reason) if it has now repeated
    /// LOOP_REPEAT_THRESHOLD times in the recent window.
    pub fn record_call_and_detect_loop(&mut self, hash: u64) -> Option<String> {
        let prior_count = self.recent_calls.iter().filter(|&&h| h == hash).count();
        if self.recent_calls.len() >= RECENT_CALLS_CAPACITY {
            self.recent_calls.pop_front();
        }
        self.recent_calls.push_back(hash);
        if prior_count + 1 >= LOOP_REPEAT_THRESHOLD {
            Some(format!(
                "Loop detected: this exact call has now run {} times in a row without progress. \
                 Stop calling the same tool with the same arguments. Read the previous result, \
                 explain what you found, and either change approach or ask the user.",
                prior_count + 1
            ))
        } else {
            None
        }
    }
}

/// Hash a tool call so equivalent invocations collide regardless of whitespace
/// or JSON key order. Falls back to the raw string when args don't parse —
/// malformed-but-identical calls still trip the detector.
fn loop_args_hash(tool_name: &str, args: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tool_name.hash(&mut h);
    let canon = serde_json::from_str::<serde_json::Value>(args)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_else(|| args.to_string());
    canon.hash(&mut h);
    h.finish()
}

// Per-server session map (keyed by server_id: "local" or SSH server id)
pub type SharedSessionMap = Arc<Mutex<std::collections::HashMap<String, AgentSession>>>;

pub fn create_session_map() -> SharedSessionMap {
    Arc::new(Mutex::new(std::collections::HashMap::new()))
}

// ── Session Persistence ──

fn sessions_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".echobird")
        .join("config")
        .join("agent_sessions")
}

fn session_file(server_key: &str) -> std::path::PathBuf {
    // Sanitize server_key for filename
    let safe_key = server_key.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    sessions_dir().join(format!("{}.json", safe_key))
}

/// Save a session's messages to disk
pub fn save_session_to_disk(server_key: &str, messages: &[Message]) {
    if let Err(e) = std::fs::create_dir_all(sessions_dir()) {
        log::error!("[AgentSession] Failed to create sessions dir: {}", e);
        return;
    }
    let path = session_file(server_key);
    match serde_json::to_string_pretty(messages) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::error!(
                    "[AgentSession] Failed to write session {}: {}",
                    server_key,
                    e
                );
            }
        }
        Err(e) => log::error!(
            "[AgentSession] Failed to serialize session {}: {}",
            server_key,
            e
        ),
    }
}

/// Load a session's messages from disk
pub fn load_session_from_disk(server_key: &str) -> Vec<Message> {
    let path = session_file(server_key);
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Clear a session's persisted file from disk
pub fn clear_session_from_disk(server_key: &str) {
    let path = session_file(server_key);
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            log::error!(
                "[AgentSession] Failed to delete session file {}: {}",
                server_key,
                e
            );
        } else {
            log::info!("[AgentSession] Session file deleted for {}", server_key);
        }
    }
}

// ── Main Agent Loop ──

pub async fn run_agent(
    app: AppHandle,
    request: AgentRequest,
    session_map: SharedSessionMap,
    ssh_pool: SSHPool,
) -> Result<(), String> {
    // Derive server key from request
    let server_key = request
        .server_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "local".to_string());

    // 1. Build the LLM client — one protocol per session, decided by config
    //    and never switched mid-session: Anthropic when anthropic_url is set,
    //    OpenAI otherwise.
    let decrypted_key = super::model_manager::decrypt_key_for_use(&request.api_key);

    let (active_provider, client) = if let Some(ref anth_url) = request.anthropic_url {
        let cfg = LlmConfig {
            provider: LlmProvider::Anthropic,
            base_url: anth_url.clone(),
            api_key: decrypted_key.clone(),
            model: request.model_name.clone(),
        };
        (LlmProvider::Anthropic, LlmClient::new(cfg)?)
    } else {
        let cfg = LlmConfig {
            provider: LlmProvider::OpenAI,
            base_url: request.base_url.clone(),
            api_key: decrypted_key.clone(),
            model: request.model_name.clone(),
        };
        (LlmProvider::OpenAI, LlmClient::new(cfg)?)
    };

    let tools = agent_tools::get_tool_definitions();

    // 2. Build system prompt
    let system_prompt = build_system_prompt(&request, &ssh_pool).await;

    // 3. Add user message to per-server session history
    let cancel_token = {
        let mut map = session_map.lock().await;
        let sess = map.entry(server_key.clone()).or_insert_with(|| {
            let mut s = AgentSession::new();
            s.messages = load_session_from_disk(&server_key);
            s
        });
        sess.prepare_run();
        sess.messages.push(Message {
            role: "user".into(),
            content: MessageContent::Text(request.message.clone()),
        });
        sess.cancel_token.clone()
    };

    // Per-turn datalog — markdown trace under ~/.echobird/datalog/<server>/.
    // Default-on; flip the constructor arg to gate via a settings flag later.
    let mut datalog = DatalogWriter::new(true);
    datalog.begin_turn(&server_key, &request.message, &request.model_name);

    emit_event(
        &app,
        AgentEvent::StateChange {
            state: "processing".into(),
        },
    );

    // 4. ReAct loop
    let mut loop_count = 0;
    let mut sse_retry_count = 0;
    const MAX_SSE_RETRIES: u32 = 3;

    loop {
        loop_count += 1;
        if loop_count > MAX_TOOL_LOOPS {
            emit_event(
                &app,
                AgentEvent::Error {
                    message: format!("Reached maximum tool call limit ({})", MAX_TOOL_LOOPS),
                },
            );
            break;
        }

        // Check cancellation
        if cancel_token.is_cancelled() {
            log::info!("[AgentLoop] Cancelled by user");
            emit_event(
                &app,
                AgentEvent::Error {
                    message: "Cancelled by user".into(),
                },
            );
            break;
        }

        // Get current messages (byte-budget truncation: keep most-recent messages within 150KB)
        let messages = {
            let map = session_map.lock().await;
            let all = map
                .get(&server_key)
                .map(|s| s.messages.clone())
                .unwrap_or_default();
            // Walk backwards accumulating until budget is exceeded, then reverse
            let mut budget = MAX_CONTEXT_BYTES;
            let mut kept: Vec<_> = all
                .iter()
                .rev()
                .take_while(|m| {
                    let sz = serde_json::to_string(m).map(|s| s.len()).unwrap_or(256);
                    if sz > budget {
                        return false;
                    }
                    budget -= sz;
                    true
                })
                .cloned()
                .collect();
            kept.reverse();
            if kept.len() < all.len() {
                log::info!(
                    "[AgentLoop] Context trimmed: {} → {} messages (byte budget {}KB)",
                    all.len(),
                    kept.len(),
                    MAX_CONTEXT_BYTES / 1024
                );
            }
            // Trim may have dropped an assistant message that contained
            // `tool_use(call_xxx)` while the following user message
            // carrying `tool_result(call_xxx)` survived. Anthropic-style
            // upstreams reject that with "tool_result block has no
            // preceding tool_use" (issue #106). Strip orphan tool_results
            // so the resulting message list is always valid.
            ensure_tool_results_paired(&mut kept);
            kept
        };

        // Call LLM. Logging the model name + active provider here turns
        // "which model did the user pick?" from a guessing game into a
        // one-line grep when an issue lands with mismatched tool blocks
        // or other model-capability errors (see
        // feedback_tool_errors_check_model_first memory).
        log::info!(
            "[AgentLoop] Loop {}: calling LLM model={} provider={:?} with {} messages (SSE retry: {}/{})",
            loop_count,
            request.model_name,
            active_provider,
            messages.len(),
            sse_retry_count,
            MAX_SSE_RETRIES
        );
        datalog.log_llm_call();

        let mut rx = match client.chat_stream(&messages, &tools, &system_prompt).await {
            Ok(rx) => rx,
            Err(ref e) if is_llm_server_down(e) => {
                // Fatal: LLM server is unreachable — no point retrying
                let msg = format_server_down_error(e);
                log::error!("[AgentLoop] LLM server is down, aborting: {}", e);
                emit_event(&app, AgentEvent::Error { message: msg });
                break;
            }
            Err(e) => {
                if sse_retry_count < MAX_SSE_RETRIES {
                    sse_retry_count += 1;
                    // Silent retry — no UI signal. Mirrors ClaudeCode/OpenCode:
                    // a transient upstream blip shouldn't break the user's flow,
                    // only an exhausted retry budget surfaces as an error.
                    log::warn!(
                        "[AgentLoop] chat_stream failed, retrying ({}/{}): {}",
                        sse_retry_count,
                        MAX_SSE_RETRIES,
                        e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    loop_count -= 1; // Don't count retries toward tool loop limit
                    continue;
                }
                emit_event(&app, AgentEvent::Error { message: e });
                break;
            }
        };

        // Collect the response
        let mut text_accumulator = String::new();
        // Thinking-mode round-trip: must be saved on the assistant message so
        // the next turn can echo it back. Empty when the model isn't a thinker.
        let mut thinking_accumulator = String::new();
        let mut thinking_signature = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut tool_args_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut stop_reason = String::new();
        let mut had_error = false;
        let mut sse_error_msg = String::new();
        let mut received_any_token = false;
        let mut wait_warnings = 0u32;
        const FIRST_TOKEN_TIMEOUT_SECS: u64 = 60;
        const INTER_TOKEN_TIMEOUT_SECS: u64 = 120;
        const MAX_WAIT_WARNINGS: u32 = 2;

        loop {
            // Per-event timeout — longer after first token (thinking models can be slow)
            let timeout_secs = if received_any_token {
                INTER_TOKEN_TIMEOUT_SECS
            } else {
                FIRST_TOKEN_TIMEOUT_SECS
            };
            // Race: receive next LLM token OR user cancellation
            let recv_result = tokio::select! {
                result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    rx.recv()
                ) => result,
                _ = cancel_token.cancelled() => {
                    log::info!("[AgentLoop] Cancelled during LLM stream");
                    emit_event(&app, AgentEvent::Error { message: "Cancelled by user".into() });
                    had_error = true;
                    break;
                }
            };

            match recv_result {
                // Timeout: no token received within the window
                Err(_elapsed) => {
                    wait_warnings += 1;
                    if wait_warnings <= MAX_WAIT_WARNINGS {
                        let hint = format!(
                            "\n\u{23f3} Still waiting for model response... ({}/{})\n\
                             If the local LLM is not responding, try restarting it.\n",
                            wait_warnings, MAX_WAIT_WARNINGS
                        );
                        log::warn!(
                            "[AgentLoop] No token after {}s (warning {}/{})",
                            timeout_secs,
                            wait_warnings,
                            MAX_WAIT_WARNINGS
                        );
                        emit_event(&app, AgentEvent::TextDelta { text: hint });
                        continue; // Keep waiting
                    }
                    // Max warnings reached — abort
                    let timeout_msg = if received_any_token {
                        format!("\u{26a0}\u{fe0f} Model stopped responding (no data for {}s).\nThe local LLM may have crashed. Please restart it.", INTER_TOKEN_TIMEOUT_SECS)
                    } else {
                        format!("\u{26a0}\u{fe0f} LLM did not respond within {}s.\nThe model may be overloaded or crashed. Please restart the LLM server.", FIRST_TOKEN_TIMEOUT_SECS * (MAX_WAIT_WARNINGS as u64 + 1))
                    };
                    log::error!("[AgentLoop] LLM response timed out");
                    emit_event(
                        &app,
                        AgentEvent::Error {
                            message: timeout_msg,
                        },
                    );
                    had_error = true;
                    sse_error_msg = String::new(); // Non-retryable
                    break;
                }

                // Channel closed without Done event
                Ok(None) => break,

                // Got an event — process it
                Ok(Some(event)) => match event {
                    LlmEvent::TextDelta(text) => {
                        received_any_token = true;
                        wait_warnings = 0;
                        text_accumulator.push_str(&text);
                        emit_event(&app, AgentEvent::TextDelta { text });
                    }
                    LlmEvent::Thinking(text) => {
                        received_any_token = true;
                        thinking_accumulator.push_str(&text);
                        emit_event(&app, AgentEvent::Thinking { text });
                    }
                    LlmEvent::ThinkingSignature(sig) => {
                        // Anthropic streams signature separately; keep latest.
                        thinking_signature = sig;
                    }
                    LlmEvent::ToolCallStart { id, name } => {
                        received_any_token = true;
                        emit_event(
                            &app,
                            AgentEvent::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                            },
                        );
                        emit_event(
                            &app,
                            AgentEvent::StateChange {
                                state: "tool_calling".into(),
                            },
                        );
                        tool_args_map.insert(id.clone(), String::new());
                        tool_calls.push(ToolCall {
                            id,
                            name,
                            arguments: String::new(),
                        });
                    }
                    LlmEvent::ToolCallDelta { id, args_chunk } => {
                        if let Some(args) = tool_args_map.get_mut(&id) {
                            args.push_str(&args_chunk);
                        }
                        emit_event(
                            &app,
                            AgentEvent::ToolCallArgs {
                                id,
                                args: args_chunk,
                            },
                        );
                    }
                    LlmEvent::ToolCallEnd { id } => {
                        if let Some(final_args) = tool_args_map.get(&id) {
                            if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == id) {
                                // Repair common LLM JSON malformations before the
                                // tool sees the args. Pass-through on valid JSON.
                                let repaired = repair_tool_args(&tc.name, final_args);
                                if repaired != *final_args {
                                    log::warn!(
                                        "[AgentLoop] Repaired malformed tool args for {} ({}): {} → {}",
                                        tc.name,
                                        id,
                                        truncate_for_log(final_args),
                                        truncate_for_log(&repaired),
                                    );
                                }
                                tc.arguments = repaired;
                            }
                        }
                    }
                    LlmEvent::Done {
                        stop_reason: reason,
                    } => {
                        stop_reason = reason;
                        break;
                    }
                    LlmEvent::Error(e) => {
                        // Fatal: LLM server is down — abort immediately without retry
                        if is_llm_server_down(&e) {
                            let msg = format_server_down_error(&e);
                            log::error!("[AgentLoop] LLM server went down during stream: {}", e);
                            emit_event(&app, AgentEvent::Error { message: msg });
                            had_error = true;
                            sse_error_msg = String::new(); // Prevent retry
                            break;
                        }
                        // Stream failed mid-flight. One protocol per session
                        // (no mid-stream downgrade): treat every failure as a
                        // retryable SSE error — the retry budget absorbs
                        // transient blips, and an exhausted budget surfaces as
                        // a clean error below.
                        sse_error_msg = e;
                        had_error = true;
                        break;
                    }
                },
            }
        }

        if had_error {
            if !sse_error_msg.is_empty() && sse_retry_count < MAX_SSE_RETRIES {
                // SSE stream error — silent retry (no UI signal). The user
                // sees whatever partial text streamed before the failure, the
                // retry stream appends naturally, and only an exhausted retry
                // budget surfaces as an error.
                sse_retry_count += 1;
                log::warn!(
                    "[AgentLoop] SSE stream error, retrying ({}/{}): {}",
                    sse_retry_count,
                    MAX_SSE_RETRIES,
                    sse_error_msg
                );
                // If we had partial text but no tool calls, save it to avoid losing progress
                if !text_accumulator.is_empty() && tool_calls.is_empty() {
                    let mut map = session_map.lock().await;
                    let sess = map
                        .entry(server_key.clone())
                        .or_insert_with(AgentSession::new);
                    sess.messages.push(Message {
                        role: "assistant".into(),
                        content: MessageContent::Text(text_accumulator.clone()),
                    });
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                loop_count -= 1; // Don't count retries toward tool loop limit
                continue;
            }
            // Max retries exceeded or non-retryable error
            if !sse_error_msg.is_empty() {
                emit_event(
                    &app,
                    AgentEvent::Error {
                        message: format!("__CONN_FAILED__:{}\n__CONN_HINT__", MAX_SSE_RETRIES),
                    },
                );
            }
            // Remove the user message that caused the error from history
            let mut map = session_map.lock().await;
            if let Some(sess) = map.get_mut(&server_key) {
                if let Some(last) = sess.messages.last() {
                    if last.role == "user" {
                        sess.messages.pop();
                    }
                }
            }
            break;
        }

        // Reset SSE retry counter on successful stream completion
        sse_retry_count = 0;

        // 5. Store assistant response in history
        {
            let mut map = session_map.lock().await;
            let sess = map
                .entry(server_key.clone())
                .or_insert_with(AgentSession::new);
            sess.messages.push(build_assistant_message(
                &text_accumulator,
                &thinking_accumulator,
                &thinking_signature,
                &tool_calls,
            ));
        }

        // 6. If no tool calls, we're done
        if tool_calls.is_empty() {
            log::info!(
                "[AgentLoop] LLM finished with no tool calls (reason: {})",
                stop_reason
            );
            datalog.log_text(&text_accumulator);
            break;
        }

        // 7. Execute tool calls and feed results back
        log::info!("[AgentLoop] Executing {} tool calls", tool_calls.len());
        emit_event(
            &app,
            AgentEvent::StateChange {
                state: "executing".into(),
            },
        );

        // Pre-validate shell_exec install commands against the user's stated
        // intent. Any command that would install a different product than
        // requested gets short-circuited with a synthetic tool_result here
        // (no actual shell call), so the model has to re-plan.
        let messages_snapshot: Vec<Message> = {
            let map = session_map.lock().await;
            map.get(&server_key)
                .map(|s| s.messages.clone())
                .unwrap_or_default()
        };
        let mut precomputed: std::collections::HashMap<String, agent_tools::ToolResult> =
            std::collections::HashMap::new();
        for tc in &tool_calls {
            if tc.name == "shell_exec" {
                if let Ok(args) = serde_json::from_str::<Value>(&tc.arguments) {
                    if let Some(cmd) = args["command"].as_str() {
                        if let Err(msg) = validate_install_intent(cmd, &messages_snapshot) {
                            log::warn!(
                                "[AgentLoop] Install-intent block on tool {}: {}",
                                tc.id,
                                msg
                            );
                            precomputed.insert(
                                tc.id.clone(),
                                agent_tools::ToolResult {
                                    success: false,
                                    output: msg,
                                },
                            );
                        }
                    }
                }
            }
        }

        // Loop detection: short-circuit any tool call that would be the Nth
        // identical (tool, args) repeat within the current turn. Hash is
        // computed against canonical JSON so whitespace / key-order doesn't
        // hide loops. Skipped for tools already short-circuited above
        // (intent validator wins; no need to also flag a loop on them).
        {
            let mut map = session_map.lock().await;
            let sess = map
                .entry(server_key.clone())
                .or_insert_with(AgentSession::new);
            for tc in &tool_calls {
                if precomputed.contains_key(&tc.id) {
                    continue;
                }
                let h = loop_args_hash(&tc.name, &tc.arguments);
                if let Some(reason) = sess.record_call_and_detect_loop(h) {
                    log::warn!(
                        "[AgentLoop] Loop guard tripped on tool {} ({}): {}",
                        tc.name,
                        tc.id,
                        reason
                    );
                    precomputed.insert(
                        tc.id.clone(),
                        agent_tools::ToolResult {
                            success: false,
                            output: reason,
                        },
                    );
                }
            }
        }

        // Track how many tools were saved, so we can complete the rest on cancel
        let mut completed_count = 0usize;

        // Decide dispatch mode: parallel only when ALL tool calls are read-only
        // and there are 2+ of them. Mixed batches and exclusive tools keep the
        // existing sequential behaviour so order-dependent flows (file_write
        // then shell_exec, etc.) stay correct.
        let all_shared = tool_calls.iter().all(|tc| is_shared_tool(&tc.name));
        let parallel = all_shared && tool_calls.len() > 1;

        if parallel {
            log::info!(
                "[AgentLoop] Parallel-dispatching {} read-only tools",
                tool_calls.len()
            );
            for tc in &tool_calls {
                datalog.log_tool_call(&tc.name, &tc.arguments);
            }
            let mut handles = Vec::with_capacity(tool_calls.len());
            for tc in &tool_calls {
                let name = tc.name.clone();
                let args = tc.arguments.clone();
                let sshp = ssh_pool.clone();
                let server_ids = request.server_ids.clone();
                let pre = precomputed.get(&tc.id).cloned();
                handles.push(tokio::spawn(async move {
                    if let Some(p) = pre {
                        return p;
                    }
                    agent_tools::execute_tool(&name, &args, &sshp, &server_ids).await
                }));
            }

            let mut joined = std::pin::pin!(futures_util::future::join_all(handles));
            let results: Vec<agent_tools::ToolResult> = tokio::select! {
                rs = &mut joined => rs.into_iter().map(|r| r.unwrap_or_else(|e| agent_tools::ToolResult {
                    success: false,
                    output: format!("Tool task panicked: {}", e),
                })).collect(),
                _ = cancel_token.cancelled() => {
                    log::info!("[AgentLoop] Parallel batch cancelled by user");
                    tool_calls.iter().map(|_| agent_tools::ToolResult {
                        success: false,
                        output: "Cancelled by user".to_string(),
                    }).collect()
                }
            };

            for (tc, result) in tool_calls.iter().zip(results) {
                datalog.log_tool_result(&result.output, result.success);
                emit_event(
                    &app,
                    AgentEvent::ToolResult {
                        id: tc.id.clone(),
                        output: result.output.clone(),
                        success: result.success,
                    },
                );
                let mut map = session_map.lock().await;
                let sess = map
                    .entry(server_key.clone())
                    .or_insert_with(AgentSession::new);
                let block = ContentBlock::ToolResult {
                    tool_use_id: tc.id.clone(),
                    content: result.output,
                };
                match active_provider {
                    LlmProvider::OpenAI => sess.messages.push(Message {
                        role: "tool".into(),
                        content: MessageContent::Blocks(vec![block]),
                    }),
                    LlmProvider::Anthropic => sess.messages.push(Message {
                        role: "user".into(),
                        content: MessageContent::Blocks(vec![block]),
                    }),
                }
                completed_count += 1;
            }
        } else {
            for tc in &tool_calls {
                // Check cancellation before each tool
                if cancel_token.is_cancelled() {
                    log::info!("[AgentLoop] Cancelled before tool: {}", tc.name);
                    break;
                }

                log::info!("[AgentLoop] Executing tool: {} ({})", tc.name, tc.id);
                datalog.log_tool_call(&tc.name, &tc.arguments);

                // Use precomputed result if intent-validator already produced one;
                // otherwise race tool execution against cancel token.
                let (mut result, was_cancelled) = if let Some(pre) = precomputed.remove(&tc.id) {
                    (pre, false)
                } else {
                    tokio::select! {
                        r = agent_tools::execute_tool(&tc.name, &tc.arguments, &ssh_pool, &request.server_ids) => (r, false),
                        _ = cancel_token.cancelled() => {
                            log::info!("[AgentLoop] Tool cancelled by user: {}", tc.name);
                            (agent_tools::ToolResult { output: "Cancelled by user".to_string(), success: false }, true)
                        }
                    }
                };

                // auto_fix: for known install/config-write intents, run a
                // deterministic verifier. On verify failure we flip success
                // and append a banner to result.output — the next ReAct
                // iteration sees the failure and re-plans naturally.
                if result.success && !was_cancelled {
                    if let Some((intent, server_id)) =
                        derive_verify_intent(&tc.name, &tc.arguments, &request.server_ids)
                    {
                        log::info!(
                            "[AgentLoop] auto_fix verifying {} on {}",
                            intent.label(),
                            server_id
                        );
                        match auto_fix::verify(&intent, &server_id, &ssh_pool).await {
                            Ok(()) => log::info!("[AgentLoop] auto_fix OK for {}", intent.label()),
                            Err(reason) => {
                                log::warn!(
                                    "[AgentLoop] auto_fix FAILED for {}: {}",
                                    intent.label(),
                                    reason
                                );
                                result = auto_fix::wrap_failure(result, &intent, reason);
                            }
                        }
                    }
                }

                datalog.log_tool_result(&result.output, result.success);
                // Always emit ToolResult to frontend
                emit_event(
                    &app,
                    AgentEvent::ToolResult {
                        id: tc.id.clone(),
                        output: result.output.clone(),
                        success: result.success,
                    },
                );

                // Always save tool result to message history — even for cancelled tools.
                // This keeps the conversation structure valid (every tool_call must have a tool_result).
                {
                    let mut map = session_map.lock().await;
                    let sess = map
                        .entry(server_key.clone())
                        .or_insert_with(AgentSession::new);
                    match active_provider {
                        LlmProvider::OpenAI => {
                            sess.messages.push(Message {
                                role: "tool".into(),
                                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                                    tool_use_id: tc.id.clone(),
                                    content: result.output,
                                }]),
                            });
                        }
                        LlmProvider::Anthropic => {
                            sess.messages.push(Message {
                                role: "user".into(),
                                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                                    tool_use_id: tc.id.clone(),
                                    content: result.output,
                                }]),
                            });
                        }
                    }
                }
                completed_count += 1;

                if was_cancelled {
                    break;
                }
            }
        }

        // If cancelled mid-loop, add "Cancelled" results for any remaining (unexecuted) tool calls.
        // This ensures the conversation history is always valid: every tool_call has a tool_result.
        if cancel_token.is_cancelled() {
            if completed_count < tool_calls.len() {
                let mut map = session_map.lock().await;
                let sess = map
                    .entry(server_key.clone())
                    .or_insert_with(AgentSession::new);
                for tc in tool_calls.iter().skip(completed_count) {
                    let cancelled_result = ContentBlock::ToolResult {
                        tool_use_id: tc.id.clone(),
                        content: "Cancelled by user".to_string(),
                    };
                    match active_provider {
                        LlmProvider::OpenAI => {
                            sess.messages.push(Message {
                                role: "tool".into(),
                                content: MessageContent::Blocks(vec![cancelled_result]),
                            });
                        }
                        LlmProvider::Anthropic => {
                            sess.messages.push(Message {
                                role: "user".into(),
                                content: MessageContent::Blocks(vec![cancelled_result]),
                            });
                        }
                    }
                }
            }
            break;
        }

        // Continue loop — feed tool results back to LLM
        emit_event(
            &app,
            AgentEvent::StateChange {
                state: "processing".into(),
            },
        );
    }

    // 8. Done — save session to disk
    {
        let mut map = session_map.lock().await;
        if let Some(sess) = map.get_mut(&server_key) {
            sess.running = false;
            save_session_to_disk(&server_key, &sess.messages);
        }
    }
    datalog.end_turn();
    emit_event(&app, AgentEvent::Done {});
    emit_event(
        &app,
        AgentEvent::StateChange {
            state: "idle".into(),
        },
    );

    Ok(())
}

// ── Helpers ──

fn emit_event(app: &AppHandle, event: AgentEvent) {
    if let Err(e) = app.emit("agent_event", &event) {
        log::error!("[AgentLoop] Failed to emit event: {}", e);
    }
}

/// Inspect a tool call and return (intent, resolved_server_id) when it is a
/// known install/config-write that auto_fix should verify. Mirrors the
/// server_id resolution used by execute_tool: an explicit "local" or empty
/// server_id is replaced by the session's first non-local server when one
/// exists, so the verifier hits the same machine the install ran on.
fn derive_verify_intent(
    tool_name: &str,
    args_json: &str,
    session_server_ids: &[String],
) -> Option<(auto_fix::InstallIntent, String)> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let intent = match tool_name {
        "shell_exec" => auto_fix::detect_install_intent_from_shell(args["command"].as_str()?),
        "file_write" => auto_fix::detect_install_intent_from_write(args["path"].as_str()?),
        _ => None,
    }?;
    let raw_sid = args["server_id"].as_str().unwrap_or("local");
    let server_id = if raw_sid == "local" || raw_sid.is_empty() {
        session_server_ids
            .iter()
            .find(|s| s.as_str() != "local" && !s.is_empty())
            .cloned()
            .unwrap_or_else(|| "local".to_string())
    } else {
        raw_sid.to_string()
    };
    Some((intent, server_id))
}

fn truncate_for_log(s: &str) -> String {
    const MAX: usize = 160;
    if s.chars().count() <= MAX {
        s.replace('\n', "\\n")
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{}...", head.replace('\n', "\\n"))
    }
}

/// Build the assistant `Message` to append to session history from the
/// accumulators that filled during streaming. Pure data transformation —
/// kept as a free function so `run_agent` doesn't carry the inline 40-line
/// block-building chain.
///
/// Block order matters for Anthropic: thinking must precede tool_use, and
/// text (if any) sits between them. The simple `Text` variant is reserved
/// for pure-text responses (no thinking, no tool_use) so trivial assistant
/// turns don't get gratuitous Blocks wrapping.
fn build_assistant_message(
    text_accumulator: &str,
    thinking_accumulator: &str,
    thinking_signature: &str,
    tool_calls: &[ToolCall],
) -> Message {
    let has_thinking = !thinking_accumulator.is_empty();
    if tool_calls.is_empty() && !has_thinking {
        return Message {
            role: "assistant".into(),
            content: MessageContent::Text(text_accumulator.to_string()),
        };
    }
    let mut blocks: Vec<ContentBlock> = Vec::new();
    if has_thinking {
        blocks.push(ContentBlock::Thinking {
            thinking: thinking_accumulator.to_string(),
            signature: thinking_signature.to_string(),
        });
    }
    if !text_accumulator.is_empty() {
        blocks.push(ContentBlock::Text {
            text: text_accumulator.to_string(),
        });
    }
    for tc in tool_calls {
        let input: Value =
            serde_json::from_str(&tc.arguments).unwrap_or(Value::Object(Default::default()));
        blocks.push(ContentBlock::ToolUse {
            id: tc.id.clone(),
            name: tc.name.clone(),
            input,
        });
    }
    Message {
        role: "assistant".into(),
        content: MessageContent::Blocks(blocks),
    }
}

/// Strip `tool_result` blocks whose matching `tool_use` is no longer
/// present in any kept message. After the byte-budget trim above, an
/// assistant message holding `tool_use(call_xxx)` can fall outside the
/// budget while the following user message carrying
/// `tool_result(call_xxx)` survives — the resulting message list has
/// an orphan tool_result that Anthropic-style upstreams reject with
/// "tool_result block has no preceding tool_use" (issue #106).
///
/// Mirror of `ensure_tool_outputs_paired` in codex_proxy (which handles
/// the reverse case: orphan `tool_use` without `tool_result`). The two
/// directions exist independently because Mother Agent owns its own
/// trim path and never goes through codex_proxy's pairing pass.
///
/// Strategy:
///   1. Pass 1 — collect every `tool_use` id currently present.
///   2. Pass 2 — drop `tool_result` blocks whose id isn't in the set.
///   3. Pass 3 — drop messages whose content emptied out as a result.
fn ensure_tool_results_paired(messages: &mut Vec<Message>) {
    use std::collections::HashSet;
    let mut present_tool_use_ids: HashSet<String> = HashSet::new();
    for msg in messages.iter() {
        if let MessageContent::Blocks(blocks) = &msg.content {
            for block in blocks {
                if let ContentBlock::ToolUse { id, .. } = block {
                    present_tool_use_ids.insert(id.clone());
                }
            }
        }
    }

    let mut dropped = 0usize;
    for msg in messages.iter_mut() {
        if let MessageContent::Blocks(blocks) = &mut msg.content {
            let before = blocks.len();
            blocks.retain(|block| {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    present_tool_use_ids.contains(tool_use_id)
                } else {
                    true
                }
            });
            dropped += before - blocks.len();
        }
    }

    messages.retain(|msg| match &msg.content {
        MessageContent::Text(s) => !s.is_empty(),
        MessageContent::Blocks(blocks) => !blocks.is_empty(),
    });

    if dropped > 0 {
        log::info!(
            "[AgentLoop] ensure_tool_results_paired: dropped {} orphan tool_result block(s) after trim",
            dropped
        );
    }
}

/// Load the system prompt from the compile-time bundled asset. No network
/// involved — many users pick smart-install precisely because their network
/// is unreliable, so the prompt itself must work offline.
fn load_bundled_prompt() -> String {
    let prompt = crate::services::bundled_assets::mother_system_prompt();
    log::info!("[AgentLoop] Bundled prompt loaded ({} bytes)", prompt.len());
    prompt.to_string()
}

async fn build_system_prompt(request: &AgentRequest, ssh_pool: &SSHPool) -> String {
    // Prepend locale hint so the agent responds in the user's preferred language
    let locale_hint = if let Some(ref locale) = request.locale {
        // Map locale code to readable language name for clarity
        let lang_name = match locale.as_str() {
            "zh" | "zh-Hans" => "Simplified Chinese (简体中文)",
            _ => "English",
        };
        format!("## User Language\nThe user's interface is set to **{}**. Respond in this language by default unless the user writes in a different language.\n\n", lang_name)
    } else {
        String::new()
    };

    let mut prompt = locale_hint;
    prompt.push_str(
        "You are EchoBird's AI agent deployment expert — the built-in deployment assistant of EchoBird. \
        Your purpose is to help users deploy AI agents on local machines or remote servers via SSH.\n\n\
        ## Your Mission\n\
        You specialize in one-click deployment of AI agents such as:\n\
        - OpenClaw (open-source AI agent, npm package name: `openclaw`, NOT `@anthropic-ai/claude-code`)\n\
        - Other open-source AI agents that can run autonomously\n\n\
        You ONLY deploy AI agents. You do NOT deploy IDEs, editors, or local tools.\n\n\
        Users should NEVER have to manually fiddle with installation steps. \
        You handle EVERYTHING automatically: detect the OS, install prerequisites \
        (Node.js, Git, Rust, Python, Docker, etc.), download the target agent, \
        configure it, and verify it works. The user just tells you WHAT agent to deploy \
        and WHERE, and you deliver a working result.\n\n\
        ## CRITICAL: Tool Identity — NEVER Confuse These\n\
        These are DIFFERENT products by DIFFERENT companies. NEVER mix them up:\n\
        | Tool | Company | npm Package | Binary |\n\
        |------|---------|-------------|--------|\n\
        | Claude Code | Anthropic | `@anthropic-ai/claude-code` | claude |\n\
        | Codex CLI | OpenAI | `@openai/codex` | codex |\n\
        | OpenCode | Charm/Anomaly | `opencode-ai` | opencode |\n\
        | OpenClaw | Community | `openclaw` | openclaw |\n\
        | MiMo Code | Xiaomi | `@mimo-ai/cli` | mimo |\n\
        | Kilo Code | Kilo | `@kilocode/cli` | kilo |\n\
        | Kimi Code | Moonshot AI | `@moonshot-ai/kimi-code` | kimi |\n\
        When the user says 'install Codex', install `@openai/codex`. Do NOT install Claude Code.\n\
        When the user says 'install Claude Code', install via `irm https://claude.ai/install.ps1 | iex` (Windows) or `curl -fsSL https://claude.ai/install.sh | bash`. Do NOT install Codex.\n\
        When the user says 'install OpenCode', install `opencode-ai`. Do NOT install Codex or Claude Code.\n\
        When the user says 'install MiMo Code', install `@mimo-ai/cli` (or `curl -fsSL https://mimo.xiaomi.com/install | bash` on macOS/Linux). It is a fork of OpenCode but a SEPARATE product — do NOT install `opencode-ai`.\n\
        When the user says 'install Kilo Code', install `@kilocode/cli` (or `curl -fsSL https://kilo.ai/cli/install | bash` on macOS/Linux). It is a fork of OpenCode but a SEPARATE product — do NOT install `opencode-ai`.\n\
        When the user says 'install Kimi Code', install `@moonshot-ai/kimi-code` (or `curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash` on macOS/Linux). It is built on Pi's pi-tui but a SEPARATE product — do NOT install `pi` or `@earendil-works/pi-coding-agent`.\n\
        ALWAYS read the tool's install JSON first from the **Embedded Install References** section appended below — do NOT `web_fetch` echobird.ai for these (they are already in this prompt).\n\n\
        ## Rules\n\
        - Work autonomously. Do NOT ask the user unnecessary questions.\n\
        - Detect the OS and package manager first, then proceed.\n\
        - Always verify each step succeeded before moving to the next.\n\
        - If a command fails, diagnose and try alternative approaches automatically.\n\
        - For destructive operations, explain briefly before executing.\n\
        - Keep responses concise. Only show output when it reveals useful info.\n\
        - After deployment is complete, summarize what was installed and how to access it.\n\
        - **Stay in your lane**: After install/configure/repair completes, do NOT direct the user to other EchoBird pages (App Desktop, Model Nexus, etc.) or describe what to click there. Your job is the install/configure/repair work itself, not UI navigation. Users already know the rest of the app.\n\
        - **Windows targets**: When the user wants to install an AI agent on Windows, \
install it directly on Windows using native Windows commands (PowerShell, cmd). \
Do NOT suggest or mention WSL2 — it creates unnecessary complexity for most users. \
If a specific agent does not support Windows natively, clearly tell the user: \
'This agent currently only supports Linux/macOS. You would need a Linux or macOS machine to run it.' \
Do NOT offer WSL2 as a workaround.\n\
        - **CRITICAL SAFETY RULES** (NEVER violate these):\n\
          - NEVER delete, remove, or modify the `~/.echobird/` directory or anything inside it. \
            This directory contains Echobird's configuration, models, and user data.\n\
          - NEVER kill or stop the Echobird process (echobird.exe / Echobird).\n\
          - When uninstalling agents (e.g. OpenClaw), ONLY remove the agent itself \
            (e.g. `npm uninstall -g openclaw`). Do NOT touch Echobird's files.\n\
          - NEVER run commands that delete user home directories or broad recursive deletions.\n\n\
        ## UI Chat Protocol (MANDATORY)\n\
        After completing all thinking, analysis, and tool calls, wrap your final reply to the user in `<chat>...</chat>` tags.\n\
        - Write the `<chat>` message like a friendly chat message: concise, clear, natural language.\n\
        - Keep ALL technical details, logs, raw command output, and tool execution inside your reasoning -- NOT in `<chat>`.\n\
        - Any content that requires the user to respond, choose, or take action MUST be inside `<chat>`.\n\
        - ONE `<chat>` block per response, at the very end.\n\n\
        ## Server Context Lock -- NEVER Violate This\n\
        The user's currently selected server is the ONE AND ONLY target for ALL operations.\n\
        - Every shell_exec, install, uninstall, configure, restart, or delete action targets the selected server -- no exceptions.\n\
        - NEVER switch servers mid-conversation.\n\
        - NEVER mix local and remote. If user selected REMOTE, don't run anything on local machine. If LOCAL (127.0.0.1), don't SSH anywhere.\n\
        - Before any action, verify: \"Which server is selected? Am I targeting that server?\"\n\
        - If unsure which server to target, ask the user -- do NOT assume.\n\n\
        ## Tool Calling Capability Check\n\
        CRITICAL: If you cannot call tools, say so immediately -- do NOT pretend to act.\n\
        If you are a small local model that does not support function/tool calling:\n\
        1. Stop immediately -- do NOT describe what you \"would do\".\n\
        2. Be honest: Tell the user you lack tool-calling capability for this task.\n\
        3. Guide them to the Local LLM page to download a larger Function Calling model (e.g. Qwen2.5-Coder 7B+).\n\n\
        ## Language Rules\n\
        Always respond in the same language the user is writing in.\n\
        - Product name: Always \"EchoBird\" in any language. Never translate it.\n\
        - Page names in Chinese (zh-Hans/zh-Hant): 模型中心 / 应用桌面 / 频道 / 技能浏览 / 本地大模型\n\
        - Page names in all other languages: Model Nexus / App Desktop / Skill Browser / Local LLM\n\n\
        ## First Interaction Behavior\n\
        When a user first interacts without a specific request:\n\
        - Do NOT proactively push any specific agent. Wait for the user to state what they want.\n\
        - Briefly introduce yourself as EchoBird's AI agent deployment expert (install, configure, repair). Do NOT mention or use the name \"Mother Agent\".\n\
        - Only recommend OpenClaw if the user explicitly asks for an Agent OS recommendation.\n\n\
        ## CRITICAL MODEL CONFIGURATION RULES\n\
        - NEVER tell users to set API key environment variables (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.) manually.\n\
        - NEVER direct users to Anthropic/OpenAI websites to get keys. Users manage keys in EchoBird's Model Nexus page.\n\
        - NEVER manually write model config files (config.json, config.yaml, etc.) for agents. EchoBird handles this automatically.\n\
        - Model configuration for installed agents is fully automatic — handled by EchoBird's existing UI. Do NOT explain where to switch models or which page to visit; users manage that themselves.\n\
        - OpenClaw is NOT Claude Code. Do NOT apply Claude Code configuration methods to OpenClaw.\n\
        - CLI tools (Claude Code, Codex CLI (@openai/codex), OpenCode, Aider) can be installed on the user-selected local or remote machine. Check that target's OS and prerequisites; desktop apps require a graphical session.\n\
        - For unknown agents, use web_fetch on official docs. NEVER fabricate configuration steps.\n\n\
        ## Handling sudo / password prompts (Local AND Remote)\n\
        On Linux/macOS, plain `sudo <command>` will FAIL FAST with an error like 'a terminal is required' \
        or 'no askpass program specified' — the tool runner pipes /dev/null to stdin so commands cannot hang. \
        Other tools that prompt for a password interactively (ssh, git over https, npm login, gh auth, etc.) \
        will fail the same way. This is intentional: never wait on a password prompt. Instead:\n\
        1. **Prefer the non-sudo / no-password path first.** For most agent installs you do NOT need sudo at all:\n\
           - Node.js → install via nvm into the user's home (no sudo).\n\
           - Python packages → `pip install --user <pkg>` or use a venv.\n\
           - Rust binaries → `cargo install <pkg>` (lands in ~/.cargo/bin).\n\
           - Global npm CLIs → after nvm installs node, `npm install -g` works without sudo.\n\
           - Git → prefer https public repos over ssh; avoid `git push` flows that trigger auth prompts.\n\
           Only fall back to the system package manager (apt/dnf/pacman/zypper/apk) when the user-mode path is impossible.\n\
        2. **If a password is unavoidable, behavior depends on which machine:**\n\
           - REMOTE server: call `get_sudo_password` with the server_id. You get the saved SSH password back. \
             Then run `echo '<password>' | sudo -S <command>`. Mask the password as `***` in any text shown to the user.\n\
           - LOCAL machine (server_id=\"local\" or omitted): **DO NOT proactively ask the user to type their password into chat.** \
             EchoBird does not collect or store local sudo passwords. The default path is hand-off:\n\
             a. Stop trying to install through `shell_exec`.\n\
             b. In your `<chat>` reply, give the user the exact copy-paste commands to run in their own terminal, \
                pulled from the tool's install JSON (the **Embedded Install References** section). Include the \
                tool's `homepage` and/or `docs` URL so they can verify.\n\
             c. Tell the user, in one short sentence, that this step needs their sudo password and is faster to \
                run locally; once it's done they can come back and you'll continue with verification/config. \
                Mention as a brief aside that they MAY paste the password in chat if they prefer — but do not pressure or repeat the offer.\n\
             d. Do NOT call `get_sudo_password` for `local` again — that path is intentionally closed.\n\
           - LOCAL machine, password volunteered: if the user proactively pastes their sudo password (now or in a \
             previous turn), you MAY use it. Run `echo '<password>' | sudo -S <command>`, and in any text or \
             commands you echo back to the UI, mask it as `echo '***' | sudo -S ...`. Use it for the current task \
             only — do not log it, do not save it, do not include it in your final summary.\n\
        3. NEVER attempt to brute-force, bypass sudo, edit sudoers, or run `sudo -k` to clear cache.\n\
        4. NEVER call bare `sudo apt install ...` and expect it to work — it will return a stdin/tty error in 1-2 seconds. Pipe the password via `-S` (remote) or hand off to the user (local).\n\n\
        ## Linux shell notes\n\
        - The local shell runner uses `bash -c` (not `sh -c`), so `source`, `[[ ... ]]`, arrays, and `$'...'` all work.\n\
        - For nvm, the standard pattern works: `curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash && source ~/.nvm/nvm.sh && nvm install --lts`.\n\
        - When in doubt about which package manager exists, probe with: `command -v apt-get || command -v dnf || command -v pacman || command -v zypper || command -v apk`.\n\n\
        ## CRITICAL: Destructive Action Safety Rule\n\
        Before ANY destructive action (uninstall, delete files, stop services, wipe data), you MUST:\n\
        1. Explicitly state the target machine: \"I will perform this on [server name / IP].\"\n\
        2. Ask for confirmation: \"Confirm? (yes/no)\"\n\
        3. Only proceed after the user says yes.\n\n\
        ### Uninstall / Delete Any Agent or Service\n\
        - Always identify the exact target tool first. If ambiguous, ask the user.\n\
        - General uninstall: confirm target -> identify method (npm uninstall -g, systemctl, rm binary) -> stop processes -> verify removed.\n\
        - OpenClaw remote: npm uninstall -g openclaw && pkill -f 'openclaw gateway' || true\n\
        - NEVER delete ~/.openclaw/openclaw.json unless user explicitly requests -- it contains the channel pairing token.\n\n\
        ## Tool Install Reference\n\
        When the user asks to install any tool, ALWAYS read the install reference from the **Embedded Install References** section appended at the end of this system prompt — it contains the install JSON for every supported tool (openclaw, opencode, mimocode, kilo, kimicode, claudecode, claudescience, openscience, codex, hermes, grok, workbuddy, zcode, dsh).\n\
        Do NOT `web_fetch` `https://echobird.ai/api/tools/install/...` — that content is already embedded in this prompt and works offline.\n\
        Use `web_fetch` on the tool's official site when the tool is not in the embedded list, when its reference explicitly requires current download links or repository setup instructions, or when an install failure indicates an outdated endpoint, package, prerequisite, or installer option. Verify replacements before retrying.\n\n\
        ## Network Pre-Check (MANDATORY Before Installation)\n\
        Before installing ANY agent, reason about the network — but NEVER let a single reachability probe decide the strategy:\n\
        1. **Detect user region from their INPUT LANGUAGE** (what they type, NOT the UI setting):\n\
           - Simplified Chinese input -> likely mainland China (GitHub/npm/PyPI often blocked or extremely slow)\n\
           - Traditional Chinese -> likely Taiwan/HK (usually fine)\n\
           - Other languages -> usually fine\n\
        2. **If the tool's install JSON has `install_flow.agent_steps`, that field is AUTHORITATIVE — follow it literally, in order.** It already encodes the correct, tested network strategy (for some tools a MANDATORY mirror-backed flow). Do NOT override it with your own probe, and do NOT pick a different path just because some URL happened to respond.\n\
        3. **NEVER determine the install strategy by testing whether GitHub (or any single upstream) opens.** Reaching github.com / raw.githubusercontent.com at normal speed does NOT predict success — many installers are BOOTSTRAPPERS that then pull Python/Node/Git/Electron/Playwright from many separate hosts, and any one being blocked silently hangs the whole install (e.g. stuck at '0 of 0 steps'). A momentary 'reachable' result is also unreliable on China's intermittent GFW.\n\
        4. Mirror selection — ONLY for tools WITHOUT an `install_flow.agent_steps`:\n\
           - Default to the tool's `mirrors` (network_requirements) for Simplified-Chinese users; a reachable upstream is NEVER by itself a reason to run an official multi-source installer\n\
           - You may web_fetch a `mirrors` entry only to pick the fastest WORKING mirror, and follow the install-method rules elsewhere in this prompt (e.g. ask before switching npm registries)\n\
           - BOTH original AND mirror URLs ALL fail or extremely slow -> STOP. Tell the user:\n\
             'We detected you may be in [region]. Your network cannot reliably connect to [servers]. These packages are large (100MB+) and installation success rate is nearly zero at your current network speed. Please configure a VPN/proxy and try again.'\n\
             Do NOT attempt installation when all connectivity tests fail or are extremely slow.\n\
        5. For likely-restricted users (Simplified-Chinese input / mainland China), follow the tool's agent_steps — some tools default to the official installer and offer a mirror FALLBACK triggered by a user-reported failure (e.g. Hermes Desktop). Proactively tell such users to report a failure/hang so you can switch to the mirror method; do NOT force mirrors up front or probe to 'prove' GitHub is blocked.\n\n\
"
    );

    // Bundled system prompt + embedded install/script references.
    // Both are compile-time `include_str!` so the agent works offline.
    prompt.push_str(&load_bundled_prompt());
    prompt.push_str(&crate::services::bundled_assets::build_embedded_refs_section());
    prompt.push_str("\n\n");

    // Local platform info
    prompt.push_str("## Local Machine\n");
    prompt.push_str(&agent_tools::get_local_platform_info());
    prompt.push_str("\n\n");

    // SSH servers info
    if !request.server_ids.is_empty() {
        let has_remote = request.server_ids.iter().any(|s| s != "local");
        if has_remote {
            prompt.push_str("## ACTIVE TARGET SERVER (CRITICAL)\n");
            prompt.push_str(
                "The user has selected a REMOTE server as their target. \
                You MUST execute ALL shell_exec, file_read, and file_write calls \
                with the server_id shown below. NEVER omit server_id — \
                omitting it will run commands on the LOCAL machine instead of the remote server, \
                which is WRONG and defeats the user's intent.\n\n",
            );
            let connections = ssh_pool.lock().await;
            for sid in &request.server_ids {
                if sid == "local" {
                    continue;
                }
                let status = if connections.contains_key(sid) {
                    "connected"
                } else {
                    "not connected"
                };
                prompt.push_str(&format!(
                    ">>> TARGET: server_id='{}' ({}) <<<\n",
                    sid, status
                ));
                prompt.push_str(&format!(
                    "Every tool call MUST include: \"server_id\": \"{}\"\n\n",
                    sid
                ));
            }
        }
    }

    // Skills
    if !request.skills.is_empty() {
        prompt.push_str("## Active Skills\n");
        for skill in &request.skills {
            prompt.push_str(&format!("- {}\n", skill));
        }
        prompt.push('\n');
    }

    prompt
}

// -- LLM Server Down Detection --

/// Returns true if the error indicates the LLM server is completely unreachable.
/// These are fatal connection errors -- no point retrying.
fn is_llm_server_down(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("connection refused")
        || lower.contains("os error 111")    // Linux: connection refused
        || lower.contains("os error 61")     // macOS: connection refused
        || lower.contains("os error 10061")  // Windows: connection refused
        || lower.contains("no connection could be made")
        || lower.contains("failed to connect")
        || lower.contains("tcp connect error")
}

/// Build a clear, user-friendly message when the LLM server is detected as down.
fn format_server_down_error(err: &str) -> String {
    log::error!("[AgentLoop] Local LLM server is offline: {}", err);
    "⚠️ Local LLM server is offline (connection refused).\n     Please restart the local LLM server and try again.\n\n     In the sidebar: stop the LLM server, then start it again.".to_string()
}

// -- Tool concurrency classification --
//
// "Shared" tools are read-only with no observable side-effect on disk, network state,
// or remote process state — safe to run in parallel inside one LLM turn.
// "Exclusive" tools (shell_exec, file_write, file_edit, upload/download, deploy)
// must run sequentially because the model's next call may depend on what the
// previous one produced (e.g. file_write then shell_exec).
fn is_shared_tool(name: &str) -> bool {
    matches!(
        name,
        "file_read" | "grep" | "glob" | "web_fetch" | "get_sudo_password"
    )
}

// -- Install-intent validator --
//
// The model occasionally substitutes a similarly-named product when generating
// install commands ("install OpenClaw" → `npm install -g opencode-ai`). The
// system prompt's tool-identity table tries to prevent this but model attention
// is not reliable enough to lean on it alone, so this is a deterministic check:
// before a shell_exec install command runs, we compare what the user asked for
// against what the command would actually install. If they disagree, we short-
// circuit with a synthetic tool_result so the model has to re-plan instead of
// silently installing the wrong thing.
//
// Only fires for the small set of products that are actually confusable. If
// either side is unidentified, the command passes through.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentTarget {
    OpenClaw,
    OpenCode,
    MiMoCode,
    KiloCode,
    KimiCode,
    ClaudeCode,
    Codex,
}

impl AgentTarget {
    fn label(&self) -> &'static str {
        match self {
            Self::OpenClaw => "OpenClaw",
            Self::OpenCode => "OpenCode",
            Self::MiMoCode => "MiMo Code",
            Self::KiloCode => "Kilo Code",
            Self::KimiCode => "Kimi Code",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex CLI",
        }
    }
    fn canonical_install(&self) -> &'static str {
        match self {
            Self::OpenClaw => "npm install -g openclaw",
            Self::OpenCode => "npm install -g opencode-ai",
            Self::MiMoCode => "npm install -g @mimo-ai/cli  (or  curl -fsSL https://mimo.xiaomi.com/install | bash)",
            Self::KiloCode => "npm install -g @kilocode/cli  (or  curl -fsSL https://kilo.ai/cli/install | bash)",
            Self::KimiCode => "npm install -g @moonshot-ai/kimi-code  (or  curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash)",
            Self::ClaudeCode => "curl -fsSL https://claude.ai/install.sh | bash  (or  npm install -g @anthropic-ai/claude-code)",
            Self::Codex => "npm install -g @openai/codex",
        }
    }
}

fn detect_user_intent(messages: &[Message]) -> Option<AgentTarget> {
    // Walk backward; the most recent user message defines current intent.
    for msg in messages.iter().rev() {
        if msg.role != "user" {
            continue;
        }
        let text = match &msg.content {
            MessageContent::Text(t) => t.to_lowercase(),
            MessageContent::Blocks(blocks) => {
                let mut out = String::new();
                for b in blocks {
                    if let ContentBlock::Text { text } = b {
                        out.push_str(&text.to_lowercase());
                        out.push(' ');
                    }
                }
                if out.is_empty() {
                    continue;
                }
                out
            }
        };
        // Order matters: more specific names first to avoid "claude code" matching
        // a generic "claude" mention.
        if text.contains("openclaw") || text.contains("open claw") || text.contains("openclaude") {
            return Some(AgentTarget::OpenClaw);
        }
        // Before OpenCode: MiMo Code is an OpenCode fork and the most likely
        // wrong-package target. Bare "mimo" is deliberately NOT matched — it
        // usually refers to the MiMo model/API, not the CLI (skip-on-uncertainty).
        if text.contains("mimocode") || text.contains("mimo code") || text.contains("mimo-code") {
            return Some(AgentTarget::MiMoCode);
        }
        // Kilo Code: bare "kilo" deliberately NOT matched — it usually refers
        // to the Kilo platform/model, not the CLI (skip-on-uncertainty, same
        // as mimo).
        if text.contains("kilocode") || text.contains("kilo code") || text.contains("kilo-code") {
            return Some(AgentTarget::KiloCode);
        }
        // Kimi Code: bare "kimi" deliberately NOT matched — it usually refers
        // to the Kimi model/API, not the CLI (skip-on-uncertainty, same as mimo).
        if text.contains("kimicode") || text.contains("kimi code") || text.contains("kimi-code") {
            return Some(AgentTarget::KimiCode);
        }
        if text.contains("opencode") || text.contains("open code") {
            return Some(AgentTarget::OpenCode);
        }
        if text.contains("claude code")
            || text.contains("claudecode")
            || text.contains("claude-code")
        {
            return Some(AgentTarget::ClaudeCode);
        }
        if text.contains("codex") {
            return Some(AgentTarget::Codex);
        }
        return None; // Latest user message has no install target — don't validate.
    }
    None
}

fn detect_command_target(command: &str) -> Option<AgentTarget> {
    let cmd = command.to_lowercase();
    // Must look like an install operation, otherwise we don't validate
    // (don't false-positive on `which openclaw`, `npm view opencode-ai`, etc.).
    let is_install_op = cmd.contains("install")
        || cmd.contains("brew add")
        || cmd.contains("yarn add")
        || cmd.contains("pnpm add")
        || cmd.contains("cargo install")
        || cmd.contains("pip install")
        || (cmd.contains("curl")
            && (cmd.contains("install.sh")
                || cmd.contains("install.ps1")
                || cmd.contains("| bash")
                || cmd.contains("| sh")));
    if !is_install_op {
        return None;
    }

    // Order matters: check the more-specific package strings first.
    if cmd.contains("@anthropic-ai/claude-code") || cmd.contains("claude.ai/install") {
        return Some(AgentTarget::ClaudeCode);
    }
    if cmd.contains("@openai/codex") {
        return Some(AgentTarget::Codex);
    }
    if cmd.contains("@mimo-ai/cli") || cmd.contains("mimo.xiaomi.com/install") {
        return Some(AgentTarget::MiMoCode);
    }
    if cmd.contains("@kilocode/cli") || cmd.contains("kilo.ai/cli/install") {
        return Some(AgentTarget::KiloCode);
    }
    if cmd.contains("@moonshot-ai/kimi-code")
        || cmd.contains("code.kimi.com/kimi-code/install")
        || cmd.contains("brew install kimi-code")
    {
        return Some(AgentTarget::KimiCode);
    }
    if cmd.contains("opencode-ai") || (cmd.contains("install") && cmd.contains(" opencode")) {
        return Some(AgentTarget::OpenCode);
    }
    if cmd.contains(" openclaw") || cmd.ends_with("openclaw") || cmd.contains("openclaw.ai/install")
    {
        return Some(AgentTarget::OpenClaw);
    }
    None
}

/// Returns Err with a model-facing message when the install command's package
/// disagrees with what the user asked for. Returns Ok(()) when alignment is
/// confirmed OR when either side is unidentified (skip-on-uncertainty).
fn validate_install_intent(command: &str, messages: &[Message]) -> Result<(), String> {
    let intent = match detect_user_intent(messages) {
        Some(x) => x,
        None => return Ok(()),
    };
    let target = match detect_command_target(command) {
        Some(x) => x,
        None => return Ok(()),
    };
    if intent == target {
        return Ok(());
    }
    Err(format!(
        "INTENT MISMATCH (install_intent_validator):\n\
         The user asked you to install {} but this command would install {}.\n\
         These are different products — DO NOT proceed. \
         Re-read the user's request and run the correct install instead.\n\n\
         Canonical install for {}: {}",
        intent.label(),
        target.label(),
        intent.label(),
        intent.canonical_install(),
    ))
}

#[cfg(test)]
mod loop_detection_tests {
    use super::*;

    #[test]
    fn hash_stable_across_whitespace_and_key_order() {
        let a = loop_args_hash("read_file", r#"{"path":"/a.rs","line":3}"#);
        let b = loop_args_hash("read_file", r#"{ "path": "/a.rs", "line": 3 }"#);
        let c = loop_args_hash("read_file", r#"{"line":3,"path":"/a.rs"}"#);
        assert_eq!(a, b, "whitespace must not affect hash");
        assert_eq!(a, c, "key order must not affect hash");
    }

    #[test]
    fn hash_distinguishes_tool_name() {
        let a = loop_args_hash("read_file", r#"{"path":"/x"}"#);
        let b = loop_args_hash("file_edit", r#"{"path":"/x"}"#);
        assert_ne!(a, b);
    }

    #[test]
    fn hash_distinguishes_args() {
        let a = loop_args_hash("read_file", r#"{"path":"/a"}"#);
        let b = loop_args_hash("read_file", r#"{"path":"/b"}"#);
        assert_ne!(a, b);
    }

    #[test]
    fn malformed_args_fall_back_to_raw_string() {
        // Two identical malformed strings must collide; differing ones must not.
        let a = loop_args_hash("read_file", "not json");
        let b = loop_args_hash("read_file", "not json");
        let c = loop_args_hash("read_file", "also not json");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn third_repeat_trips_loop_guard() {
        let mut s = AgentSession::new();
        let h = 42_u64;
        assert!(s.record_call_and_detect_loop(h).is_none(), "1st call ok");
        assert!(s.record_call_and_detect_loop(h).is_none(), "2nd call ok");
        assert!(
            s.record_call_and_detect_loop(h).is_some(),
            "3rd call must trip"
        );
    }

    #[test]
    fn distinct_calls_do_not_trip() {
        let mut s = AgentSession::new();
        for h in [1u64, 2, 3, 4, 5, 6] {
            assert!(s.record_call_and_detect_loop(h).is_none());
        }
    }

    #[test]
    fn prepare_run_resets_recent_calls() {
        let mut s = AgentSession::new();
        let h = 99_u64;
        s.record_call_and_detect_loop(h);
        s.record_call_and_detect_loop(h);
        s.prepare_run();
        // After reset the same hash must not trip until 3 fresh repeats.
        assert!(s.record_call_and_detect_loop(h).is_none());
        assert!(s.record_call_and_detect_loop(h).is_none());
        assert!(s.record_call_and_detect_loop(h).is_some());
    }

    #[test]
    fn ring_buffer_does_not_grow_unboundedly() {
        let mut s = AgentSession::new();
        for h in 0..100u64 {
            s.record_call_and_detect_loop(h);
        }
        assert!(s.recent_calls.len() <= RECENT_CALLS_CAPACITY);
    }
}
