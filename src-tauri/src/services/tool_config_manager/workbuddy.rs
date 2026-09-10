//! Model configuration for workbuddy.

use super::{extract_domain_name, read_json_file, write_json_file, ApplyResult, ModelInfo};

// ════════════════════════════════════════════════════════════════
//  WorkBuddy (Tencent CodeBuddy 办公版) → ~/.workbuddy/models.json
//  Shape: { models: [{ id, name, vendor, url, apiKey, maxInputTokens,
//  maxOutputTokens, supportsToolCall, supportsImages }], availableModels: [id] }
//
//  Notes:
//   • WorkBuddy uses its OWN ~/.workbuddy dir — NOT the ~/.codebuddy that the
//     CodeBuddy IDE uses. (Earlier docs that said .codebuddy were CodeBuddy's,
//     not WorkBuddy's.)
//   • `url` MUST be the full /chat/completions endpoint.
//   • File must be UTF-8 WITHOUT BOM — some desktop builds fail to parse a
//     BOM'd models.json. write_json_file writes raw UTF-8 (no BOM), so this
//     is satisfied for free.
//   • Single-entry overwrite = "switch model" semantics, matching every
//     other apply_* (one entry, never accumulates).
// ════════════════════════════════════════════════════════════════

pub(super) fn apply_workbuddy(model_info: &ModelInfo) -> ApplyResult {
    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".workbuddy")
        .join("models.json");

    let model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("echobird-model");
    let display_name = model_info
        .name
        .as_deref()
        .or(model_info.model.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or(model_id);

    let base_url = model_info.base_url.as_deref().unwrap_or("");
    let api_key = model_info.api_key.as_deref().unwrap_or("");
    if base_url.is_empty() || api_key.is_empty() {
        return ApplyResult {
            success: false,
            message: "WorkBuddy needs both a base URL and an API key — pick a model first."
                .to_string(),
        };
    }

    // WorkBuddy requires the full /chat/completions URL.
    let mut url = base_url.trim_end_matches('/').to_string();
    if !url.ends_with("/chat/completions") {
        url.push_str("/chat/completions");
    }

    let vendor = extract_domain_name(base_url);

    let config = serde_json::json!({
        "models": [{
            "id": model_id,
            "name": display_name,
            "vendor": vendor,
            "url": url,
            "apiKey": api_key,
            "maxInputTokens": 200000,
            "maxOutputTokens": 8192,
            "supportsToolCall": true,
            "supportsImages": true,
        }],
        "availableModels": [model_id],
    });

    match write_json_file(&config_path, &config) {
        Ok(_) => ApplyResult {
            success: true,
            message: format!(
                "Model \"{}\" applied to WorkBuddy. Fully quit and reopen WorkBuddy, then select it in the model picker.",
                display_name
            ),
        },
        Err(e) => ApplyResult {
            success: false,
            message: format!("WorkBuddy error: {}", e),
        },
    }
}

pub(super) fn read_workbuddy() -> Option<ModelInfo> {
    let path = dirs::home_dir()?.join(".workbuddy").join("models.json");
    let config = read_json_file(&path)?;
    let models = config.get("models")?.as_array()?;
    let m = models.first()?;
    let model = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if model.is_empty() {
        return None;
    }
    let base_url = m
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches("/chat/completions")
        .trim_end_matches('/')
        .to_string();
    Some(ModelInfo {
        name: m.get("name").and_then(|v| v.as_str()).map(String::from),
        model: Some(model.to_string()),
        base_url: if base_url.is_empty() {
            None
        } else {
            Some(base_url)
        },
        api_key: m.get("apiKey").and_then(|v| v.as_str()).map(String::from),
        anthropic_url: None,
        protocol: None,
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}
