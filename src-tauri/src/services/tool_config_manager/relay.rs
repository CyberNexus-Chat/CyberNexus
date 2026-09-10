//! EchoBird relay configuration for custom tools without a dedicated adapter.

use super::{echobird_dir, read_json_file, write_json_file, ApplyResult, ModelInfo};

// ════════════════════════════════════════════════════════════════
//  Type 2: Echobird relay JSON (OpenClaw + custom plug-and-play tools)
//  Write to ~/.echobird/{tool_id}.json
// ════════════════════════════════════════════════════════════════

pub(super) fn apply_echobird_relay(
    tool_id: &str,
    model_info: &ModelInfo,
    include_provider: bool,
) -> ApplyResult {
    let config_path = echobird_dir().join(format!("{}.json", tool_id));
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

    let mut config = serde_json::json!({
        "apiKey": model_info.api_key.as_deref().unwrap_or(""),
        "modelId": model_id,
        "modelName": model_info.name.as_deref().unwrap_or(model_id),
    });

    if let Some(ref base_url) = model_info.base_url {
        config["baseUrl"] = serde_json::Value::String(base_url.clone());
    }
    if include_provider {
        config["provider"] = serde_json::Value::String("openai".to_string());
    }
    if tool_id == "openclaw" {
        config["protocol"] = serde_json::Value::String(
            model_info
                .protocol
                .as_deref()
                .unwrap_or("openai")
                .to_string(),
        );
    }

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] {} config written to {:?}",
                tool_id,
                config_path
            );
            crate::services::tool_patcher::patch_tool(tool_id);
            let tool_display = match tool_id {
                "openclaw" => "OpenClaw",
                _ => tool_id,
            };
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for {}. Restart to apply.",
                    model_info.name.as_deref().unwrap_or(model_id),
                    tool_display
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

pub(super) fn read_echobird_relay(tool_id: &str) -> Option<ModelInfo> {
    let config_path = echobird_dir().join(format!("{}.json", tool_id));
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
        protocol: config
            .get("protocol")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}
