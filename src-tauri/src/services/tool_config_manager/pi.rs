//! Model configuration for pi.

use super::{read_json_file, write_json_file, ApplyResult, ModelInfo};
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  Pi (earendil-works/pi) — split config:
//   ~/.pi/agent/models.json    — provider definitions (custom OpenAI/Anthropic-compat)
//   ~/.pi/agent/settings.json  — defaultProvider + defaultModel
//  We register a single "echobird" provider in models.json and point
//  settings.json at it. Anthropic-protocol models switch the api type
//  to "anthropic-messages" and use anthropicUrl as the baseUrl.
//  Docs: https://pi.dev/docs/latest/models
// ════════════════════════════════════════════════════════════════

fn pi_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".pi")
        .join("agent")
}

pub(super) fn apply_pi(model_info: &ModelInfo) -> ApplyResult {
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

    // OpenAI-only — Pi's anthropic-messages API uses the @anthropic-ai/sdk
    // which appends /v1/messages to baseURL; model companies' Anthropic
    // endpoints have inconsistent path structures (some serve at <root>/v1/
    // messages, some don't), so the SDK's path-append can't be made reliable
    // across them. We don't expose Anthropic for Pi (same call as kimi) —
    // users can configure an anthropic provider manually in ~/.pi/agent/
    // models.json if they need it.
    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string();
    let api_type = "openai-completions";

    let provider_id = "echobird";

    // models.json — register/replace the echobird provider
    let models_path = pi_dir().join("models.json");
    let mut models_config = read_json_file(&models_path).unwrap_or(serde_json::json!({}));
    if !models_config.is_object() {
        models_config = serde_json::json!({});
    }
    if !models_config
        .get("providers")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        models_config["providers"] = serde_json::json!({});
    }
    models_config["providers"][provider_id] = serde_json::json!({
        "baseUrl": base_url,
        "api": api_type,
        "apiKey": model_info.api_key.as_deref().unwrap_or(""),
        "models": [{ "id": model_id }]
    });
    if let Err(e) = write_json_file(&models_path, &models_config) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }

    // settings.json — point defaultProvider/defaultModel at our provider
    let settings_path = pi_dir().join("settings.json");
    let mut settings = read_json_file(&settings_path).unwrap_or(serde_json::json!({}));
    if !settings.is_object() {
        settings = serde_json::json!({});
    }
    settings["defaultProvider"] = serde_json::Value::String(provider_id.to_string());
    settings["defaultModel"] = serde_json::Value::String(model_id.to_string());
    if let Err(e) = write_json_file(&settings_path, &settings) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }

    log::info!(
        "[ToolConfigManager] Pi configured: provider={}, model={}, api={}",
        provider_id,
        model_id,
        api_type
    );
    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for Pi ({}). Restart pi to apply.",
            model_info.name.as_deref().unwrap_or(model_id),
            api_type
        ),
    }
}

pub(super) fn read_pi() -> Option<ModelInfo> {
    let dir = pi_dir();
    let settings = read_json_file(&dir.join("settings.json"))?;
    let provider_id = settings.get("defaultProvider")?.as_str()?;
    let model_id = settings.get("defaultModel")?.as_str()?.to_string();
    if model_id.is_empty() {
        return None;
    }

    let models = read_json_file(&dir.join("models.json"))?;
    let prov = models.pointer(&format!("/providers/{}", provider_id))?;

    let api_type = prov
        .get("api")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-completions");
    let base_url = prov
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let api_key = prov
        .get("apiKey")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let (base, anthro) = if api_type == "anthropic-messages" {
        (None, base_url)
    } else {
        (base_url, None)
    };

    Some(ModelInfo {
        name: Some(model_id.clone()),
        model: Some(model_id),
        base_url: base,
        api_key,
        anthropic_url: anthro,
        protocol: Some(
            if api_type == "anthropic-messages" {
                "anthropic"
            } else {
                "openai"
            }
            .to_string(),
        ),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore_pi_to_official() -> ApplyResult {
    let dir = pi_dir();
    // Delete only our additions — the echobird provider entry in models.json,
    // and clear defaultProvider/defaultModel in settings.json. Don't nuke the
    // files (other providers and unrelated settings stay intact).
    let models_path = dir.join("models.json");
    if let Some(mut models) = read_json_file(&models_path) {
        if models
            .get("providers")
            .map(|v| v.is_object())
            .unwrap_or(false)
        {
            if let Some(obj) = models["providers"].as_object_mut() {
                obj.remove("echobird");
            }
            let _ = write_json_file(&models_path, &models);
        }
    }

    let settings_path = dir.join("settings.json");
    if let Some(mut settings) = read_json_file(&settings_path) {
        if let Some(obj) = settings.as_object_mut() {
            obj.remove("defaultProvider");
            obj.remove("defaultModel");
        }
        let _ = write_json_file(&settings_path, &settings);
    }

    ApplyResult {
        success: true,
        message: "Pi restored — echobird provider removed, defaults cleared. Pi will fall back to its built-in providers on next launch.".to_string(),
    }
}
