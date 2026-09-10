//! Model configuration for grok.

use super::{
    echobird_dir, ensure_parent, read_json_file, toml_escape, write_json_file, ApplyResult,
    ModelInfo,
};
use std::fs;
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  Grok Build CLI (xAI) — sectioned TOML
//
// Config shape per https://docs.x.ai/build/overview:
//
//   [model.echobird]
//   model    = "deepseek-chat"
//   base_url = "https://api.deepseek.com/v1"
//   name     = "EchoBird"
//   api_key  = "<real key>"
//
//   [models]
//   default  = "echobird"
//
// The real API key is written INLINE as `api_key` (grok's documented
// inline-key field — verified against grok 0.2.93: it sends the value
// as `Authorization: Bearer <key>` with no env var needed). This makes
// `grok` runnable from ANY terminal, not just one EchoBird launched —
// manual launch reads config.toml directly, no env-var injection
// required. (Using `env_key` instead would tie grok to EchoBird's
// process_manager env injection and break manual launches.)
//
// We ALSO keep ~/.echobird/grok.json as the read-side source of truth
// (holds the key + URL so read_grok / the UI can round-trip without
// re-parsing TOML). process_manager still injects the key into env as
// a harmless redundancy for EchoBird-launched sessions.
//
// We rewrite ONLY the [model.echobird] and [models] sections — every
// other thing the user has in ~/.grok/config.toml (skills paths, plugin
// paths, marketplace sources, permission mode, hooks) is preserved.
// ════════════════════════════════════════════════════════════════

const GROK_PROFILE_NAME: &str = "echobird";

const GROK_API_KEY_ENV: &str = "ECHOBIRD_GROK_API_KEY";

fn grok_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".grok")
        .join("config.toml")
}

/// Remove all lines belonging to `[section_header]` (e.g. `[models]` or
/// `[model.echobird]`) and continue until the next `[…]` table header or
/// EOF. Preserves every other line verbatim.
fn toml_strip_section(content: &str, section_header: &str) -> String {
    let header_trimmed = section_header.trim();
    let mut out: Vec<&str> = Vec::with_capacity(content.lines().count());
    let mut skipping = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            skipping = t == header_trimmed;
            if skipping {
                continue;
            }
        }
        if skipping {
            continue;
        }
        out.push(line);
    }
    // Trim any trailing blank lines from the strip, leave one separator at the end.
    while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        out.pop();
    }
    out.join("\n")
}

pub(super) fn apply_grok(model_info: &ModelInfo) -> ApplyResult {
    let model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    if model_id.is_empty() {
        return ApplyResult {
            success: false,
            message: "Model ID is empty.".to_string(),
        };
    }
    let base_url = model_info
        .base_url
        .as_deref()
        .map(|u| u.trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://api.x.ai/v1".to_string());
    let display_name = model_info.name.as_deref().unwrap_or(model_id);
    let api_key = match model_info.api_key.as_deref().filter(|k| !k.is_empty()) {
        Some(k) => k.to_string(),
        None => {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Grok config.".to_string(),
            };
        }
    };

    let config_path = grok_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();

    // Surgically remove any prior [model.echobird] and [models] sections,
    // then append fresh ones. Preserves the user's other config.
    let our_model_header = format!("[model.{}]", GROK_PROFILE_NAME);
    let mut stripped = toml_strip_section(&existing, &our_model_header);
    stripped = toml_strip_section(&stripped, "[models]");

    let new_section = format!(
        "\n\n[model.{name}]\nmodel = \"{model}\"\nbase_url = \"{base}\"\nname = \"{display}\"\napi_key = \"{key}\"\n\n[models]\ndefault = \"{name}\"\n",
        name = GROK_PROFILE_NAME,
        model = toml_escape(model_id),
        base = toml_escape(&base_url),
        display = toml_escape(display_name),
        key = toml_escape(&api_key),
    );
    let final_content = format!("{}{}", stripped.trim_end(), new_section);

    ensure_parent(&config_path);
    if let Err(e) = fs::write(&config_path, &final_content) {
        return ApplyResult {
            success: false,
            message: format!("Grok config error: {}", e),
        };
    }

    // Relay file — read-side source of truth (read_grok parses this, not
    // the TOML). process_manager also still injects the key into env as a
    // harmless redundancy for EchoBird-launched sessions; the inline
    // api_key in config.toml is what makes manual launches work.
    let relay_path = echobird_dir().join("grok.json");
    let relay = serde_json::json!({
        "apiKey": api_key,
        "baseUrl": base_url,
        "actualModel": model_id,
        "modelName": display_name,
        "envKey": GROK_API_KEY_ENV,
    });
    let _ = write_json_file(&relay_path, &relay);

    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" applied to Grok. Run `grok` from any terminal — the API key is written into ~/.grok/config.toml, so manual launch works without EchoBird.",
            display_name
        ),
    }
}

pub(super) fn restore_grok_to_official() -> ApplyResult {
    let config_path = grok_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();

    // Only strip what we wrote; leave every other section alone so grok
    // falls back to its xAI default (auth.json / XAI_API_KEY).
    let our_model_header = format!("[model.{}]", GROK_PROFILE_NAME);
    let mut stripped = toml_strip_section(&existing, &our_model_header);
    stripped = toml_strip_section(&stripped, "[models]");

    let final_content = stripped.trim_end().to_string() + "\n";

    if existing.is_empty() && final_content.trim().is_empty() {
        // Nothing to do — config was empty, no echobird block to remove.
    } else if let Err(e) = fs::write(&config_path, &final_content) {
        return ApplyResult {
            success: false,
            message: format!("Failed to clean Grok config: {}", e),
        };
    }

    let relay_path = echobird_dir().join("grok.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    ApplyResult {
        success: true,
        message:
            "Grok restored to xAI official. Run `grok login` if you haven't authenticated with xAI."
                .to_string(),
    }
}

pub(super) fn read_grok() -> Option<ModelInfo> {
    // Source of truth is the relay file (holds the real key + URL).
    // ~/.grok/config.toml stores the inline api_key too, but we read the
    // relay here so the UI round-trips without re-parsing TOML.
    let relay_path = echobird_dir().join("grok.json");
    let relay = read_json_file(&relay_path)?;

    let model = relay
        .get("actualModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let base_url = relay
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let api_key = relay
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Some(ModelInfo {
        name: Some(model.clone()),
        model: Some(model),
        base_url,
        api_key,
        anthropic_url: None,
        protocol: Some("openai".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}
