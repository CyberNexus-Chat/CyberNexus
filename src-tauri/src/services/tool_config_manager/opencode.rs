//! Shared model configuration for OpenCode CLI and Desktop.

use super::{
    echobird_dir, read_json_file, read_jsonc_file, write_json_file, ApplyResult, ModelInfo,
};
use crate::services::tool_manager;
use std::fs;
use std::path::Path;

// ════════════════════════════════════════════════════════════════
//  Type 3b: OpenCode
//  ~/.config/opencode/opencode.json  {provider: {X: {npm, options, models}}}
// ════════════════════════════════════════════════════════════════

pub(super) fn apply_opencode(model_info: &ModelInfo) -> ApplyResult {
    // Write echobird relay JSON — the patched launcher reads this
    let config_path = echobird_dir().join("opencode.json");
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
    let provider_name = model_id; // Use model ID instead of domain name

    let config = serde_json::json!({
        "apiKey": model_info.api_key.as_deref().unwrap_or(""),
        "baseUrl": base_url,
        "modelId": model_id,
        "modelName": model_info.name.as_deref().unwrap_or(model_id),
        "providerName": provider_name,
    });

    if let Err(e) = write_opencode_native_config(model_info, model_id, &base_url, provider_name) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] OpenCode config written to {:?}",
                config_path
            );
            crate::services::tool_patcher::patch_opencode();
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for OpenCode. Use /models in TUI to select echobird/{}.",
                    model_info.name.as_deref().unwrap_or(model_id), model_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

fn write_opencode_native_config(
    model_info: &ModelInfo,
    model_id: &str,
    base_url: &str,
    provider_name: &str,
) -> Result<(), String> {
    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("opencode")
        .join("opencode.jsonc");

    let mut config = read_jsonc_file(&config_path)
        .or_else(|| read_json_file(&config_path.with_extension("json")))
        .unwrap_or(serde_json::json!({}));

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
    config["provider"][provider_id] = serde_json::json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": provider_name,
        "options": {
            "baseURL": base_url,
            "apiKey": model_info.api_key.as_deref().unwrap_or("")
        },
        "models": {
            model_id: {
                "name": model_info.name.as_deref().unwrap_or(model_id)
            }
        }
    });
    config["model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));
    config["small_model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));

    write_json_file(&config_path, &config)
}

pub(super) fn read_opencode() -> Option<ModelInfo> {
    let native_path = dirs::home_dir()?
        .join(".config")
        .join("opencode")
        .join("opencode.jsonc");
    if let Some(info) = read_opencode_native_config(&native_path)
        .or_else(|| read_opencode_native_config(&native_path.with_extension("json")))
    {
        return Some(info);
    }

    // Read from echobird relay JSON
    let config_path = echobird_dir().join("opencode.json");
    let config = read_json_file(&config_path)?;
    let model_id = config.get("modelId")?.as_str()?.to_string();
    if model_id.is_empty() {
        return None;
    }

    Some(ModelInfo {
        name: config
            .get("modelName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: Some(model_id),
        base_url: config
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        api_key: config
            .get("apiKey")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        anthropic_url: None,
        protocol: None,
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn read_opencode_native_config(path: &Path) -> Option<ModelInfo> {
    let config = if path.extension().and_then(|e| e.to_str()) == Some("jsonc") {
        read_jsonc_file(path)?
    } else {
        read_json_file(path)?
    };
    let selected = config.get("model")?.as_str()?;
    let (provider_id, model_id) = selected.split_once('/')?;
    let provider = config.pointer(&format!("/provider/{}", provider_id))?;

    Some(ModelInfo {
        name: provider
            .pointer(&format!("/models/{}/name", model_id))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: Some(model_id.to_string()),
        base_url: provider
            .pointer("/options/baseURL")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        api_key: provider
            .pointer("/options/apiKey")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        anthropic_url: None,
        protocol: Some("openai".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore_opencode_to_official() -> ApplyResult {
    let relay_path = echobird_dir().join("opencode.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    let native_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("opencode")
        .join("opencode.jsonc");

    if !native_path.exists() {
        return ApplyResult {
            success: true,
            message: "OpenCode already at defaults - no config file to update.".to_string(),
        };
    }

    let mut updated_any = false;
    for path in [&native_path, &native_path.with_extension("json")] {
        if !path.exists() {
            continue;
        }

        let mut config = match read_jsonc_file(path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!("Failed to parse OpenCode config: {}", path.display()),
                }
            }
        };

        if let Some(provider) = config.get_mut("provider").and_then(|v| v.as_object_mut()) {
            provider.remove("echobird");
        }
        if config
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("echobird/"))
            .unwrap_or(false)
        {
            tool_manager::delete_nested_value(&mut config, "model");
        }
        if config
            .get("small_model")
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("echobird/"))
            .unwrap_or(false)
        {
            tool_manager::delete_nested_value(&mut config, "small_model");
        }

        if let Err(e) = write_json_file(path, &config) {
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
            message: "OpenCode restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "OpenCode already at defaults - no config file to update.".to_string(),
        }
    }
}
