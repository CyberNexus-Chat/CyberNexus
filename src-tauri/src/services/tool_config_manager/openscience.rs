//! Model configuration for openscience.

use super::{read_jsonc_file, write_json_file, ApplyResult, ModelInfo};
use crate::services::tool_manager;
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  Type 3d: OpenScience - open-source Claude Science alternative.
//  ~/.config/openscience/openscience.json(c)  {provider: {X: {npm, options, models}}}
//  Same models.dev provider schema as OpenCode; model-agnostic (Anthropic +
//  OpenAI both native). Single binary (npm/curl install) - no launcher
//  patch, no relay file; the native config write is the whole mechanism.
//  `openscience_config_path()` picks the highest-precedence file the install
//  already owns (`.jsonc` wins by merge order), so a user who has touched any
//  global setting gets edits applied to the file that actually shadows the
//  rest — never a silent no-op against an out-ranked file.
// ════════════════════════════════════════════════════════════════

// OpenScience's GLOBAL config dir is ~/.config/openscience on every platform:
// it resolves `xdgConfig` from `xdg-basedir`, which has NO Windows APPDATA
// fallback (it joins os.homedir()/.config everywhere), so this path is correct
// on Windows / macOS / Linux alike.
fn openscience_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("openscience")
}

/// OpenScience merges its GLOBAL configs in order `config.json` →
/// `openscience.json` → `openscience.jsonc`, so `openscience.jsonc` has the
/// HIGHEST precedence (loaded last). Its own UI (`globalConfigFile`) defaults
/// to writing `.jsonc`, so a user who has touched any global setting already
/// owns a `.jsonc` that would shadow a plain `.json` write. Target the
/// `.jsonc` when it exists so our keys always take effect; default a fresh
/// install to `openscience.json` (the documented canonical name). Mirrors
/// `mimocode_write_path`. NB: writing a `.jsonc` strips the user's comments
/// (same tradeoff as mimicode) — accepted, because an invisible write is worse.
fn openscience_config_path() -> PathBuf {
    let jsonc = openscience_config_dir().join("openscience.jsonc");
    if jsonc.exists() {
        jsonc
    } else {
        openscience_config_dir().join("openscience.json")
    }
}

pub(super) fn apply_openscience(model_info: &ModelInfo) -> ApplyResult {
    let model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    if model_id.is_empty() {
        return ApplyResult {
            success: false,
            message: "Model ID is empty, cannot apply config".to_string(),
        };
    }

    // OpenScience speaks both protocols natively. The chosen protocol picks
    // the AI SDK npm package: Anthropic -> @ai-sdk/anthropic (endpoint from
    // anthropic_url), OpenAI-compatible -> @ai-sdk/openai-compatible (base_url).
    // `endpoint` is always set after this block — the anthropic arm early-
    // returns on an empty URL, the openai arm defaults to api.openai.com — so
    // no Option wrapping is needed downstream.
    let is_anthropic = model_info.protocol.as_deref() == Some("anthropic");
    let endpoint = if is_anthropic {
        match model_info
            .anthropic_url
            .as_deref()
            .or(model_info.base_url.as_deref())
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(|u| u.trim_end_matches('/').to_string())
        {
            Some(u) => u,
            None => {
                return ApplyResult {
                    success: false,
                    message: "Base URL is empty. Pick a model first.".to_string(),
                };
            }
        }
    } else {
        model_info
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/')
            .to_string()
    };
    let npm = if is_anthropic {
        "@ai-sdk/anthropic"
    } else {
        "@ai-sdk/openai-compatible"
    };

    // Local llama-server / vllm proxies need no real key; mirror the dummy
    // used by apply_codex/apply_claudedesktop so an empty key on a loopback
    // endpoint is legitimate instead of a hard failure.
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider = endpoint.contains("127.0.0.1") || endpoint.contains("localhost");
    let api_key = if !raw_api_key.is_empty() {
        raw_api_key.to_string()
    } else if is_local_provider {
        "local-no-auth".to_string()
    } else {
        return ApplyResult {
            success: false,
            message: "API Key is empty, cannot apply OpenScience config.".to_string(),
        };
    };

    let config_path = openscience_config_path();
    let mut config = read_jsonc_file(&config_path).unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://syntheticsciences.ai/config.json");
    }
    if !config
        .get("provider")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        config["provider"] = serde_json::json!({});
    }

    let provider_id = "echobird";
    let display_name = model_info.name.as_deref().unwrap_or(model_id);
    // Provider `name` is the PROVIDER's human label in OpenScience's workspace
    // (provider.ts resolves `provider.name ?? providerID`); the model's own
    // name belongs only in models.<id>.name. Pin it to "EchoBird" so it isn't
    // relabeled to whatever model was last applied.
    config["provider"][provider_id] = serde_json::json!({
        "npm": npm,
        "name": "EchoBird",
        "options": {
            "apiKey": api_key,
            "baseURL": endpoint
        },
        "models": {
            model_id: {
                "name": display_name
            }
        }
    });
    // OpenScience-standard active-model selectors (model + small_model for
    // title generation etc.). NB a running `openscience serve` memoizes config
    // in State.create keyed by Instance.directory and only re-reads on restart
    // / dispose, so the new model takes effect the next time the server starts
    // — not on an already-running one.
    config["model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));
    config["small_model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] OpenScience config written to {:?}",
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for OpenScience (echobird/{}) — start or restart `openscience serve` for the workspace to pick it up.",
                    display_name, model_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

pub(super) fn read_openscience() -> Option<ModelInfo> {
    let config = read_jsonc_file(&openscience_config_path())?;
    let selected = config.get("model")?.as_str()?;
    let (provider_id, model_id) = selected.split_once('/')?;
    if provider_id != "echobird" {
        // Not our provider - show as unconfigured so the user can apply.
        return None;
    }
    let provider = config.pointer("/provider/echobird")?;
    let npm = provider
        .get("npm")
        .and_then(|v| v.as_str())
        .unwrap_or("@ai-sdk/openai-compatible");
    let protocol = if npm == "@ai-sdk/anthropic" {
        "anthropic"
    } else {
        "openai"
    };
    let options = provider.get("options");
    let base_url = options
        .and_then(|o| o.get("baseURL"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let api_key = options
        .and_then(|o| o.get("apiKey"))
        .and_then(|v| v.as_str())
        .map(String::from);
    // Echo back the endpoint via anthropic_url when the provider is
    // Anthropic-shaped so the protocol toggle stays in sync on reload. (Only
    // the anthropic path pays one clone — unavoidable since both fields hold
    // the same value; the common openai path moves base_url with no clone.)
    let anthropic_url = if protocol == "anthropic" {
        base_url.clone()
    } else {
        None
    };

    Some(ModelInfo {
        // Use direct .get() lookups instead of a JSON pointer for the model
        // name: model_id can contain '/' (e.g. "openai/gpt-4o-mini"), and a
        // pointer like "/models/openai/gpt-4o-mini/name" would traverse into a
        // nested path instead of the single "openai/gpt-4o-mini" key.
        name: provider
            .get("models")
            .and_then(|m| m.get(model_id))
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(model_id.to_string())),
        model: Some(model_id.to_string()),
        base_url,
        api_key,
        anthropic_url,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore_openscience_to_official() -> ApplyResult {
    // Loop BOTH precedence files — OpenScience deep-merges config.json →
    // openscience.json → openscience.jsonc, so a stale `echobird` block left
    // in a lower-precedence file re-emerges after we clean the top one. Mirrors
    // restore_mimocode_to_official / restore_opencode_to_official.
    let dir = openscience_config_dir();
    let mut updated_any = false;

    for path in [dir.join("openscience.jsonc"), dir.join("openscience.json")] {
        if !path.exists() {
            continue;
        }
        let mut config = match read_jsonc_file(&path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!("Failed to parse OpenScience config: {}", path.display()),
                };
            }
        };
        if let Some(provider) = config.get_mut("provider").and_then(|v| v.as_object_mut()) {
            provider.remove("echobird");
            // Prune the now-empty `provider` object: OpenScience's
            // defaultModel() gates every provider through
            // `!cfg.provider || Object.keys(cfg.provider).includes(id)`, so an
            // empty `provider: {}` is truthy yet matches no id → it rejects
            // ALL providers (env keys included) and throws NO_PROVIDER_HINT,
            // bricking the workspace. Drop the key entirely when nothing's left.
            if provider.is_empty() {
                if let Some(obj) = config.as_object_mut() {
                    obj.remove("provider");
                }
            }
        }
        for key in ["model", "small_model"] {
            if config
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.starts_with("echobird/"))
                .unwrap_or(false)
            {
                tool_manager::delete_nested_value(&mut config, key);
            }
        }
        if let Err(e) = write_json_file(&path, &config) {
            return ApplyResult {
                success: false,
                message: e,
            };
        }
        updated_any = true;
    }

    if updated_any {
        ApplyResult {
            success: true,
            message: "OpenScience restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "OpenScience already at defaults - no config file to update.".to_string(),
        }
    }
}
