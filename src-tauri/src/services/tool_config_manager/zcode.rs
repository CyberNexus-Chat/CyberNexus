//! Model configuration for zcode.

use super::{model_input_modalities_for, read_jsonc_file, write_json_file, ApplyResult, ModelInfo};
use crate::services::tool_manager;
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  ZCode — Z.AI's desktop OpenCode fork. ~/.zcode/v2/config.json.
//  OpenCode config schema, but the provider uses a `kind` discriminator
//  ("openai-compatible" | "anthropic") instead of OpenCode's `npm`, and
//  it supports BOTH protocols. The native config write is the whole
//  mechanism — desktop app, no launcher patch, no ~/.echobird relay.
//  Default model is the OpenCode-standard top-level `model` selector.
// ════════════════════════════════════════════════════════════════

fn zcode_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".zcode")
        .join("v2")
        .join("config.json")
}

pub(super) fn apply_zcode(model_info: &ModelInfo) -> ApplyResult {
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

    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string();

    // Local llama-server needs no real key; mirror the dummy used elsewhere.
    let is_local = base_url.contains("127.0.0.1") || base_url.contains("localhost");
    let api_key = match model_info.api_key.as_deref() {
        Some(k) if !k.is_empty() => k.to_string(),
        _ if is_local => "local-no-auth".to_string(),
        _ => {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply config".to_string(),
            }
        }
    };

    // Protocol → provider `kind`. The frontend already collapsed the chosen
    // protocol's URL into base_url, so base_url is correct for either kind.
    let kind = if model_info.protocol.as_deref() == Some("anthropic") {
        "anthropic"
    } else {
        "openai-compatible"
    };

    let config_path = zcode_config_path();
    let mut config = read_jsonc_file(&config_path).unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://opencode.ai/config.json");
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
    // Resolve the real input modalities for the selected model so ZCode's
    // `modalities.input` reflects image/video support when the model has it,
    // instead of declaring every model text-only.
    let input_modalities = model_input_modalities_for(model_id);
    config["provider"][provider_id] = serde_json::json!({
        "name": display_name,
        "kind": kind,
        "options": {
            "apiKey": api_key,
            "baseURL": base_url,
            "apiKeyRequired": true
        },
        "source": "custom",
        "models": {
            model_id: {
                "name": display_name,
                "modalities": { "input": input_modalities, "output": ["text"] }
            }
        }
    });
    // OpenCode-standard active-model selectors (UI stores its pick elsewhere;
    // these set the configured default that ZCode reads on launch).
    config["model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));
    config["small_model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] ZCode config written to {:?}",
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for ZCode. Restart ZCode to apply.",
                    display_name
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

pub(super) fn read_zcode() -> Option<ModelInfo> {
    let config = read_jsonc_file(&zcode_config_path())?;
    let selected = config.get("model")?.as_str()?;
    let (provider_id, model_id) = selected.split_once('/')?;
    let provider = config.pointer(&format!("/provider/{}", provider_id))?;
    let kind = provider
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-compatible");
    let protocol = if kind == "anthropic" {
        "anthropic"
    } else {
        "openai"
    };

    Some(ModelInfo {
        name: provider
            .pointer(&format!("/models/{}/name", model_id))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(model_id.to_string())),
        model: Some(model_id.to_string()),
        base_url: provider
            .pointer("/options/baseURL")
            .and_then(|v| v.as_str())
            .map(String::from),
        api_key: provider
            .pointer("/options/apiKey")
            .and_then(|v| v.as_str())
            .map(String::from),
        anthropic_url: None,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore_zcode_to_official() -> ApplyResult {
    let path = zcode_config_path();
    if !path.exists() {
        return ApplyResult {
            success: true,
            message: "ZCode already at defaults — no config file to update.".to_string(),
        };
    }
    let mut config = match read_jsonc_file(&path) {
        Some(c) => c,
        None => {
            return ApplyResult {
                success: false,
                message: format!("Failed to parse ZCode config: {}", path.display()),
            }
        }
    };
    if let Some(provider) = config.get_mut("provider").and_then(|v| v.as_object_mut()) {
        provider.remove("echobird");
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
    match write_json_file(&path, &config) {
        Ok(_) => ApplyResult {
            success: true,
            message: "ZCode restored — Echobird provider removed.".to_string(),
        },
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}
