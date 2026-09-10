//! Model configuration for aider.

use super::{ensure_parent, ApplyResult, ModelInfo};
use std::fs;

// ════════════════════════════════════════════════════════════════
//  Type 4a: Aider �?~/.aider.conf.yml (simple YAML key: value)
// ════════════════════════════════════════════════════════════════

pub(super) fn apply_aider(model_info: &ModelInfo) -> ApplyResult {
    let config_path = dirs::home_dir().unwrap_or_default().join(".aider.conf.yml");
    let mut content = fs::read_to_string(&config_path).unwrap_or_default();

    let model = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    if !model.is_empty() {
        content = yaml_write(&content, "model", model);
    }

    let protocol = model_info.protocol.as_deref().unwrap_or("openai");
    if protocol == "anthropic" {
        if let Some(ref k) = model_info.api_key {
            content = yaml_write(&content, "anthropic-api-key", k);
        }
        content = yaml_remove(&content, "openai-api-key");
        content = yaml_remove(&content, "openai-api-base");
    } else {
        if let Some(ref k) = model_info.api_key {
            content = yaml_write(&content, "openai-api-key", k);
        }
        if let Some(ref u) = model_info.base_url {
            content = yaml_write(&content, "openai-api-base", u);
        }
        content = yaml_remove(&content, "anthropic-api-key");
    }

    ensure_parent(&config_path);
    match fs::write(&config_path, &content) {
        Ok(_) => ApplyResult {
            success: true,
            message: format!("Model \"{}\" applied to Aider ({}).", model, protocol),
        },
        Err(e) => ApplyResult {
            success: false,
            message: format!("Aider error: {}", e),
        },
    }
}

pub(super) fn read_aider() -> Option<ModelInfo> {
    let path = dirs::home_dir()?.join(".aider.conf.yml");
    let content = fs::read_to_string(&path).ok()?;
    let model = yaml_read(&content, "model");
    if model.is_empty() {
        return None;
    }
    let ok = yaml_read(&content, "openai-api-key");
    let ak = yaml_read(&content, "anthropic-api-key");
    let api_key = if !ok.is_empty() {
        Some(ok)
    } else if !ak.is_empty() {
        Some(ak)
    } else {
        None
    };
    let bu = yaml_read(&content, "openai-api-base");
    Some(ModelInfo {
        name: Some(model.clone()),
        model: Some(model),
        base_url: if bu.is_empty() { None } else { Some(bu) },
        api_key,
        anthropic_url: None,
        protocol: None,
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

// ════════════════════════════════════════════════════════════════
//  Simple YAML helpers (key: value format only)
// ════════════════════════════════════════════════════════════════

fn yaml_read(content: &str, key: &str) -> String {
    let prefix = format!("{}:", key);
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix(&prefix) {
            let v = rest.trim();
            if let Some(stripped) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                return stripped.to_string();
            }
            if let Some(stripped) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                return stripped.to_string();
            }
            return v.to_string();
        }
    }
    String::new()
}

fn yaml_write(content: &str, key: &str, value: &str) -> String {
    let prefix = format!("{}:", key);
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found = false;
    for line in lines.iter_mut() {
        let t = line.trim();
        if !t.starts_with('#') && t.starts_with(&prefix) {
            *line = format!("{}: {}", key, value);
            found = true;
            break;
        }
    }
    if !found {
        lines.push(format!("{}: {}", key, value));
    }
    lines.join("\n")
}

fn yaml_remove(content: &str, key: &str) -> String {
    let prefix = format!("{}:", key);
    content
        .lines()
        .filter(|l| l.trim().starts_with('#') || !l.trim().starts_with(&prefix))
        .collect::<Vec<_>>()
        .join("\n")
}
