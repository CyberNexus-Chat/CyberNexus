//! Model configuration for mimocode.

use super::opencode::read_opencode_native_config;
use super::{echobird_dir, read_jsonc_file, write_json_file, ApplyResult, ModelInfo};
use crate::services::tool_manager;
use std::fs;
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  Type 3c: MiMo Code — Xiaomi's fork of OpenCode (binary: `mimo`)
//  ~/.config/mimocode/mimocode.json(c)  {provider: {X: {npm, options, models}}}
//  Same provider schema as OpenCode; no launcher patch and no relay
//  file — the curl install is a standalone binary, so the native
//  config write is the whole mechanism.
// ════════════════════════════════════════════════════════════════

fn mimocode_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("mimocode")
}

/// MiMo Code merges global configs in order config.json → mimocode.json →
/// mimocode.jsonc (later wins). Target the highest-priority file that
/// exists so our keys always take effect; default new installs to the
/// documented canonical name, mimocode.json.
fn mimocode_write_path() -> PathBuf {
    let jsonc = mimocode_config_dir().join("mimocode.jsonc");
    if jsonc.exists() {
        jsonc
    } else {
        mimocode_config_dir().join("mimocode.json")
    }
}

pub(super) fn apply_mimocode(model_info: &ModelInfo) -> ApplyResult {
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

    let config_path = mimocode_write_path();
    let mut config = read_jsonc_file(&config_path).unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://mimo.xiaomi.com/config.json");
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
        "name": model_id,
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

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] MiMo Code config written to {:?}",
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for MiMo Code. Restart `mimo` or use /models to select {}/{}.",
                    model_info.name.as_deref().unwrap_or(model_id),
                    provider_id,
                    model_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

pub(super) fn read_mimocode() -> Option<ModelInfo> {
    // Same provider schema as OpenCode — reuse its native-config reader.
    let dir = mimocode_config_dir();
    read_opencode_native_config(&dir.join("mimocode.jsonc"))
        .or_else(|| read_opencode_native_config(&dir.join("mimocode.json")))
}

/// Model id for the launcher's `--model echobird/<id>` injection. MiMo Code
/// keeps no ~/.echobird relay file (the native config is the single source
/// of truth), so the launcher reads the selection from here instead. Some
/// only while the native config currently selects our provider — after a
/// restore or a hand-edit to another provider, no flag is injected and
/// MiMo Code's own config/model resolution applies.
pub fn mimocode_echobird_model() -> Option<String> {
    let dir = mimocode_config_dir();
    let config = read_jsonc_file(&dir.join("mimocode.jsonc"))
        .or_else(|| read_jsonc_file(&dir.join("mimocode.json")))?;
    let selected = config.get("model")?.as_str()?;
    selected.strip_prefix("echobird/").map(|s| s.to_string())
}

pub(super) fn restore_mimocode_to_official() -> ApplyResult {
    // Builds prior to the dedicated mimocode arm fell through to the
    // relay side-channel; clean that file up so it can't linger.
    let relay_path = echobird_dir().join("mimocode.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    let dir = mimocode_config_dir();
    let mut updated_any = false;

    for path in [dir.join("mimocode.jsonc"), dir.join("mimocode.json")] {
        if !path.exists() {
            continue;
        }

        let mut config = match read_jsonc_file(&path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!("Failed to parse MiMo Code config: {}", path.display()),
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
            message: "MiMo Code restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "MiMo Code already at defaults - no config file to update.".to_string(),
        }
    }
}
