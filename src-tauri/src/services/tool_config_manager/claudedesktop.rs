//! Model configuration for claudedesktop.

use super::{echobird_dir, read_json_file, write_json_file, ApplyResult, ModelInfo};
use crate::services::codex_proxy::CODEX_PROXY_PORT;
use std::fs;
use std::path::{Path, PathBuf};

// ════════════════════════════════════════════════════════════════
//  Claude Desktop — 3P provider profile writer.
//
// Claude Desktop has an officially-supported "third-party profile"
// (3P) mechanism that's completely separate from Claude Code's
// ~/.claude/settings.json. Setting `deploymentMode = "3p"` in
// claude_desktop_config.json + writing a profile JSON to
// Claude-3p/configLibrary/ tells Desktop to route /v1/messages
// traffic to a custom inferenceGatewayBaseUrl instead of Anthropic.
//
// We only support providers that natively speak the Anthropic
// Messages API (i.e. `model.anthropicUrl` is set in
// modelDirectory.json — DeepSeek, GLM, Kimi, Qwen, MiniMax,
// Xiaomi, WorldRouter, Anthropic itself). Non-Anthropic providers
// would need a translation proxy, which we deliberately skip —
// the AppManager UI filters out incompatible models via the
// existing apiProtocol/anthropicUrl gate.
// ════════════════════════════════════════════════════════════════

const CLAUDE_DESKTOP_PROFILE_ID: &str = "7d8f4e2a-9c3b-4f1a-b0e5-1a2b3c4d5e6f";

const CLAUDE_DESKTOP_PROFILE_NAME: &str = "EchoBird";

struct ClaudeDesktopLayout {
    cfg_official: PathBuf,
    cfg_threep: PathBuf,
    lib_dir: PathBuf,
}

#[cfg(any(target_os = "macos", windows))]
fn resolve_claudedesktop_paths() -> Option<ClaudeDesktopLayout> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    let (official_dir, threep_dir) = {
        let app_support = home.join("Library").join("Application Support");
        (app_support.join("Claude"), app_support.join("Claude-3p"))
    };

    #[cfg(windows)]
    let (official_dir, threep_dir) = {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        (local.join("Claude"), local.join("Claude-3p"))
    };

    Some(ClaudeDesktopLayout {
        cfg_official: official_dir.join("claude_desktop_config.json"),
        cfg_threep: threep_dir.join("claude_desktop_config.json"),
        lib_dir: threep_dir.join("configLibrary"),
    })
}

#[cfg(not(any(target_os = "macos", windows)))]
fn resolve_claudedesktop_paths() -> Option<ClaudeDesktopLayout> {
    None
}

/// Flip the `deploymentMode` key on one of Claude Desktop's top-level
/// config files. Preserves every other key the user (or Desktop itself)
/// has stashed there — `mcpServers`, telemetry opt-outs, window size, etc.
fn set_claude_deployment_mode(path: &Path, mode: &str) -> Result<(), String> {
    let mut cfg = read_json_file(path).unwrap_or_else(|| serde_json::json!({}));
    if !cfg.is_object() {
        cfg = serde_json::json!({});
    }
    if let Some(obj) = cfg.as_object_mut() {
        obj.insert(
            "deploymentMode".to_string(),
            serde_json::Value::String(mode.to_string()),
        );
    }
    write_json_file(path, &cfg)
}

pub(super) fn apply_claudedesktop(model_info: &ModelInfo) -> ApplyResult {
    let paths = match resolve_claudedesktop_paths() {
        Some(p) => p,
        None => {
            return ApplyResult {
                success: false,
                message: "Claude Desktop is only supported on macOS and Windows.".to_string(),
            };
        }
    };

    // The frontend's AppManagerProvider.applyModel collapses the chosen
    // protocol's URL into `base_url` when sending to the backend (the
    // long-standing claudecode convention — see line `const apiUrl =
    // useAnthropicUrl ? model.anthropicUrl! : model.baseUrl`). For
    // claudedesktop the picked model's Anthropic endpoint will arrive
    // in `base_url` with `protocol == "anthropic"`. Accept either field
    // so we work today and stay compatible if the frontend ever sends
    // `anthropic_url` explicitly. The AppManager protocol filter is the
    // real gate that ensures the URL is actually Anthropic-shaped.
    let anthropic_url = model_info
        .anthropic_url
        .as_deref()
        .or(model_info.base_url.as_deref())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(|u| u.trim_end_matches('/').to_string());
    let anthropic_url = match anthropic_url {
        Some(u) => u,
        None => {
            return ApplyResult {
                success: false,
                message: "Base URL is empty. Pick a model first.".to_string(),
            };
        }
    };

    // Mirror the local-provider fallback in `apply_codex` (line 1153):
    // local LLM proxies (llama.cpp / vllm / sglang under our unified
    // proxy) don't require an API key, so an empty user-supplied key
    // is legitimate when the upstream is loopback. Substitute a
    // non-empty sentinel so anthropic_proxy still has something to
    // forward as Bearer / x-api-key (the local server ignores it).
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider =
        anthropic_url.contains("127.0.0.1") || anthropic_url.contains("localhost");
    let api_key = if raw_api_key.is_empty() {
        if is_local_provider {
            "local-no-auth".to_string()
        } else {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Claude Desktop config.".to_string(),
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
        .unwrap_or("");

    if let Err(e) = set_claude_deployment_mode(&paths.cfg_official, "3p") {
        return ApplyResult {
            success: false,
            message: format!("Failed to set Claude config to 3p mode: {}", e),
        };
    }
    if let Err(e) = set_claude_deployment_mode(&paths.cfg_threep, "3p") {
        return ApplyResult {
            success: false,
            message: format!("Failed to set Claude-3p config to 3p mode: {}", e),
        };
    }

    let _ = fs::create_dir_all(&paths.lib_dir);

    // Two routing modes, picked by `model_info.relay_mode`:
    //
    // • Bridge (default): Desktop's gateway hits our local Anthropic
    //   proxy on `127.0.0.1:CODEX_PROXY_PORT/v1/messages`. The proxy
    //   reads the real upstream URL, API key, and model id fresh from
    //   ~/.echobird/claudedesktop.json on every request, rewrites the
    //   Anthropic-only model id (Desktop hardcodes claude-sonnet-4-*
    //   etc.) to whatever the EchoBird user actually picked, and
    //   forwards. The `inferenceGatewayApiKey` is a non-empty sentinel
    //   — Desktop refuses an empty value but the proxy ignores what
    //   Desktop sends here, using the real key from the relay file.
    //
    // • Relay (relay_mode = true): Desktop's gateway hits the upstream
    //   directly. We write the real URL + real API key into the
    //   profile JSON; the proxy is bypassed for /v1/messages. Used
    //   for relay stations (cc-vibe.com etc.) that natively serve
    //   Anthropic Messages and do their own model-id mapping. Caveat:
    //   model-id rewrite is lost, so the upstream sees whatever id
    //   Desktop chose (claude-sonnet-4-*, …) — fine for stations that
    //   accept those, broken for raw Chat-only providers.
    let proxy_base = format!("http://127.0.0.1:{}", CODEX_PROXY_PORT);
    let relay_mode = model_info.relay_mode.unwrap_or(false);
    let (gateway_base_url, gateway_api_key) = if relay_mode {
        (anthropic_url.clone(), api_key.clone())
    } else {
        (proxy_base.clone(), "echobird-local-proxy".to_string())
    };

    let profile_path = paths
        .lib_dir
        .join(format!("{}.json", CLAUDE_DESKTOP_PROFILE_ID));
    // Profile body — Desktop's gateway reads these on launch. The profile's
    // display name is NOT a field of this object; it belongs in _meta.json
    // entries[].name (Desktop's profile picker reads it from there).
    //
    // `inferenceModels` populates Desktop's in-app model picker directly
    // and, crucially, BYPASSES Desktop's `/v1/models` discovery probe.
    // Without this field, Desktop 1.7.x falls back to GET /v1/models on
    // our gateway (which we don't implement) and surfaces a
    // "Gateway returned an error" banner on every fresh install. We
    // write a single entry whose `name` is the id Desktop sends on
    // `/v1/messages`. In bridge mode that's the canonical claude-* id
    // (messages_handler rewrites it to the real upstream model); in relay
    // mode Desktop hits the upstream directly with no rewrite, so `name`
    // must instead carry the real upstream id (computed below). `labelOverride` is
    // the upstream model id (`real_model_id`, source = model_info.model
    // with fallback to model_info.name) — NOT the user's editable card
    // display name, which can be anything ("deepseek你好" etc.) and
    // would surface garbage in Desktop's picker. cc-switch surfaces the
    // upstream id here too.
    // Bridge mode rewrites the id downstream, so the canonical claude-opus-5
    // is correct (and clears Desktop's Claude-name filter). Relay mode bypasses
    // the proxy — Desktop talks to the upstream directly — so the real upstream
    // id (e.g. "fable-5") must be sent as-is, or the station receives a model
    // name it never advertised. Fall back to the canonical id when no real id
    // is known, so we never emit an empty name.
    let base_model_id = if relay_mode && !real_model_id.is_empty() {
        real_model_id
    } else {
        "claude-opus-5"
    };
    // 1M context: Claude Desktop expresses the long-context variant via a
    // `supports1m: true` flag on the model entry — NOT a `[1m]` name suffix
    // (Desktop's profile schema rejects the suffix). This flag is exactly what
    // Desktop's "Offer 1M-context variant" UI toggle sets; the name stays the
    // plain id. Set in BOTH routing modes. In bridge mode the entry name is
    // the canonical claude-* id and every request passes our proxy, which
    // strips any `[1m]` the client attaches. In relay mode Desktop talks to
    // the third-party upstream directly with the real model id; but Desktop's
    // 1M variant is itself a secondary, user-selected pick inside Desktop, so
    // `<real-id>[1m]` only reaches the upstream when the user explicitly
    // chooses it there (mirroring Desktop's native behavior). We just
    // advertise support; whether to actually send `[1m]` is the user's call.
    //
    // We deliberately do NOT set `anthropicFamilyTier` / `isFamilyDefault`.
    // They were added to try to make the 1M variant the default, which turned
    // out to be an unfixable Desktop-side bug. Worse, hardcoding a tier
    // mislabels the model in relay mode: there the name is the real upstream id
    // (e.g. a Sonnet model), so tagging it "opus" makes Desktop's opus alias
    // resolve to a Sonnet model. The tier alias is optional — Desktop keeps the
    // model usable without it.
    let mut model_entry = serde_json::Map::new();
    model_entry.insert(
        "name".to_string(),
        serde_json::Value::String(base_model_id.to_string()),
    );
    model_entry.insert(
        "labelOverride".to_string(),
        serde_json::Value::String(real_model_id.to_string()),
    );
    model_entry.insert("supports1m".to_string(), serde_json::Value::Bool(true));
    let model_entry = serde_json::Value::Object(model_entry);
    let profile = serde_json::json!({
        "disableDeploymentModeChooser": true,
        "inferenceGatewayApiKey": gateway_api_key,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": gateway_base_url,
        "inferenceModels": [model_entry],
        "inferenceProvider": "gateway",
        // Claude Desktop 3P mode derives the built-in web_fetch egress policy
        // from this field. Without it, egress falls back to the gateway host
        // only (127.0.0.1), so web_fetch can't reach external URLs. ["*"]
        // lets web_fetch fetch any URL the user asks Claude to fetch.
        "coworkEgressAllowedHosts": ["*"],
    });
    if let Err(e) = write_json_file(&profile_path, &profile) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Desktop profile: {}", e),
        };
    }

    // Meta — Desktop reads `appliedId` to know which profile is active,
    // and `entries[]` to populate the in-app profile picker. Without the
    // entries array the picker won't render our profile as named.
    let meta_path = paths.lib_dir.join("_meta.json");
    let meta = serde_json::json!({
        "appliedId": CLAUDE_DESKTOP_PROFILE_ID,
        "entries": [
            {
                "id": CLAUDE_DESKTOP_PROFILE_ID,
                "name": CLAUDE_DESKTOP_PROFILE_NAME,
            }
        ],
    });
    let _ = write_json_file(&meta_path, &meta);

    // Relay — the real upstream config that anthropic_proxy reads on
    // every /v1/messages request. Updates here take effect on the next
    // request without restarting anything.
    // Relay file always reflects the real upstream config (even in Relay
    // mode where the proxy is bypassed for the network path), so `read_
    // claudedesktop` can round-trip the active model back to the UI
    // and so future debug tooling has a consistent source of truth.
    let relay_path = echobird_dir().join("claudedesktop.json");
    let relay = serde_json::json!({
        "baseUrl": anthropic_url,
        "apiKey": api_key,
        "actualModel": real_model_id,
        "modelName": model_info.name.as_deref().unwrap_or(real_model_id),
        "relayMode": relay_mode,
    });
    if let Err(e) = write_json_file(&relay_path, &relay) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Desktop relay file: {}", e),
        };
    }

    ApplyResult {
        success: true,
        message:
            "Claude Desktop configured. Please fully quit and reopen Claude Desktop for the change to take effect."
                .to_string(),
    }
}

pub(super) fn restore_claudedesktop_to_official() -> ApplyResult {
    let paths = match resolve_claudedesktop_paths() {
        Some(p) => p,
        None => {
            return ApplyResult {
                success: true,
                message: "Claude Desktop is not supported on this OS — nothing to restore."
                    .to_string(),
            };
        }
    };

    let _ = set_claude_deployment_mode(&paths.cfg_official, "1p");
    let _ = set_claude_deployment_mode(&paths.cfg_threep, "1p");

    let profile_path = paths
        .lib_dir
        .join(format!("{}.json", CLAUDE_DESKTOP_PROFILE_ID));
    if profile_path.exists() {
        let _ = fs::remove_file(&profile_path);
    }

    let meta_path = paths.lib_dir.join("_meta.json");
    if meta_path.exists() {
        let _ = fs::remove_file(&meta_path);
    }

    // Drop the relay file so anthropic_proxy stops accepting requests
    // for the previously-applied provider. (The proxy keeps running and
    // returns 503 on a missing relay — graceful no-op for any stale
    // request still in flight.)
    let relay_path = echobird_dir().join("claudedesktop.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    ApplyResult {
        success: true,
        message:
            "Claude Desktop restored to official provider. Please fully quit and reopen Claude Desktop."
                .to_string(),
    }
}

pub(super) fn read_claudedesktop() -> Option<ModelInfo> {
    // Source of truth is the relay file, NOT Claude Desktop's profile
    // JSON. The profile only holds the proxy URL (127.0.0.1:53682) and
    // a sentinel key; the real upstream URL / key / model id live in
    // ~/.echobird/claudedesktop.json so apply_claudedesktop can update
    // them without touching Desktop's config and triggering a restart.
    let relay_path = echobird_dir().join("claudedesktop.json");
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

    Some(ModelInfo {
        name: None,
        model: None,
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
