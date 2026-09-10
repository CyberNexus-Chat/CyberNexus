//! Shared model configuration for Codex CLI and ChatGPT Desktop.

use super::{
    echobird_dir, ensure_parent, toml_delete_top, toml_read_table_value, toml_read_top,
    toml_write_table_value, toml_write_table_value_raw, toml_write_top, toml_write_top_raw,
    write_json_file, ApplyResult, ModelInfo,
};
use crate::services::codex_catalog;
use crate::services::codex_proxy::CODEX_PROXY_PORT;
use std::fs;
use std::path::Path;

/// Canonical Codex config identity. Every apply_codex run produces a
/// config.toml with the same provider name and display model regardless
/// of which third-party endpoint is actually behind the proxy — keeps
/// the config file clean and avoids stale orphan sections accumulating
/// across model switches. The launcher proxy translates the display
/// model to the real provider's model ID when forwarding requests.
const CODEX_PROVIDER: &str = "OpenAI";

/// Codex's display model alias. Pinned in Bridge and Relay sessions so
/// Codex thinks it's talking to a gpt-5 family model (model-id deception
/// moat); the proxy / relay station rewrites to the real id. Exposed
/// `pub(crate)` so `codex_proxy::config_manager` can pass it to
/// `write_codex_canonical_fields` on the pre-spawn self-heal path.
pub(crate) const CODEX_DISPLAY_MODEL: &str = "gpt-5.5";

// ─── Per-model capability registry ───
//
// Some tool configs must reflect a model's real context window and input
// modalities rather than a one-size-fits-all default. Codex's
// `model_context_window` / `model_auto_compact_token_limit` and ZCode's
// `modalities.input` are the two places a wrong default silently caps or
// mis-advertises a third-party model. The values below are sourced from the
// provider's official model specs so apply_codex / apply_zcode stay
// data-driven — no model-id branching inside the apply logic.

pub(crate) const DEFAULT_CODEX_CONTEXT_WINDOW: u64 = 1_000_000;

/// Look up a model's real context window (in tokens). Returns
/// `DEFAULT_CODEX_CONTEXT_WINDOW` for models the registry does not list —
/// the historic Codex default — so unknown models keep working as before.
fn model_context_window_for(model_id: &str) -> u64 {
    match model_id {
        "MiniMax-M3" => 1_000_000,
        "MiniMax-M2.7" => 204_800,
        _ => DEFAULT_CODEX_CONTEXT_WINDOW,
    }
}

/// Derive the auto-compact token limit (90% of the context window) that
/// Codex writes as `model_auto_compact_token_limit`. Keeping it proportional
/// to the real window stops Codex from trying to compact at 900k on a model
/// whose entire window is only 204,800 tokens.
fn codex_compact_limit_for(context_window: u64) -> u64 {
    context_window * 9 / 10
}

// Codex CLI and ChatGPT desktop share ~/.codex/config.toml.

/// Apply our 10 canonical Codex fields surgically — overwrite if
/// present, insert if missing — and return the rewritten content.
/// Preserves everything else in the file (`[projects.*]` trust grants,
/// `[tui.*]` NUX progress, `[plugins.*]` state, comments, hand-edited
/// top-level keys).
///
/// This is the bottom-out for cases where a sibling model-switcher
/// (cc-switch, manual edits, a different tool) rewrote keys we own
/// (`model_provider`, `model`, `wire_api`, `requires_openai_auth`, etc.)
/// to point at a different provider. Without this, a v4.8.x `apply_codex`
/// that only flipped `base_url` would leave the rest of the sibling
/// tool's edits in place and Codex would behave wrong (wrong model id,
/// wrong wire protocol, wrong reasoning effort).
///
/// Used by both:
///   • `apply_codex` (this file) — every model switch
///   • `codex_proxy::config_manager::ensure_canonical_config` — every
///     Codex spawn (pre-launch self-heal)
///
/// `codex_base_url` is the URL Codex will see in config.toml. In Bridge
/// mode this is `http://127.0.0.1:53682/v1`; in Relay and Responses-
/// direct modes it's the real upstream URL (Codex skips our proxy).
/// Write the model id the caller chose — Bridge and Relay sessions pin
/// Codex's `gpt-5.5` display alias (CODEX_DISPLAY_MODEL); a Responses
/// direct-connect session passes the real upstream model id (e.g.
/// `glm-5.2`) so Codex talks to the third party in its own id.
///
/// `context_window` is the real token limit of the selected model. Codex
/// writes it as `model_context_window` and derives
/// `model_auto_compact_token_limit` as 90% of it, so a model whose window
/// is smaller than the historic 1M default is not over-claimed.
pub(crate) fn write_codex_canonical_fields(
    content: &str,
    codex_base_url: &str,
    model: &str,
    context_window: u64,
) -> String {
    // Preserve the input's trailing-newline convention. `toml_write_*`
    // helpers go through `content.lines().collect().join("\n")` which
    // strips trailing newlines; without re-adding it, a canonical-input
    // round-trip would always show as a one-byte diff and trigger
    // pointless rewrites (e.g. ensure_canonical_config flapping from
    // "already-canonical" to "drifted" on every Codex spawn).
    let trailing_nl = content.ends_with('\n');
    let mut c = content.to_string();

    // Top-level string keys.
    c = toml_write_top(&c, "model_provider", CODEX_PROVIDER);
    c = toml_write_top(&c, "model", model);
    c = toml_write_top(&c, "model_reasoning_effort", "high");
    // Evict legacy keys we no longer own. Older EchoBird versions wrote
    // `review_model = "gpt-5.5"`; we stopped writing it (Codex no longer
    // consumes it). But our TOML helpers are update-or-insert — they
    // never delete — so a stale line from an old version survives every
    // switch and self-heal. Under a Responses direct-connect session
    // (model = the real upstream id, e.g. "glm-5.2"; base_url = the real
    // upstream) Codex would still send `gpt-5.5` on its review pass to a
    // gateway that only knows the real id → 4xx. Strip it on every write
    // so the canonical set we write below is the full top-level truth.
    c = toml_delete_top(&c, "review_model");
    // Evict any stale `model_catalog_json` from a previous Responses-direct
    // session. The line is conditional (only written by apply_codex for
    // passthrough + bundled-vendor catalogs), so a plain overwrite-or-insert
    // helper would never remove it after switching back to bridge/relay mode
    // or to a non-catalog vendor — leaving config.toml pointing at a catalog
    // that no longer matches the selected model. apply_codex re-adds it when
    // the catalog applies.
    c = toml_delete_top(&c, "model_catalog_json");
    // Top-level raw (bool, int).
    c = toml_write_top_raw(&c, "disable_response_storage", "true");
    c = toml_write_top_raw(&c, "model_context_window", &context_window.to_string());
    c = toml_write_top_raw(
        &c,
        "model_auto_compact_token_limit",
        &codex_compact_limit_for(context_window).to_string(),
    );

    // [model_providers.OpenAI] string keys.
    let table = format!("model_providers.{}", CODEX_PROVIDER);
    c = toml_write_table_value(&c, &table, "name", CODEX_PROVIDER);
    c = toml_write_table_value(&c, &table, "base_url", codex_base_url);
    c = toml_write_table_value(&c, &table, "wire_api", "responses");
    // [model_providers.OpenAI] raw (bool).
    c = toml_write_table_value_raw(&c, &table, "requires_openai_auth", "true");

    if trailing_nl && !c.ends_with('\n') {
        c.push('\n');
    }
    c
}

/// Whether `content` (a config.toml) references OUR canonical catalog path in
/// `model_catalog_json`. Used by `apply_codex` to decide when to delete the
/// stale `~/.codex/models.json` file after leaving catalog mode. The check is
/// exact-path, not substring: a user's own catalog pointed at via a different
/// path (e.g. MiniMax docs' `~/.codex/model-catalogs/custom-catalog.json`)
/// must NOT trigger deletion of our file. Accepts both the absolute
/// forward-slash form we write and the `~/.codex/models.json` shorthand the
/// vendor docs use.
fn codex_catalog_referenced(content: &str, our_path: &str) -> bool {
    let referenced = toml_read_top(content, "model_catalog_json");
    !referenced.is_empty() && (referenced == our_path || referenced == "~/.codex/models.json")
}

pub(super) fn apply_codex(tool_id: &str, model_info: &ModelInfo) -> ApplyResult {
    // Two write modes, picked by `model_info.relay_mode`:
    //
    // • Bridge (default): config.toml's base_url is permanently
    //   "http://127.0.0.1:53682/v1" (CODEX_PROXY_PORT). The proxy
    //   reads ~/.echobird/codex.json on every request and forwards
    //   with Responses ↔ Chat translation as needed. Same shape
    //   across model switches, so Codex's runtime state in config.toml
    //   ([projects.*] trust, [tui.*] NUX) survives switches.
    //
    // • Relay (relay_mode = true): config.toml's base_url is the
    //   provider's REAL upstream URL. Codex talks to it directly. Used
    //   for relay stations (cc-vibe.com etc.) that already speak the
    //   Responses protocol — no proxy hop, no translation. The local
    //   proxy stays running but Codex doesn't touch it for this
    //   provider.

    let codex_dir = dirs::home_dir().unwrap_or_default().join(".codex");
    let config_path = codex_dir.join("config.toml");
    let auth_path = codex_dir.join("auth.json");

    let model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    if model_id.is_empty() {
        return ApplyResult {
            success: false,
            message: "Model ID is empty".to_string(),
        };
    }

    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string();

    // Reject ONLY the codex_proxy's own port (53682). Applying that as the
    // upstream would make the proxy forward every request back to itself —
    // an infinite loop. Other 127.0.0.1 ports are legitimate upstreams
    // (most importantly 127.0.0.1:11434, EchoBird's local-LLM proxy that
    // sits in front of llama-server), so the broader "no localhost"
    // blanket-ban previously here was too aggressive and made Codex +
    // local-LLM combinations impossible.
    if base_url.contains(":53682") {
        return ApplyResult {
            success: false,
            message: "Cannot use EchoBird's own Codex proxy (127.0.0.1:53682) as the provider — that would create a forwarding loop. Pick a real provider, or use the local LLM endpoint (127.0.0.1:11434).".to_string(),
        };
    }

    // For local-LLM endpoints (127.0.0.1 / localhost — but NOT our own
    // codex_proxy port 53682, which we already rejected above), llama-server
    // ignores the API key entirely. Codex CLI, on the other hand, refuses to
    // start when OPENAI_API_KEY is empty. Substitute a non-empty dummy so
    // users don't have to invent a fake key in the Model Center just to use
    // their own local model.
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider = base_url.contains("127.0.0.1") || base_url.contains("localhost");
    let api_key = if raw_api_key.is_empty() {
        if is_local_provider {
            "local-no-auth"
        } else {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Codex config".to_string(),
            };
        }
    } else {
        raw_api_key
    };

    // Resolve the real context window for the selected model so Codex writes
    // `model_context_window` / `model_auto_compact_token_limit` matching the
    // model's actual token budget rather than the historic 1M default.
    let context_window = model_context_window_for(model_id);

    // Resolve the URL + model id Codex itself will see in its config.toml.
    // Three routing modes:
    //   • Bridge (default): base_url = our proxy port; model = "gpt-5.5"
    //     display alias. The proxy reads ~/.echobird/codex.json per request
    //     and forwards with Responses ↔ Chat translation as needed.
    //   • Relay (relay_mode): base_url = real upstream; model = "gpt-5.5"
    //     display alias (relay stations accept the alias and map it
    //     themselves — keep the model-id deception moat). Codex skips the
    //     proxy entirely.
    //   • Responses direct (responses_passthrough): base_url = real
    //     upstream; model = the REAL upstream model id (e.g. "glm-5.2").
    //     For third parties that natively speak the Responses protocol —
    //     Codex connects straight to them, no proxy hop, no translation,
    //     no id spoof. The two direct modes are mutually exclusive (UI
    //     auto-flips), so we force passthrough off when relay is on.
    let relay_mode = model_info.relay_mode.unwrap_or(false);
    let responses_passthrough = !relay_mode && model_info.responses_passthrough.unwrap_or(false);
    let direct_mode = relay_mode || responses_passthrough;
    let proxy_base_url = format!("http://127.0.0.1:{}/v1", CODEX_PROXY_PORT);
    let codex_base_url = if direct_mode {
        base_url.clone()
    } else {
        proxy_base_url.clone()
    };
    // Direct Responses connect needs the real model id; every other mode
    // pins Codex's "gpt-5.5" display alias.
    let codex_model = if responses_passthrough {
        model_id
    } else {
        CODEX_DISPLAY_MODEL
    };

    ensure_parent(&config_path);

    // Canonicalize ALL 10 fields we own, every time. Overwrite-in-place
    // if present, insert if missing. This is the bottom-out for sibling
    // model-switchers (cc-switch, manual edits, etc.) that may have
    // rewritten our keys to point at a different provider — we restore
    // canonical shape end-to-end, not just `base_url`. Codex's own
    // runtime state (`[projects.*]` trust, `[tui.*]` NUX, `[plugins.*]`)
    // and any unrelated user-edited top-level keys stay untouched.
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let mut new_content =
        write_codex_canonical_fields(&existing, &codex_base_url, codex_model, context_window);

    // web_search: user toggle. `Some(false)` → "disabled" (Codex removes
    // its built-in search tool); ON → "live" — unrestricted live retrieval,
    // i.e. actual real-time web search. NOT Codex's default "cached": that
    // uses an OpenAI-maintained index with NO external web access, which is
    // meaningless for our third-party upstreams (OpenAI's index doesn't
    // cover them) and isn't "web search on" as a user expects the toggle to
    // mean. Written here (not in write_codex_canonical_fields) so the
    // pre-spawn self-heal leaves the user's choice untouched.
    let web_search_value = if model_info.web_search == Some(false) {
        "disabled"
    } else {
        "live"
    };
    new_content = toml_write_top(&new_content, "web_search", web_search_value);

    // Model catalog — Responses-direct third parties (DeepSeek / MiniMax /
    // MiMo) need `model_catalog_json` so Codex knows the real model's
    // context window, reasoning levels, and tool capabilities. Only written
    // in passthrough mode (Codex talks to the upstream directly with the
    // real model id); bridge + relay keep the display-alias shape and get no
    // catalog line. Vendors without a bundled catalog keep today's behavior.
    // The stale line is evicted by `write_codex_canonical_fields` on every
    // canonicalize, so switching away from passthrough (or to a non-catalog
    // vendor) can't leave a dangling pointer.
    let catalog_template = if responses_passthrough {
        codex_catalog::template_for_url(&base_url)
    } else {
        None
    };
    if let Some(template_str) = catalog_template {
        let catalog_path = codex_catalog::models_json_path();
        let template = serde_json::from_str(template_str).unwrap_or_default();
        // Stamp the SELECTED model onto the vendor capability template and
        // write a single-entry catalog — we never enumerate a vendor's model
        // versions, so `deepseek-v5-flash` / `mimo-v2.6` need no bundled
        // asset change.
        let catalog = codex_catalog::build_catalog(
            &template,
            model_id,
            model_info.name.as_deref().unwrap_or(model_id),
            context_window,
        );
        // Only add the config line if the file write succeeded — a dangling
        // model_catalog_json pointing at a missing file makes Codex error on
        // startup.
        if write_json_file(&catalog_path, &catalog).is_ok() {
            new_content = toml_write_top(
                &new_content,
                "model_catalog_json",
                &catalog_path.to_string_lossy(),
            );
        }
    } else {
        // Leaving catalog mode (passthrough off, or a non-bundled vendor):
        // the canonical write evicted the `model_catalog_json` line, so Codex
        // no longer reads the file. Delete the stale file at OUR canonical
        // path so the switch is disk-clean too — but ONLY when the previous
        // config actually referenced OUR canonical path. A user's own catalog
        // pointed at a different path (e.g. MiniMax docs' custom-catalog.json)
        // must not trigger deletion of our file, and a config that never
        // mentioned a catalog must leave whatever's on disk alone.
        let catalog_path = codex_catalog::models_json_path();
        if codex_catalog_referenced(&existing, &catalog_path.to_string_lossy())
            && catalog_path.exists()
        {
            let _ = fs::remove_file(&catalog_path);
        }
    }

    // Only write if content actually changed — avoids touching mtime
    // for no-op applies and avoids unnecessary fs traffic.
    if new_content != existing {
        if let Err(e) = fs::write(&config_path, &new_content) {
            return ApplyResult {
                success: false,
                message: format!("Codex config error: {}", e),
            };
        }
    }

    // Back up any existing auth.json (OAuth-token sign-ins, prior api-key
    // configs, etc.) before overwriting so restore-to-official can put it
    // back. We keep one snapshot per session — apply_codex called multiple
    // times in a row preserves the FIRST snapshot, not the most recent
    // (which would clobber the original OAuth state with our apikey state).
    let auth_backup_path = echobird_dir().join("codex-auth.bak.json");
    if auth_path.exists() && !auth_backup_path.exists() {
        if let Ok(existing) = fs::read(&auth_path) {
            ensure_parent(&auth_backup_path);
            let _ = fs::write(&auth_backup_path, existing);
        }
    }

    // Write the api-key auth.json that Codex v0.130+ expects.
    let auth_payload = serde_json::json!({ "OPENAI_API_KEY": api_key });
    ensure_parent(&auth_path);
    if let Err(e) = fs::write(
        &auth_path,
        serde_json::to_string_pretty(&auth_payload).unwrap_or_default(),
    ) {
        return ApplyResult {
            success: false,
            message: format!("Codex auth.json error: {}", e),
        };
    }

    // The live relay — in Bridge mode the proxy reads this on every
    // request, so it must reflect the upstream we want forwarded to.
    // In Relay mode the proxy is bypassed for Codex, but we still
    // write the same file so `read_codex` (used by the model-picker
    // UI to round-trip the current selection) and ensure_canonical_config
    // (which reads `relayMode` to decide whether to self-heal config.toml)
    // both see consistent state.
    let relay_path = echobird_dir().join("codex.json");
    let relay = serde_json::json!({
        "apiKey": api_key,
        "baseUrl": base_url,
        "displayModel": CODEX_DISPLAY_MODEL,
        "actualModel": model_id,
        "modelName": model_info.name.as_deref().unwrap_or(model_id),
        "providerId": CODEX_PROVIDER,
        "relayMode": relay_mode,
        "responsesPassthrough": responses_passthrough,
        "contextWindow": context_window,
    });
    let _ = write_json_file(&relay_path, &relay);

    let display = if tool_id == "chatgptdesktop" {
        "ChatGPT"
    } else {
        "Codex CLI"
    };
    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for {}.",
            model_info.name.as_deref().unwrap_or(model_id),
            display
        ),
    }
}

pub(super) fn read_codex() -> Option<ModelInfo> {
    let codex_dir = dirs::home_dir()?.join(".codex");
    let content = fs::read_to_string(codex_dir.join("config.toml")).ok()?;

    let model_from_toml = toml_read_top(&content, "model");
    if model_from_toml.is_empty() {
        return None;
    }

    let provider_id = toml_read_top(&content, "model_provider");
    let mut base_url = if provider_id.is_empty() {
        None
    } else {
        let value = toml_read_table_value(
            &content,
            &format!("model_providers.{}", provider_id),
            "base_url",
        );
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    };

    // If base_url is EchoBird's own codex_proxy (port 53682), read the real
    // provider URL from the relay file for UI display. The launcher rewrites
    // config.toml to point at 127.0.0.1:53682 while running, but the UI should
    // show users the actual provider they configured (e.g., api.xiaomimimo.com).
    // This does NOT affect Codex's runtime behavior — Codex always reads from
    // config.toml, which the launcher controls. Matching ONLY :53682 (not all
    // 127.0.0.1) means a legitimate local-LLM endpoint such as
    // 127.0.0.1:11434 stays visible as-is — that IS the real provider.
    if let Some(ref url) = base_url {
        if url.contains(":53682") {
            base_url = read_codex_relay_base_url();
        }
    }

    // When we wrote the canonical OpenAI provider, config.toml's `model`
    // is the display alias ("gpt-5.5"), not the real third-party model.
    // The UI needs the real one to round-trip a meaningful selection back
    // to the user — read it from the relay file. Fall back to the
    // config.toml value for non-canonical setups (e.g., user manually
    // edited their config to point at a different provider).
    let model = if provider_id == CODEX_PROVIDER {
        read_codex_relay_model().unwrap_or(model_from_toml)
    } else {
        model_from_toml
    };

    // API key now lives in ~/.codex/auth.json (preferred_auth_method=apikey).
    // Fall back to the legacy env_key path for configs written before this change.
    let api_key = read_codex_auth_key(&codex_dir).or_else(|| {
        let env_key = if provider_id.is_empty() {
            String::new()
        } else {
            toml_read_table_value(
                &content,
                &format!("model_providers.{}", provider_id),
                "env_key",
            )
        };
        if env_key.is_empty() {
            None
        } else {
            std::env::var(&env_key).ok()
        }
    });

    Some(ModelInfo {
        name: Some(model.clone()),
        model: Some(model),
        base_url,
        api_key,
        anthropic_url: None,
        protocol: Some("openai".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

fn read_codex_relay_base_url() -> Option<String> {
    let relay_path = echobird_dir().join("codex.json");
    let content = fs::read_to_string(relay_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("baseUrl").and_then(|x| x.as_str()).map(String::from)
}

fn read_codex_relay_model() -> Option<String> {
    let relay_path = echobird_dir().join("codex.json");
    let content = fs::read_to_string(relay_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("actualModel")
        .or_else(|| v.get("modelName"))
        .and_then(|x| x.as_str())
        .map(String::from)
}

fn read_codex_auth_key(codex_dir: &Path) -> Option<String> {
    let content = fs::read_to_string(codex_dir.join("auth.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("OPENAI_API_KEY")
        .and_then(|x| x.as_str())
        .map(String::from)
}

pub(super) fn restore_codex_to_official(tool_id: &str, config_path: &Path) -> ApplyResult {
    // Full-file overwrite. ChatGPT's model picker shows the
    // `model_provider` id VERBATIM, so the built-in lowercase id
    // "openai" rendered as a lowercase "openai" chip — inconsistent with
    // the third-party path, which uses a capitalized "OpenAI" provider.
    // Point at a capitalized "OpenAI" provider so the chip casing
    // matches everywhere.
    //
    // We deliberately do NOT set `base_url`. Codex's `to_api_provider`
    // resolves an unset base_url from the AUTH MODE — chatgpt.com/
    // backend-api/codex for a ChatGPT login, api.openai.com otherwise —
    // which is exactly the built-in openai behavior that keeps
    // ChatGPT-account users working. `requires_openai_auth = true`
    // reproduces the built-in's login-screen / auth.json handling. (A
    // table keyed lowercase "openai" would be silently dropped: Codex's
    // merge does `entry(key).or_insert`, and the built-in already owns
    // that key — only a new "OpenAI" key takes effect.)
    //
    // We also write no `model` line — pinning one (we used to pin
    // "gpt-4o") breaks ChatGPT-account users because OpenAI rejects
    // gpt-4o for that auth path. Without it Codex selects an
    // auth-appropriate default (gpt-5-codex for ChatGPT, otherwise its
    // built-in default). Codex regenerates everything else
    // (projects/marketplaces/tui state) on next launch.
    let content = "model_provider = \"OpenAI\"\n\
                   \n\
                   [model_providers.OpenAI]\n\
                   name = \"OpenAI\"\n\
                   wire_api = \"responses\"\n\
                   requires_openai_auth = true\n";

    ensure_parent(config_path);
    match fs::write(config_path, content) {
        Ok(_) => {
            // Restore auth.json from our backup if we have one (OAuth
            // tokens, prior api-key from before the third-party detour).
            //
            // We do NOT delete auth.json when no backup exists. The old
            // behavior was "fall through to Codex's own login flow",
            // but deleting auth.json out from under a running Codex
            // process produces a worse failure mode: the in-memory
            // React state and the on-disk auth become inconsistent,
            // which surfaces as a "fake account"
            // displayed in the Codex sidebar and silently partitions
            // any chats the user created during the third-party
            // session into an inaccessible namespace. Leaving the
            // third-party apikey in place at worst causes a 401 on
            // next Codex request, which Codex handles loudly via its
            // own re-login UI — better than silent data loss. Users
            // who actually want to log out should use Codex's own
            // logout button.
            let auth_path = config_path
                .parent()
                .unwrap_or(Path::new(""))
                .join("auth.json");
            let auth_backup_path = echobird_dir().join("codex-auth.bak.json");
            if auth_backup_path.exists() {
                if let Ok(bak) = fs::read(&auth_backup_path) {
                    let _ = fs::write(&auth_path, bak);
                    let _ = fs::remove_file(&auth_backup_path);
                }
            }

            let relay_path = echobird_dir().join("codex.json");
            if relay_path.exists() {
                let _ = fs::remove_file(&relay_path);
            }
            ApplyResult {
                success: true,
                message: format!(
                    "{} restored to OpenAI official provider.",
                    if tool_id == "chatgptdesktop" {
                        "ChatGPT"
                    } else {
                        "Codex CLI"
                    }
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: format!("Failed to restore Codex config: {}", e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_codex_canonical_fields_evicts_stale_review_model_on_direct_connect() {
        // Regression: an older EchoBird version wrote `review_model =
        // "gpt-5.5"`. Our write helpers never delete, so it survived every
        // switch. Under a Responses direct-connect session (real model id
        // + real upstream base_url) Codex would still send `gpt-5.5` on its
        // review pass to a gateway that only knows the real id → 4xx. The
        // canonical write must strip the stale line so `model` is the only
        // model key the upstream ever sees.
        let stale = "model_provider = \"OpenAI\"\n\
                     model = \"gpt-5.5\"\n\
                     review_model = \"gpt-5.5\"\n\
                     [model_providers.OpenAI]\n\
                     name = \"OpenAI\"\n";
        let out = write_codex_canonical_fields(
            stale,
            "https://ark.cn-beijing.volces.com/api/coding/v1",
            "glm-5.2",
            DEFAULT_CODEX_CONTEXT_WINDOW,
        );
        assert!(
            !out.contains("review_model"),
            "stale review_model survived: {out}"
        );
        assert!(out.contains("model = \"glm-5.2\""));
        assert!(out.contains("base_url = \"https://ark.cn-beijing.volces.com/api/coding/v1\""));
    }

    #[test]
    fn write_codex_canonical_fields_evicts_review_model_in_bridge_mode_too() {
        // Bridge mode (proxy base_url + gpt-5.5 alias) must ALSO strip a
        // stale review_model — the pre-spawn self-heal runs write_codex
        // _canonical_fields too, and a cold start shouldn't leave a stale
        // value lying around even when not on direct connect.
        let stale = "model = \"gpt-5.5\"\n\
                     review_model = \"gpt-5.5\"\n\
                     [model_providers.OpenAI]\n";
        let out = write_codex_canonical_fields(
            stale,
            "http://127.0.0.1:53682/v1",
            "gpt-5.5",
            DEFAULT_CODEX_CONTEXT_WINDOW,
        );
        assert!(!out.contains("review_model"));
    }

    #[test]
    fn write_codex_canonical_fields_evicts_stale_model_catalog_json() {
        // Regression: `model_catalog_json` is conditional (apply_codex writes
        // it only for Responses passthrough + bundled-vendor catalogs). After
        // switching back to bridge mode — or to a non-catalog vendor — the
        // stale line must not survive, or config.toml points at a catalog that
        // no longer matches the selected model. Same never-delete-helper
        // problem as review_model: the canonical write owns the eviction.
        let stale = "model_provider = \"OpenAI\"\n\
                     model = \"gpt-5.5\"\n\
                     model_catalog_json = \"C:/Users/x/.codex/models.json\"\n\
                     [model_providers.OpenAI]\n\
                     name = \"OpenAI\"\n";
        let out = write_codex_canonical_fields(
            stale,
            "http://127.0.0.1:53682/v1",
            "gpt-5.5",
            DEFAULT_CODEX_CONTEXT_WINDOW,
        );
        assert!(
            !out.contains("model_catalog_json"),
            "stale model_catalog_json survived: {out}"
        );
    }

    #[test]
    fn codex_catalog_referenced_matches_only_our_canonical_path() {
        // Our absolute forward-slash form.
        let ours = "C:/Users/x/.codex/models.json";
        let abs = "model_provider = \"OpenAI\"\n\
                   model_catalog_json = \"C:/Users/x/.codex/models.json\"\n\
                   [model_providers.OpenAI]\n";
        assert!(codex_catalog_referenced(abs, ours));
        // Vendor-doc tilde shorthand resolves to the same file.
        let tilde = "model_catalog_json = \"~/.codex/models.json\"\n";
        assert!(codex_catalog_referenced(tilde, ours));
        // A user's own catalog pointed at a DIFFERENT path must NOT match.
        let other_path = "model_catalog_json = \"~/.codex/model-catalogs/custom-catalog.json\"\n";
        assert!(!codex_catalog_referenced(other_path, ours));
        // Config with no catalog line must not match.
        let no_line = "model = \"gpt-5.5\"\n[model_providers.OpenAI]\n";
        assert!(!codex_catalog_referenced(no_line, ours));
    }

    // ── Per-model context window + compaction (Codex) ──
    // apply_codex must write the real model_context_window and a proportional
    // model_auto_compact_token_limit (90% of the window) so a model whose
    // window is smaller than the historic 1M default is not over-claimed.

    #[test]
    fn model_context_window_for_known_model() {
        assert_eq!(model_context_window_for("MiniMax-M2.7"), 204_800);
    }

    #[test]
    fn model_context_window_for_unknown_model_defaults_to_1m() {
        assert_eq!(
            model_context_window_for("glm-5.2"),
            DEFAULT_CODEX_CONTEXT_WINDOW
        );
    }

    #[test]
    fn codex_compact_limit_is_90_percent_of_window() {
        assert_eq!(codex_compact_limit_for(1_000_000), 900_000);
        assert_eq!(codex_compact_limit_for(204_800), 184_320);
    }

    #[test]
    fn write_codex_canonical_fields_writes_model_context_window() {
        // A 204,800-token model must get model_context_window = 204800 and
        // model_auto_compact_token_limit = 184320 (90%), not the historic
        // 1,000,000 / 900,000 defaults.
        let out = write_codex_canonical_fields(
            "model = \"gpt-5.5\"\n[model_providers.OpenAI]\n",
            "http://127.0.0.1:53682/v1",
            "gpt-5.5",
            204_800,
        );
        assert!(out.contains("model_context_window = 204800"), "got: {out}");
        assert!(
            out.contains("model_auto_compact_token_limit = 184320"),
            "got: {out}"
        );
        assert!(
            !out.contains("model_context_window = 1000000"),
            "got: {out}"
        );
        assert!(
            !out.contains("model_auto_compact_token_limit = 900000"),
            "got: {out}"
        );
    }
}
