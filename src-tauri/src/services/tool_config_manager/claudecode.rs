//! Model configuration for claudecode.

use super::{echobird_dir, read_json_file, write_json_file, ApplyResult, ModelInfo};
use crate::services::codex_proxy::CODEX_PROXY_PORT;
use std::fs;

pub(super) fn normalize_model_info_for_tool(tool_id: &str, mut model_info: ModelInfo) -> ModelInfo {
    if tool_id == "claudecode" && model_info.protocol.as_deref() == Some("anthropic") {
        if let Some(ref mut base_url) = model_info.base_url {
            let trimmed = base_url.trim_end_matches('/').to_string();
            if let Some(without_v1) = trimmed.strip_suffix("/v1") {
                *base_url = without_v1.to_string();
            } else {
                *base_url = trimmed;
            }
        }
    }
    model_info
}

// Model-tier env vars Claude Code consults. Bridge mode clears them so Claude
// Code falls back to its built-in claude-* ids (the proxy rewrites them);
// relay mode pins them all to the real upstream model. Hoisted to module
// scope so the set is lockable by tests — the apply path writes to
// ~/.claude/settings.json via dirs::home_dir(), which isn't injectable.
//
// `ANTHROPIC_SMALL_FAST_MODEL` is deliberately NOT here — Claude Code
// deprecated it in favor of `ANTHROPIC_DEFAULT_HAIKU_MODEL` (still pinned).
// `CLAUDE_CODE_SUBAGENT_MODEL` IS here so relay mode pins subagents to the
// upstream model too (otherwise subagents fall back to claude-* ids and
// bypass the third-party router, breaking the "全量" write-in).
// https://code.claude.com/docs/en/model-config
const CLAUDECODE_MODEL_VARS: [&str; 6] = [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
];

/// The 1M-capable subset of [`CLAUDECODE_MODEL_VARS`]: env vars that receive
/// a `[1m]` suffix when the user opts into the 1M context window. `HAIKU` and
/// `CLAUDE_CODE_SUBAGENT_MODEL` are deliberately excluded — Haiku has no 1M
/// tier, and subagents pin to the bare upstream id.
const CLAUDECODE_MODEL_VARS_1M: &[&str] = &[
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
];

/// Value written to a `CLAUDECODE_MODEL_VARS` entry in relay mode. Appends
/// `[1m]` only when the user opted in (`one_m`) AND `var` is in the 1M-capable
/// tier; `ANTHROPIC_DEFAULT_HAIKU_MODEL` / `CLAUDE_CODE_SUBAGENT_MODEL` always
/// return the bare id. Bridge mode never calls this — it removes the vars
/// instead of writing them, so the `relay` flag is the caller's responsibility.
fn claudecode_env_model_id(real_id: &str, var: &str, one_m: bool) -> String {
    if one_m && CLAUDECODE_MODEL_VARS_1M.contains(&var) {
        format!("{}[1m]", real_id)
    } else {
        real_id.to_string()
    }
}

/// Claude Code 3P config. Mirrors `apply_claudedesktop` but targets Claude
/// Code's env-var config (~/.claude/settings.json) instead of Desktop's
/// profile JSON. Two routing modes, picked by `model_info.relay_mode`:
///
/// • Bridge (default): point ANTHROPIC_BASE_URL at our local Anthropic proxy
///   (127.0.0.1:<port>/claudecode) and write NO model id, so Claude Code
///   sends its built-in claude-* ids and the proxy rewrites every request to
///   the real upstream model (read fresh from ~/.echobird/claudecode.json).
///   This is the "first-class citizen" path — strict upstreams never see a
///   claude-* name they'd reject, matching how Claude Desktop already works.
///
/// • Relay (relay_mode = true): write the real upstream URL + key + model id
///   straight into settings.json so Claude Code talks to the upstream
///   directly, bypassing the proxy. For relay stations that natively serve
///   Anthropic Messages and map the ids themselves. (≈ the legacy full-write.)
///
/// The relay side-channel (~/.echobird/claudecode.json) is written in BOTH
/// modes so `read_claudecode` can round-trip the active model back to the UI.
pub(super) fn apply_claudecode(model_info: &ModelInfo) -> ApplyResult {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            return ApplyResult {
                success: false,
                message: "Cannot resolve home directory.".to_string(),
            }
        }
    };
    let settings_path = home.join(".claude").join("settings.json");

    // Frontend collapses the chosen protocol's URL into base_url (same
    // convention as apply_claudedesktop). Accept either field.
    let anthropic_url = model_info
        .anthropic_url
        .as_deref()
        .or(model_info.base_url.as_deref())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(|u| {
            // Trim a trailing '/' and a trailing '/v1' so relay mode (where
            // Claude Code's SDK appends '/v1/messages' to ANTHROPIC_BASE_URL)
            // can't produce a doubled '/v1/v1/messages'. normalize already does
            // this for base_url; do it here too so the anthropic_url branch is
            // equally safe. Bridge mode is immune either way (the proxy
            // recomposes the path), but this keeps both modes consistent.
            let u = u.trim_end_matches('/');
            u.strip_suffix("/v1")
                .unwrap_or(u)
                .trim_end_matches('/')
                .to_string()
        });
    let anthropic_url = match anthropic_url {
        Some(u) => u,
        None => {
            return ApplyResult {
                success: false,
                message: "Base URL is empty. Pick a model first.".to_string(),
            }
        }
    };

    // Local providers (loopback) legitimately have no key — substitute a
    // sentinel so the relay always has something to forward as Bearer.
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider =
        anthropic_url.contains("127.0.0.1") || anthropic_url.contains("localhost");
    let api_key = if raw_api_key.is_empty() {
        if is_local_provider {
            "local-no-auth".to_string()
        } else {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Claude Code config.".to_string(),
            };
        }
    } else {
        raw_api_key.to_string()
    };

    let real_model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();

    let relay_mode = model_info.relay_mode.unwrap_or(false);
    let proxy_base = format!("http://127.0.0.1:{}/claudecode", CODEX_PROXY_PORT);

    // ── settings.json env block (preserve every other key the user has) ──
    // Only start from a fresh object when the file genuinely does NOT exist. If
    // it exists but won't parse as a JSON object (hand-edited typo, half-written
    // by another tool), ABORT — defaulting to {} here and writing back would
    // wipe the user's allowedTools / permissions / hooks / MCP config.
    let mut config = if settings_path.exists() {
        match read_json_file(&settings_path) {
            Some(v) if v.is_object() => v,
            _ => {
                return ApplyResult {
                    success: false,
                    message:
                        "~/.claude/settings.json exists but isn't valid JSON. Fix or remove it, then apply again — EchoBird won't overwrite it to avoid losing your Claude Code settings."
                            .to_string(),
                };
            }
        }
    } else {
        serde_json::json!({})
    };
    {
        let obj = config.as_object_mut().expect("config is an object");
        if !obj.get("env").map(|v| v.is_object()).unwrap_or(false) {
            obj.insert("env".to_string(), serde_json::json!({}));
        }
    }
    let env = config
        .get_mut("env")
        .and_then(|v| v.as_object_mut())
        .expect("env is an object");

    // Shared knobs (match the long-standing generic claudecode mapping).
    env.insert(
        "API_TIMEOUT_MS".to_string(),
        serde_json::Value::String("3000000".to_string()),
    );
    env.insert(
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
        serde_json::Value::String("1".to_string()),
    );

    // Both modes authenticate via ANTHROPIC_AUTH_TOKEN (Bearer), so always drop
    // any ANTHROPIC_API_KEY (x-api-key) the user may have left in settings.json —
    // a stale real key would otherwise conflict. We *remove* the key rather than
    // write an empty string (the old generic mapping could only set "", not
    // delete) so settings.json stays clean.
    env.remove("ANTHROPIC_API_KEY");

    if relay_mode {
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            serde_json::Value::String(anthropic_url.clone()),
        );
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            serde_json::Value::String(api_key.clone()),
        );
        let one_m = model_info.one_m_context.unwrap_or(false);
        for var in CLAUDECODE_MODEL_VARS {
            env.insert(
                var.to_string(),
                serde_json::Value::String(claudecode_env_model_id(&real_model_id, var, one_m)),
            );
        }
    } else {
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            serde_json::Value::String(proxy_base),
        );
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            serde_json::Value::String("echobird-local-proxy".to_string()),
        );
        for var in CLAUDECODE_MODEL_VARS {
            env.remove(var);
        }
    }

    if let Err(e) = write_json_file(&settings_path, &config) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Code settings: {}", e),
        };
    }

    // Relay side-channel — real upstream, read fresh by anthropic_proxy on
    // every /claudecode/v1/messages request and by read_claudecode for the UI.
    let relay_path = echobird_dir().join("claudecode.json");
    let relay = serde_json::json!({
        "baseUrl": anthropic_url,
        "apiKey": api_key,
        "actualModel": real_model_id,
        "modelName": model_info.name.as_deref().unwrap_or(real_model_id.as_str()),
        "relayMode": relay_mode,
    });
    if let Err(e) = write_json_file(&relay_path, &relay) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Code relay file: {}", e),
        };
    }

    ApplyResult {
        success: true,
        message:
            "Claude Code configured. Restart Claude Code (or /exit and reopen) for the change to take effect."
                .to_string(),
    }
}

pub(super) fn read_claudecode() -> Option<ModelInfo> {
    // Source of truth is the relay file (written in both modes), NOT
    // settings.json — mirrors read_claudedesktop. Surfaces the real upstream
    // model so the App Desktop card shows it after a rescan.
    let relay_path = echobird_dir().join("claudecode.json");
    let relay = read_json_file(&relay_path)?;

    let anthropic_url = relay
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let api_key = relay
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let model = relay
        .get("actualModel")
        .and_then(|v| v.as_str())
        // Legacy tolerance: relay files written by ≤v5.3.8 (which had a 1M
        // toggle) may carry a `[1m]` suffix on actualModel. Strip it so the
        // surfaced active-model id matches the user's stored modelId —
        // otherwise the App Desktop card's "currently applied" highlight
        // fails to match after a rescan. Nothing writes the suffix any more.
        .map(|s| s.strip_suffix("[1m]").unwrap_or(s))
        .filter(|s| !s.is_empty())
        .map(String::from);
    let name = relay
        .get("modelName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Some(ModelInfo {
        name,
        model,
        base_url: None,
        api_key,
        anthropic_url: Some(anthropic_url),
        protocol: Some("anthropic".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

/// Surgically remove the env keys we own from ~/.claude/settings.json and
/// drop the relay file. Unlike the generic restore (which deletes the whole
/// config file), this preserves allowedTools / hooks / anything else the user
/// keeps in settings.json — deleting it wholesale would wipe their Claude Code
/// setup, not just our model config.
pub(super) fn restore_claudecode_to_official() -> ApplyResult {
    const OUR_ENV_KEYS: [&str; 12] = [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_MODEL",
        // Kept for migration cleanup: older applies wrote this now-deprecated
        // var, so restore must still strip it from legacy settings.json even
        // though apply_claudecode no longer writes it.
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        // Pins subagents to the upstream model in relay mode (new env var,
        // replaces the small-fast tier's role for subagent selection).
        "CLAUDE_CODE_SUBAGENT_MODEL",
        "API_TIMEOUT_MS",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    ];

    if let Some(home) = dirs::home_dir() {
        let settings_path = home.join(".claude").join("settings.json");
        if settings_path.exists() {
            match read_json_file(&settings_path) {
                Some(mut config) => {
                    if let Some(env) = config.get_mut("env").and_then(|v| v.as_object_mut()) {
                        for key in OUR_ENV_KEYS {
                            env.remove(key);
                        }
                        let _ = write_json_file(&settings_path, &config);
                    }
                }
                None => {
                    // Can't parse settings.json → can't strip our env keys. Abort
                    // BEFORE deleting the relay file: in bridge mode ANTHROPIC_BASE_URL
                    // still points at the proxy, so dropping the relay would leave
                    // Claude Code 503-ing on every request while we claim success.
                    return ApplyResult {
                        success: false,
                        message:
                            "~/.claude/settings.json couldn't be parsed, so EchoBird's keys can't be cleanly removed. Restore aborted to avoid leaving Claude Code pointed at a stopped proxy — fix the file and try again."
                                .to_string(),
                    };
                }
            }
        }
    }

    let relay_path = echobird_dir().join("claudecode.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    ApplyResult {
        success: true,
        message: "Claude Code restored to official provider. Restart Claude Code for the change to take effect."
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claudecode_model_info(relay_mode: Option<bool>) -> ModelInfo {
        ModelInfo {
            name: Some("MiMo v2.5 Pro".to_string()),
            model: Some("mimo-v2.5-pro".to_string()),
            base_url: Some("https://api.example.com/v1".to_string()),
            api_key: Some("test-key".to_string()),
            anthropic_url: None,
            protocol: Some("anthropic".to_string()),
            display_model: None,
            relay_mode,
            responses_passthrough: None,
            web_search: None,
            one_m_context: None,
        }
    }

    #[test]
    fn claudecode_normalize_never_decorates_model_id() {
        // The model id must reach the upstream verbatim in every mode. Claude
        // decorations like the `[1m]` suffix are gone entirely: in bridge mode
        // Claude Code's own built-in claude-* ids already budget the full
        // window, and the rewritten upstream id must be exactly what the
        // third-party provider advertises.
        for relay_mode in [None, Some(false), Some(true)] {
            let info =
                normalize_model_info_for_tool("claudecode", claudecode_model_info(relay_mode));
            assert_eq!(info.model.as_deref(), Some("mimo-v2.5-pro"));
        }
    }

    #[test]
    fn claudecode_normalize_strips_trailing_v1_from_base_url() {
        // Relay mode appends `/v1/messages` client-side, so a `/v1` left on the
        // base URL would double into `/v1/v1/messages`.
        let info = normalize_model_info_for_tool("claudecode", claudecode_model_info(None));
        assert_eq!(info.base_url.as_deref(), Some("https://api.example.com"));
    }

    // API Router (relay) full write-in pins every model tier to the real
    // upstream model so Claude Code uses the third-party model end-to-end.
    // Two requirements locked here, per https://code.claude.com/docs/en/model-config:
    //   * CLAUDE_CODE_SUBAGENT_MODEL must be written — otherwise subagents
    //     fall back to claude-* ids and bypass the router (breaks "全量").
    //   * ANTHROPIC_SMALL_FAST_MODEL must NOT be written — Claude Code
    //     deprecated it in favor of ANTHROPIC_DEFAULT_HAIKU_MODEL (still pinned).
    #[test]
    fn claudecode_model_vars_pin_subagent_and_drop_small_fast() {
        assert!(
            CLAUDECODE_MODEL_VARS.contains(&"CLAUDE_CODE_SUBAGENT_MODEL"),
            "relay mode must pin CLAUDE_CODE_SUBAGENT_MODEL so subagents use the third-party model"
        );
        assert!(
            !CLAUDECODE_MODEL_VARS.contains(&"ANTHROPIC_SMALL_FAST_MODEL"),
            "ANTHROPIC_SMALL_FAST_MODEL is deprecated (favor ANTHROPIC_DEFAULT_HAIKU_MODEL, also pinned) — stop writing it"
        );
    }

    // ── claudecode_env_model_id: [1m] opt-in, 1M-tier-only ──
    // The 1M-context toggle is relay-only and Claude-Code-only. When opted in,
    // `[1m]` is appended to the 1M-capable tier (MODEL / SONNET / OPUS / FABLE) so CC
    // budgets the 1M window; HAIKU + SUBAGENT stay bare (no 1M concept). CC
    // strips the suffix before sending upstream, so the provider sees the bare id.
    #[test]
    fn claudecode_env_model_id_appends_1m_for_1m_tier_when_opt_in() {
        for &var in CLAUDECODE_MODEL_VARS_1M {
            assert_eq!(
                claudecode_env_model_id("mimo-v2.5-pro", var, true),
                "mimo-v2.5-pro[1m]",
                "opt-in 1M must decorate the 1M-capable tier var {}",
                var
            );
        }
    }

    #[test]
    fn claudecode_env_model_id_leaves_haiku_and_subagent_bare_even_when_opt_in() {
        for var in [
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ] {
            assert_eq!(
                claudecode_env_model_id("mimo-v2.5-pro", var, true),
                "mimo-v2.5-pro",
                "opt-in must NOT decorate the non-1M tier var {}",
                var
            );
        }
    }

    #[test]
    fn claudecode_env_model_id_bare_when_opt_out() {
        // Opt-out (or unset) → every var stays bare, regardless of tier.
        for var in CLAUDECODE_MODEL_VARS {
            assert_eq!(
                claudecode_env_model_id("mimo-v2.5-pro", var, false),
                "mimo-v2.5-pro",
                "opt-out must leave {} bare",
                var
            );
        }
    }
}
