//! Model configuration for kimicode.

use super::{
    ensure_parent, toml_write_table_value, toml_write_table_value_raw, toml_write_top, ApplyResult,
    ModelInfo,
};
use std::fs;
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  Kimi Code (MoonshotAI/kimi-code) — TOML config at ~/.kimi-code/config.toml
//   Schema (https://moonshotai.github.io/kimi-code/en/configuration/config-files):
//     default_model = "<alias>"              (top-level scalar)
//     [providers.<name>]   type / base_url / api_key / custom_headers / env
//     [models.<alias>]     provider / model / max_context_size / ...
//   Provider type maps from protocol: anthropic → "anthropic", else "openai".
//   We register a single "echobird" provider + "echobird" model alias, point
//   default_model at it, and write the api_key into the provider entry (the
//   CLI reads credentials ONLY from config — no shell-env fallback, so we
//   cannot inject via env). Surgical string-level edits (toml_write_top /
//   toml_write_table_value[_raw]) preserve the user's comments, thinking,
//   loop_control, permission, hooks, and any unrelated providers/models.
//   KIMI_CODE_HOME env overrides the whole data dir (detection blind spot —
//   same shape as MiMo's MIMOCODE_HOME). max_context_size is required (≥1);
//   we default to 262144 when ModelInfo has no context size.
// ════════════════════════════════════════════════════════════════

fn kimicode_dir() -> PathBuf {
    // KIMI_CODE_HOME overrides the whole data dir; the file is always
    // config.toml regardless (per the docs).
    if let Ok(home) = std::env::var("KIMI_CODE_HOME") {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    dirs::home_dir().unwrap_or_default().join(".kimi-code")
}

fn kimicode_config_path() -> PathBuf {
    kimicode_dir().join("config.toml")
}

pub(super) fn apply_kimicode(model_info: &ModelInfo) -> ApplyResult {
    let model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    if model_id.is_empty() {
        return ApplyResult {
            success: false,
            message: "Model ID is empty, cannot apply Kimi Code config".to_string(),
        };
    }

    // Kimi Code is exposed as OpenAI-only (apiProtocol in paths.json is
    // ["openai"]). The anthropic provider type uses the Anthropic SDK, which
    // appends /v1/messages to base_url itself — so a base_url carrying /v1
    // double-joins (404), and third-party Anthropic-compatible relays have
    // inconsistent path structures that we can't write correctly in general.
    // We don't expose Anthropic for kimi; the user can still configure an
    // anthropic provider manually in config.toml if they need it.
    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string();
    let provider_type = "openai";

    let api_key = model_info.api_key.as_deref().unwrap_or("");
    let provider = "echobird";
    let alias = "echobird"; // model alias == default_model value
    let providers_table = format!("providers.{}", provider);
    let models_table = format!("models.{}", alias);

    let config_path = kimicode_config_path();
    ensure_parent(&config_path);
    let mut content = fs::read_to_string(&config_path).unwrap_or_default();

    // Top-level default_model → our alias.
    content = toml_write_top(&content, "default_model", alias);

    // [providers.echobird]
    content = toml_write_table_value(&content, &providers_table, "type", provider_type);
    content = toml_write_table_value(&content, &providers_table, "base_url", &base_url);
    content = toml_write_table_value(&content, &providers_table, "api_key", api_key);

    // [models.echobird] — max_context_size is required (≥1); default 262144.
    content = toml_write_table_value(&content, &models_table, "provider", provider);
    content = toml_write_table_value(&content, &models_table, "model", model_id);
    content = toml_write_table_value_raw(&content, &models_table, "max_context_size", "262144");

    if let Err(e) = fs::write(&config_path, &content) {
        return ApplyResult {
            success: false,
            message: format!("Kimi Code config error: {}", e),
        };
    }

    log::info!(
        "[ToolConfigManager] Kimi Code configured: provider={}, alias={}, model={}, type={}",
        provider,
        alias,
        model_id,
        provider_type
    );
    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for Kimi Code. Restart `kimi` or use /model to select {}.",
            model_info.name.as_deref().unwrap_or(model_id),
            alias
        ),
    }
}

pub(super) fn read_kimicode() -> Option<ModelInfo> {
    let content = fs::read_to_string(kimicode_config_path()).ok()?;
    if content.trim().is_empty() {
        return None;
    }

    // Collect tables as (header, [(key, value), ...]) + track the top-level
    // default_model scalar. Values are stripped of inline comments and
    // surrounding quotes — sufficient for the simple scalars we own.
    let mut tables: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut cur: Option<(String, Vec<(String, String)>)> = None;
    let mut top_default_model: Option<String> = None;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(t) = cur.take() {
                tables.push(t);
            }
            let header = line.trim_matches(|c| c == '[' || c == ']').to_string();
            cur = Some((header, Vec::new()));
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim().to_string();
        let v = v
            .split('#')
            .next()
            .unwrap_or(v)
            .trim()
            .trim_matches('"')
            .to_string();
        if let Some((_, kvs)) = cur.as_mut() {
            kvs.push((k, v));
        } else if k == "default_model" {
            top_default_model = Some(v);
        }
    }
    if let Some(t) = cur.take() {
        tables.push(t);
    }

    // Only round-trip when our echobird alias is the active default. If the
    // user switched to a managed/native model via /model, return None so the
    // model-picker shows nothing selected (correct — not an EchoBird model).
    let alias = top_default_model?;
    if alias != "echobird" {
        return None;
    }

    let prov = tables.iter().find(|(h, _)| h == "providers.echobird")?;
    let model = tables.iter().find(|(h, _)| h == "models.echobird")?;
    let model_id = model
        .1
        .iter()
        .find(|(k, _)| k == "model")
        .map(|(_, v)| v.clone())?;
    if model_id.is_empty() {
        return None;
    }

    let provider_type = prov
        .1
        .iter()
        .find(|(k, _)| k == "type")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let base_url = prov
        .1
        .iter()
        .find(|(k, _)| k == "base_url")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let api_key = prov
        .1
        .iter()
        .find(|(k, _)| k == "api_key")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let (base, anthro, protocol) = if provider_type == "anthropic" {
        (None, Some(base_url), "anthropic")
    } else {
        (Some(base_url), None, "openai")
    };
    Some(ModelInfo {
        name: Some(model_id.clone()),
        model: Some(model_id),
        base_url: base,
        api_key: if api_key.is_empty() {
            None
        } else {
            Some(api_key)
        },
        anthropic_url: anthro,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore_kimicode_to_official() -> ApplyResult {
    let config_path = kimicode_config_path();
    let content = fs::read_to_string(&config_path).unwrap_or_default();
    if content.trim().is_empty() {
        return ApplyResult {
            success: true,
            message: "Kimi Code config not found — already at official.".to_string(),
        };
    }

    // Remove our [providers.echobird] + [models.echobird] table blocks and the
    // default_model line we set. Preserve everything else (user providers,
    // managed:kimi-code OAuth entry, thinking, hooks, permission, …). On next
    // launch Kimi Code falls back to /login (OAuth or Moonshot platform key).
    let new_content = remove_toml_table(&content, "providers.echobird");
    let new_content = remove_toml_table(&new_content, "models.echobird");
    let new_content = remove_toml_top_key(&new_content, "default_model");

    if new_content != content {
        if let Err(e) = fs::write(&config_path, &new_content) {
            return ApplyResult {
                success: false,
                message: format!("Kimi Code restore error: {}", e),
            };
        }
    }

    ApplyResult {
        success: true,
        message: "Kimi Code restored — echobird provider/model removed, default_model cleared. Kimi Code will fall back to /login on next launch.".to_string(),
    }
}

/// Remove a `[table]` block (header + its key=value lines, up to the next
/// section header or EOF). Returns content unchanged if the table is absent.
fn remove_toml_table(content: &str, table: &str) -> String {
    let header = format!("[{}]", table);
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let start = lines.iter().position(|l| l.trim() == header.as_str());
    let Some(start) = start else {
        return content.to_string();
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(i, l)| {
            let t = l.trim();
            if t.starts_with('[') && t.ends_with(']') {
                Some(i)
            } else {
                None
            }
        })
        .unwrap_or(lines.len());
    lines.drain(start..end);
    let mut joined = lines.join("\n");
    if !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Remove a top-level `key = ...` line (before any section header).
fn remove_toml_top_key(content: &str, key: &str) -> String {
    let mut first_section: Option<usize> = None;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut remove: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if first_section.is_none() && t.starts_with('[') {
            first_section = Some(i);
        }
        if first_section.is_some() {
            break;
        }
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            if k.trim() == key {
                remove = Some(i);
                break;
            }
        }
    }
    if let Some(i) = remove {
        lines.remove(i);
    }
    lines.join("\n")
}
