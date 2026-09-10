//! Model configuration for vibe_trading.

use super::{ensure_parent, ApplyResult, ModelInfo};
use std::fs;

/// Upsert a `KEY=value` line into a dotenv body. Replaces the first
/// uncommented line whose key matches; otherwise appends. Commented lines
/// and every other key (the user's data-source tokens, temperature /
/// timeout knobs) are left untouched.
fn env_upsert(content: &str, key: &str, value: &str) -> String {
    let mut replaced = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            if !replaced {
                let trimmed = line.trim_start();
                let is_match = !trimmed.starts_with('#')
                    && trimmed
                        .split_once('=')
                        .map(|(k, _)| k.trim() == key)
                        .unwrap_or(false);
                if is_match {
                    replaced = true;
                    return format!("{key}={value}");
                }
            }
            line.to_string()
        })
        .collect();
    if !replaced {
        lines.push(format!("{key}={value}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Vibe-Trading (HKUDS) — dotenv at `~/.vibe-trading/.env`. The agent
/// resolves its LLM through LangChain by reading the env vars named for
/// `LANGCHAIN_PROVIDER`. Every endpoint EchoBird points it at is
/// OpenAI-compatible, so pin the provider to `openai` and feed it the
/// custom base URL / key / model. Single EchoBird-owned model switch;
/// the user's data-source tokens and other knobs survive the per-key upsert.
pub(super) fn apply_vibe_trading(model_info: &ModelInfo) -> ApplyResult {
    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".vibe-trading")
        .join(".env");
    let mut content = fs::read_to_string(&config_path).unwrap_or_default();

    let model = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    let base_url = model_info.base_url.as_deref().unwrap_or("");
    let api_key = model_info.api_key.as_deref().unwrap_or("");

    content = env_upsert(&content, "LANGCHAIN_PROVIDER", "openai");
    if !model.is_empty() {
        content = env_upsert(&content, "LANGCHAIN_MODEL_NAME", model);
    }
    if !base_url.is_empty() {
        content = env_upsert(&content, "OPENAI_BASE_URL", base_url);
    }
    if !api_key.is_empty() {
        content = env_upsert(&content, "OPENAI_API_KEY", api_key);
    }

    ensure_parent(&config_path);
    match fs::write(&config_path, &content) {
        Ok(_) => ApplyResult {
            success: true,
            message: format!("Model \"{}\" applied to Vibe-Trading.", model),
        },
        Err(e) => ApplyResult {
            success: false,
            message: format!("Vibe-Trading error: {}", e),
        },
    }
}

/// Read the active model back from `~/.vibe-trading/.env`. EchoBird always
/// writes the OpenAI-compatible block, so the model lives in
/// `LANGCHAIN_MODEL_NAME` with `OPENAI_BASE_URL` / `OPENAI_API_KEY`.
pub(super) fn read_vibe_trading() -> Option<ModelInfo> {
    let path = dirs::home_dir()?.join(".vibe-trading").join(".env");
    let content = fs::read_to_string(&path).ok()?;
    let model = env_read(&content, "LANGCHAIN_MODEL_NAME");
    if model.is_empty() {
        return None;
    }
    let base_url = env_read(&content, "OPENAI_BASE_URL");
    let api_key = env_read(&content, "OPENAI_API_KEY");
    Some(ModelInfo {
        name: Some(model.clone()),
        model: Some(model),
        base_url: if base_url.is_empty() {
            None
        } else {
            Some(base_url)
        },
        api_key: if api_key.is_empty() {
            None
        } else {
            Some(api_key)
        },
        anthropic_url: None,
        protocol: None,
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

/// Read the value of an uncommented `KEY=value` line from a dotenv body.
/// Returns an empty string when the key is absent. Strips surrounding
/// whitespace and a single layer of matching quotes.
fn env_read(content: &str, key: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == key {
                let v = v.trim();
                let unquoted = v
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                    .unwrap_or(v);
                return unquoted.to_string();
            }
        }
    }
    String::new()
}
