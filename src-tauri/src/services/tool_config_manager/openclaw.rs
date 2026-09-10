//! Model configuration for openclaw.

use super::{
    echobird_dir, ensure_parent, extract_domain_name, read_json_file, write_json_file, ApplyResult,
    ModelInfo,
};

// ════════════════════════════════════════════════════════════════
//  OpenClaw: Direct write to ~/.openclaw/openclaw.json
//  v2026.3.13+: models.providers custom provider, mode: "merge"
//  No longer patches openclaw.mjs — writes native config directly.
// ════════════════════════════════════════════════════════════════

pub(super) fn apply_openclaw(model_info: &ModelInfo) -> ApplyResult {
    let home = dirs::home_dir().unwrap_or_default();
    let oc_dir = home.join(".openclaw");
    let oc_config_path = oc_dir.join("openclaw.json");

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

    let api_key = model_info.api_key.as_deref().unwrap_or("");
    if api_key.is_empty() {
        return ApplyResult {
            success: false,
            message: "API Key is empty, cannot apply config".to_string(),
        };
    }

    // Preserve gateway token from existing config (if any)
    let gateway = if oc_config_path.exists() {
        read_json_file(&oc_config_path).and_then(|c| c.get("gateway").cloned())
    } else {
        None
    };

    // Determine protocol and API type
    let protocol = model_info.protocol.as_deref().unwrap_or("openai");
    let is_anthropic = protocol == "anthropic"
        || model_id.to_lowercase().contains("claude")
        || model_info
            .base_url
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("anthropic");
    let api_type = if is_anthropic {
        "anthropic-messages"
    } else {
        "openai-completions"
    };

    // Extract provider tag from base URL
    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/');
    let provider_tag = extract_domain_name(base_url);
    let eb_provider = format!("eb_{}", provider_tag);

    // Build fresh openclaw.json — full overwrite, no merge
    let mut oc_config = serde_json::json!({
        "models": {
            "mode": "merge",
            "providers": {
                eb_provider.clone(): {
                    "baseUrl": base_url,
                    "apiKey": api_key,
                    "api": api_type,
                    "models": [{
                        "id": model_id,
                        "name": model_info.name.as_deref().unwrap_or(model_id),
                        "contextWindow": 128000,
                        "maxTokens": 8192,
                        "input": ["text"],
                        "reasoning": false,
                        "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
                    }]
                }
            }
        },
        "agents": {
            "defaults": {
                "model": {
                    "primary": format!("{}/{}", eb_provider, model_id)
                }
            }
        }
    });

    // Restore gateway token
    if let Some(gw) = gateway {
        oc_config["gateway"] = gw;
    }

    // Write fresh config
    ensure_parent(&oc_config_path);
    if let Err(e) = write_json_file(&oc_config_path, &oc_config) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write openclaw.json: {}", e),
        };
    }

    // Also write ~/.echobird/openclaw.json relay (used by Bridge/Channels)
    let relay_path = echobird_dir().join("openclaw.json");
    let relay = serde_json::json!({
        "apiKey": api_key,
        "modelId": model_id,
        "modelName": model_info.name.as_deref().unwrap_or(model_id),
        "baseUrl": base_url,
        "protocol": protocol,
    });
    let _ = write_json_file(&relay_path, &relay);

    log::info!(
        "[ToolConfigManager] OpenClaw config overwritten: {}/{} ({})",
        eb_provider,
        model_id,
        api_type
    );

    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for OpenClaw ({}/{}).",
            model_info.name.as_deref().unwrap_or(model_id),
            eb_provider,
            model_id
        ),
    }
}

pub(super) fn read_openclaw() -> Option<ModelInfo> {
    let oc_config_path = dirs::home_dir()?.join(".openclaw").join("openclaw.json");
    let config = read_json_file(&oc_config_path)?;

    // Read primary model: agents.defaults.model.primary = "eb_xxx/model-id"
    let primary = config
        .pointer("/agents/defaults/model/primary")
        .and_then(|v| v.as_str())?;

    // Parse "provider/model" format
    let (provider_name, model_id) = primary.split_once('/')?;

    // Only read eb_ providers (our custom ones)
    if !provider_name.starts_with("eb_") {
        return None;
    }

    // Get provider details from models.providers
    let provider = config.pointer(&format!("/models/providers/{}", provider_name))?;

    let base_url = provider
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let api_key = provider
        .get("apiKey")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let api_type = provider
        .get("api")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-completions");
    let protocol = if api_type.contains("anthropic") {
        "anthropic"
    } else {
        "openai"
    };

    // Get model name from models array
    let model_name = provider
        .get("models")
        .and_then(|m| m.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id))
        })
        .and_then(|m| m.get("name").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    Some(ModelInfo {
        name: model_name,
        model: Some(model_id.to_string()),
        base_url,
        api_key,
        anthropic_url: None,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}
