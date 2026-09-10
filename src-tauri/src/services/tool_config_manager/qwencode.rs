//! Model configuration for qwencode.

use super::{extract_domain_name, read_json_file, write_json_file, ApplyResult, ModelInfo};

// ════════════════════════════════════════════════════════════════
//  Qwen Code: direct write to ~/.qwen/settings.json
//  Format: { modelProviders: { openai: [...] }, env: {...},
//           security: { auth: { selectedType } }, model: { name } }
// ════════════════════════════════════════════════════════════════

pub(super) fn apply_qwen_code(model_info: &ModelInfo) -> ApplyResult {
    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".qwen")
        .join("settings.json");

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
    let api_key = model_info.api_key.as_deref().unwrap_or("");
    let protocol = model_info.protocol.as_deref().unwrap_or("openai");

    // Qwen Code supports: openai, anthropic, gemini
    let selected_type = match protocol {
        "anthropic" => "anthropic",
        "gemini" => "gemini",
        _ => "openai",
    };

    // Env key name derived from domain to avoid collisions
    let domain = extract_domain_name(&base_url);
    let env_key = format!("ECHOBIRD_{}_API_KEY", domain.to_uppercase());
    let display_name = model_info.name.as_deref().unwrap_or(model_id);

    // Read existing config or start fresh
    let mut config = read_json_file(&config_path).unwrap_or(serde_json::json!({}));

    // Build the model provider entry
    let provider_entry = serde_json::json!({
        "id": model_id,
        "name": display_name,
        "baseUrl": base_url,
        "description": model_id,  // Use model ID instead of domain name
        "envKey": env_key
    });

    // Write modelProviders — replace the protocol array with single entry
    config["modelProviders"][selected_type] = serde_json::json!([provider_entry]);

    // Write env — set the API key
    config["env"][&env_key] = serde_json::Value::String(api_key.to_string());

    // Write security auth type
    config["security"]["auth"]["selectedType"] =
        serde_json::Value::String(selected_type.to_string());

    // Write active model
    config["model"]["name"] = serde_json::Value::String(model_id.to_string());

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] QwenCode config written: {} ({})",
                model_id,
                selected_type
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for Qwen Code. Restart qwen to apply.",
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

pub(super) fn read_qwen_code() -> Option<ModelInfo> {
    let config_path = dirs::home_dir()?.join(".qwen").join("settings.json");
    let config = read_json_file(&config_path)?;

    // Read active model name
    let model_name = config.pointer("/model/name")?.as_str()?.to_string();
    if model_name.is_empty() {
        return None;
    }

    // Read auth type to determine which provider array to look at
    let selected_type = config
        .pointer("/security/auth/selectedType")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");

    // Find the model entry in the provider array
    let providers = config
        .pointer(&format!("/modelProviders/{}", selected_type))
        .and_then(|v| v.as_array())?;

    let entry = providers
        .iter()
        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(&model_name))
        .or_else(|| providers.first())?;

    let base_url = entry
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Resolve API key from env object via envKey reference
    let api_key = entry
        .get("envKey")
        .and_then(|v| v.as_str())
        .and_then(|env_key| config.pointer(&format!("/env/{}", env_key)))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(ModelInfo {
        name: entry
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: Some(model_name),
        base_url,
        api_key,
        anthropic_url: None,
        protocol: Some(selected_type.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}
