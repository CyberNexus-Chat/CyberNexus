//! Config-file field mappings for tools without a dedicated adapter.

use super::{read_json_file, write_json_file, ApplyResult, ModelInfo};
use crate::services::tool_manager;

// ─── Known ModelInfo fields ───

const KNOWN_MODEL_FIELDS: &[&str] = &["id", "name", "baseUrl", "apiKey", "model", "protocol"];

fn get_model_field(model_info: &ModelInfo, field_name: &str) -> Option<String> {
    match field_name {
        "model" => model_info.model.clone(),
        "name" => model_info.name.clone(),
        "baseUrl" | "base_url" => model_info.base_url.clone(),
        "apiKey" | "api_key" => model_info.api_key.clone(),
        "protocol" => model_info.protocol.clone(),
        "anthropicUrl" | "anthropic_url" => model_info.anthropic_url.clone(),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════
//  Type 1: Generic JSON mapping (ClaudeCode, etc.)
// ════════════════════════════════════════════════════════════════

pub(super) async fn apply_generic_json(tool_id: &str, model_info: &ModelInfo) -> ApplyResult {
    let (def, config_path) = match tool_manager::get_tool_config_mapping(tool_id) {
        Some(pair) => pair,
        None => {
            return ApplyResult {
                success: false,
                message: format!("Unknown tool: {}", tool_id),
            };
        }
    };

    let cm = &def.config_mapping;

    if cm.format != "json" {
        return ApplyResult {
            success: false,
            message: format!(
                "Config format '{}' not supported for generic apply",
                cm.format
            ),
        };
    }

    let write_map = match &cm.write {
        Some(w) => w.clone(),
        None => {
            return ApplyResult {
                success: false,
                message: format!("Tool '{}' has no write mapping defined", tool_id),
            };
        }
    };

    let mut config = read_json_file(&config_path).unwrap_or(serde_json::json!({}));

    for (config_json_path, model_field) in &write_map {
        let value = get_model_field(model_info, model_field);
        if let Some(val) = value {
            tool_manager::set_nested_value(
                &mut config,
                config_json_path,
                serde_json::Value::String(val),
            );
        } else if model_field.is_empty() {
            tool_manager::set_nested_value(
                &mut config,
                config_json_path,
                serde_json::Value::String(String::new()),
            );
        } else if !KNOWN_MODEL_FIELDS.contains(&model_field.as_str()) {
            tool_manager::set_nested_value(
                &mut config,
                config_json_path,
                serde_json::Value::String(model_field.clone()),
            );
        }
    }

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!("[ToolConfigManager] Config written to {:?}", config_path);
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" applied to {} successfully.",
                    model_info.model.as_deref().unwrap_or(""),
                    tool_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

pub(super) fn read_generic_json(tool_id: &str) -> Option<ModelInfo> {
    let (def, config_path) = tool_manager::get_tool_config_mapping(tool_id)?;
    let cm = &def.config_mapping;
    if cm.format != "json" {
        return None;
    }
    let read_map = cm.read.as_ref()?;
    let config = read_json_file(&config_path)?;

    let read_field = |paths: &Option<Vec<String>>| -> Option<String> {
        for p in paths.as_ref()? {
            if let Some(val) = tool_manager::get_nested_value(&config, p) {
                if let Some(s) = val.as_str() {
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        None
    };

    let model = read_field(&read_map.model);
    model.as_ref()?;

    Some(ModelInfo {
        name: None,
        model,
        base_url: read_field(&read_map.base_url),
        api_key: read_field(&read_map.api_key),
        anthropic_url: None,
        protocol: None,
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}
