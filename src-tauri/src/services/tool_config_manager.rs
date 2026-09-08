// Tool config manager �?handles model configuration for all tools
// Ports the old Electron model.ts/model.cjs logic into Rust
// Each custom tool has its own apply/read function

use std::fs;
use std::path::{Path, PathBuf};

use crate::services::codex_catalog;
use crate::services::tool_manager;

mod omp;

/// Model info to apply to a tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_model: Option<String>,
    /// When true, write the provider's REAL base_url + api_key into
    /// ~/.codex/config.toml + ~/.codex/auth.json so Codex talks to the
    /// upstream directly, bypassing our local proxy. Used for relay
    /// stations (cc-vibe.com etc.) that already serve the Responses
    /// protocol natively — protocol-translation isn't needed.
    /// Only consumed by `apply_codex`; other tools ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_mode: Option<bool>,
    /// Codex-only. When true, write the provider's REAL base_url + the
    /// REAL model id (not our "gpt-5.5" display alias) into
    /// ~/.codex/config.toml so Codex talks to the upstream directly,
    /// bypassing our local proxy. For third-party models that natively
    /// speak the Responses protocol (e.g. Volcengine ARK `glm-5.2`) —
    /// no proxy hop, no Responses→Chat translation, no model-id spoof.
    /// Mutually exclusive with `relay_mode` (UI auto-flips so both are
    /// never on). Only consumed by `apply_codex`; other tools ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responses_passthrough: Option<bool>,
    /// Codex-only. `Some(false)` → write `web_search = "disabled"` into
    /// config.toml so Codex removes its built-in web-search tool; otherwise
    /// `web_search = "live"` (unrestricted real-time retrieval). Note: NOT
    /// Codex's default `"cached"` — that's an OpenAI-maintained index with
    /// no external web access, useless for our third-party upstreams.
    /// Consumed by `apply_codex`; other tools ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search: Option<bool>,
    /// Claude Code relay-only. When `Some(true)` AND `relay_mode` is on,
    /// append `[1m]` to the 1M-capable env vars (`ANTHROPIC_MODEL` /
    /// `ANTHROPIC_DEFAULT_SONNET_MODEL` / `ANTHROPIC_DEFAULT_OPUS_MODEL` / `ANTHROPIC_DEFAULT_FABLE_MODEL`)
    /// written to ~/.claude/settings.json so Claude Code budgets the 1M
    /// context window. Claude Code strips the suffix before sending the id
    /// upstream, so the provider still sees the bare id. `HAIKU` and
    /// `CLAUDE_CODE_SUBAGENT_MODEL` never get the suffix — no 1M concept.
    /// No effect in bridge mode (bridge writes no model id — CC uses its
    /// built-in claude-* ids, which already budget the full window).
    /// Only consumed by `apply_claudecode`; other tools ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_m_context: Option<bool>,
}

/// Result of applying a model config
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyResult {
    pub success: bool,
    pub message: String,
}

// ─── Helpers ───

fn echobird_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".echobird")
}

fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }
}

/// Canonical Codex config identity. Every apply_codex run produces a
/// config.toml with the same provider name and display model regardless
/// of which third-party endpoint is actually behind the proxy — keeps
/// the config file clean and avoids stale orphan sections accumulating
/// across model switches. The launcher proxy translates the display
/// model to the real provider's model ID when forwarding requests.
const CODEX_PROVIDER: &str = "OpenAI";
/// Codex's display model alias. Pinned in Bridge and Relay sessions so
/// Codex thinks it's talking to a gpt-5 family model (model-id deception
/// moat); the proxy / relay station rewrites to the real id. Exposed
/// `pub(crate)` so `codex_proxy::config_manager` can pass it to
/// `write_codex_canonical_fields` on the pre-spawn self-heal path.
pub(crate) const CODEX_DISPLAY_MODEL: &str = "gpt-5.5";

// Stable proxy port. Sourced from codex_proxy (the listener owner) so
// the constant has exactly one definition. Bound permanently so
// ~/.codex/config.toml can hold a fixed `base_url = "http://127.0.0.1:
// <PORT>/v1"` — model switches happen by rewriting only
// ~/.echobird/codex.json, which the proxy reads fresh on every request.
use crate::services::codex_proxy::CODEX_PROXY_PORT;

/// Extract domain name from URL for use in identifiers
/// Example: "https://api.openai.com/v1" -> "api_openai_com"
fn extract_domain_name(url: &str) -> String {
    url.trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or(url)
        .split(':')
        .next()
        .unwrap_or(url)
        .replace('.', "_")
}

/// Read JSON file, return Value or None
fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_jsonc_file(path: &Path) -> Option<serde_json::Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc_comments(&content)).ok()
}

fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            out.push(c);
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }

        if c == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    for next in chars.by_ref() {
                        if prev == '*' && next == '/' {
                            break;
                        }
                        prev = next;
                    }
                    continue;
                }
                _ => {}
            }
        }

        out.push(c);
    }

    out
}

/// Write JSON value to file with pretty formatting
fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    ensure_parent(path);
    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// `serde_yaml_ng` shorthand: a string value.
fn yaml_str(s: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::Value::String(s.to_string())
}

/// `serde_yaml_ng` shorthand: an empty mapping value.
fn yaml_map() -> serde_yaml_ng::Value {
    serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new())
}

/// Coerce a YAML value into a mapping, returning `&mut Mapping`. Replaces a
/// non-mapping value with a fresh mapping — the YAML analogue of the jsonc
/// guards used by the openscience/zcode configs.
fn yaml_as_map_mut(value: &mut serde_yaml_ng::Value) -> &mut serde_yaml_ng::Mapping {
    if !value.is_mapping() {
        *value = yaml_map();
    }
    value.as_mapping_mut().expect("coerced to mapping")
}

/// Get-or-create a child mapping under `key` in `map`, returning `&mut Mapping`.
fn yaml_child_map<'a>(
    map: &'a mut serde_yaml_ng::Mapping,
    key: &str,
) -> &'a mut serde_yaml_ng::Mapping {
    let child = map.entry(yaml_str(key)).or_insert_with(yaml_map);
    yaml_as_map_mut(child)
}

/// Read a child value from a mapping by string key (None if absent/non-mapping).
fn yaml_get<'a>(value: &'a serde_yaml_ng::Value, key: &str) -> Option<&'a serde_yaml_ng::Value> {
    value.as_mapping()?.get(yaml_str(key))
}

/// Read a YAML file into a `serde_yaml_ng::Value` (None if missing/unparseable).
fn read_yaml_file(path: &Path) -> Option<serde_yaml_ng::Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_yaml_ng::from_str(&content).ok()
}

/// Write a `serde_yaml_ng::Value` back to a YAML file (block style), creating
/// parent directories as needed. Mirrors `write_json_file`.
fn write_yaml_file(path: &Path, value: &serde_yaml_ng::Value) -> Result<(), String> {
    ensure_parent(path);
    let content = serde_yaml_ng::to_string(value).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

// ─── Known ModelInfo fields ───

const KNOWN_MODEL_FIELDS: &[&str] = &["id", "name", "baseUrl", "apiKey", "model", "protocol"];

// ─── Per-model capability registry ───
//
// Some tool configs must reflect a model's real context window and input
// modalities rather than a one-size-fits-all default. Codex's
// `model_context_window` / `model_auto_compact_token_limit` and ZCode's
// `modalities.input` are the two places a wrong default silently caps or
// mis-advertises a third-party model. The values below are sourced from the
// provider's official model specs so apply_codex / apply_zcode stay
// data-driven — no model-id branching inside the apply logic.

pub(crate) const DEFAULT_CODEX_CONTEXT_WINDOW: u64 = 1_000_000;

/// Look up a model's real context window (in tokens). Returns
/// `DEFAULT_CODEX_CONTEXT_WINDOW` for models the registry does not list —
/// the historic Codex default — so unknown models keep working as before.
fn model_context_window_for(model_id: &str) -> u64 {
    match model_id {
        "MiniMax-M3" => 1_000_000,
        "MiniMax-M2.7" => 204_800,
        _ => DEFAULT_CODEX_CONTEXT_WINDOW,
    }
}

/// Derive the auto-compact token limit (90% of the context window) that
/// Codex writes as `model_auto_compact_token_limit`. Keeping it proportional
/// to the real window stops Codex from trying to compact at 900k on a model
/// whose entire window is only 204,800 tokens.
fn codex_compact_limit_for(context_window: u64) -> u64 {
    context_window * 9 / 10
}

/// Look up a model's real input modalities. Returns `["text"]` for models the
/// registry does not list so ZCode's `modalities.input` keeps the safe
/// text-only default for unknown ids.
fn model_input_modalities_for(model_id: &str) -> &'static [&'static str] {
    match model_id {
        "MiniMax-M3" => &["text", "image", "video"],
        _ => &["text"],
    }
}

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
//  APPLY MODEL �?main entry point
// ════════════════════════════════════════════════════════════════

pub async fn apply_model_to_tool(tool_id: &str, model_info: ModelInfo) -> ApplyResult {
    log::info!("[ToolConfigManager] Applying model to {}", tool_id);
    let model_info = normalize_model_info_for_tool(tool_id, model_info);

    // Dispatch custom tools to their own handlers
    match tool_id {
        // OpenClaw: direct write to ~/.openclaw/openclaw.json (no patch needed since v2026.3.13)
        "openclaw" => return apply_openclaw(&model_info),

        // Type 3: Direct JSON overwrite (special format).
        // CLI and Desktop share ~/.config/opencode/opencode.jsonc — one apply
        // covers both (Desktop spawns `opencode serve` reading the same file).
        "opencode" | "opencodedesktop" => return apply_opencode(&model_info),

        // MiMo Code (Xiaomi fork of OpenCode): same provider schema,
        // own config at ~/.config/mimocode/mimocode.json(c).
        "mimocode" => return apply_mimocode(&model_info),

        // Kilo Code (Kilo fork of OpenCode): same provider schema,
        // own config at ~/.config/kilo/kilo.json.
        "kilo" => return apply_kilo(&model_info),

        // OpenScience (open-source Claude Science alt): models.dev provider
        // schema, dual-protocol (npm @ai-sdk/anthropic | @ai-sdk/openai-compatible),
        // config at ~/.config/openscience/openscience.json.
        "openscience" => return apply_openscience(&model_info),
        "dsh" => return apply_dsh(&model_info),

        // ZCode (Z.AI desktop OpenCode fork): OpenCode schema but the provider
        // uses a `kind` discriminator and supports BOTH protocols; config at
        // ~/.zcode/v2/config.json.
        "zcode" => return apply_zcode(&model_info),

        // Codex CLI and ChatGPT desktop share ~/.codex/config.toml.
        "codex" | "chatgptdesktop" => return apply_codex(tool_id, &model_info),

        // Claude Desktop 3P profile (Anthropic-native providers only)
        "claudedesktop" => return apply_claudedesktop(&model_info),

        // Claude Code — same model-id-rewrite proxy path as Claude Desktop,
        // but writes ~/.claude/settings.json env vars + its own relay file.
        "claudecode" => return apply_claudecode(&model_info),

        // Type 4: YAML
        "aider" => return apply_aider(&model_info),

        // Grok Build CLI (xAI) — sectioned TOML with [model.echobird] + [models]
        "grok" => return apply_grok(&model_info),

        // Qwen Code: direct write to ~/.qwen/settings.json
        "qwencode" => return apply_qwen_code(&model_info),

        // Pi (earendil-works/pi): writes ~/.pi/agent/{models,settings}.json
        "pi" => return apply_pi(&model_info),
        "omp" => return omp::apply(&model_info),

        // Kimi Code (Moonshot AI): TOML at ~/.kimi-code/config.toml
        "kimicode" => return apply_kimicode(&model_info),

        // Vibe-Trading (HKUDS): dotenv at ~/.vibe-trading/.env. Every endpoint
        // we point it at is OpenAI-compatible, so pin LANGCHAIN_PROVIDER=openai.
        "vibe-trading" => return apply_vibe_trading(&model_info),

        // WorkBuddy (Tencent CodeBuddy 办公版): ~/.workbuddy/models.json.
        "workbuddy" => return apply_workbuddy(&model_info),

        // Plug-and-play: check config.json custom flag
        _ => {
            if let Some((def, _)) = tool_manager::get_tool_config_mapping(tool_id) {
                if def.config_mapping.custom {
                    return apply_echobird_relay(tool_id, &model_info, false);
                }
            }
        }
    }

    apply_generic_json(tool_id, &model_info).await
}

fn normalize_model_info_for_tool(tool_id: &str, mut model_info: ModelInfo) -> ModelInfo {
    if tool_id == "claudecode" && model_info.protocol.as_deref() == Some("anthropic") {
        if let Some(ref mut base_url) = model_info.base_url {
            let trimmed = base_url.trim_end_matches('/').to_string();
            if let Some(without_v1) = trimmed.strip_suffix("/v1") {
                *base_url = without_v1.to_string();
            } else {
                *base_url = trimmed;
            }
        }
    }
    model_info
}

// ════════════════════════════════════════════════════════════════
//  RESTORE TO OFFICIAL — delete config so tool regenerates defaults
// ════════════════════════════════════════════════════════════════

/// Delete the tool's config file (and any Echobird relay side-channel) so
/// the tool itself regenerates a fresh, vendor-default config on next launch.
/// Used by the App Desktop "restore to official" flow.
pub async fn restore_tool_to_official(tool_id: &str) -> ApplyResult {
    let config_path = match tool_manager::get_tool_config_mapping(tool_id) {
        Some((_, path)) => path,
        None => {
            return ApplyResult {
                success: false,
                message: format!("Unknown tool: {}", tool_id),
            }
        }
    };

    if matches!(tool_id, "codex" | "chatgptdesktop") {
        return restore_codex_to_official(tool_id, &config_path);
    }
    if tool_id == "claudedesktop" {
        return restore_claudedesktop_to_official();
    }
    if tool_id == "claudecode" {
        return restore_claudecode_to_official();
    }
    if tool_id == "grok" {
        return restore_grok_to_official();
    }
    if matches!(tool_id, "opencode" | "opencodedesktop") {
        return restore_opencode_to_official();
    }
    if tool_id == "mimocode" {
        return restore_mimocode_to_official();
    }
    if tool_id == "kilo" {
        return restore_kilo_to_official();
    }
    if tool_id == "zcode" {
        return restore_zcode_to_official();
    }
    if tool_id == "pi" {
        return restore_pi_to_official();
    }
    if tool_id == "omp" {
        return omp::restore();
    }
    if tool_id == "kimicode" {
        return restore_kimicode_to_official();
    }
    if tool_id == "openscience" {
        return restore_openscience_to_official();
    }
    if tool_id == "dsh" {
        return restore_dsh_to_official();
    }

    // Side-channel relay file (openclaw and other "custom" tools) —
    // best-effort cleanup, ignored if absent.
    let relay_path = echobird_dir().join(format!("{}.json", tool_id));
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    if !config_path.exists() {
        return ApplyResult {
            success: true,
            message: format!(
                "{} already at defaults — no config file to remove.",
                tool_id
            ),
        };
    }

    match fs::remove_file(&config_path) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] Restored {} — deleted {:?}",
                tool_id,
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "{} restored — config deleted, tool will regenerate defaults on next launch.",
                    tool_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: format!("Failed to delete {} config: {}", tool_id, e),
        },
    }
}

// ════════════════════════════════════════════════════════════════
//  GET MODEL INFO �?main entry point
// ════════════════════════════════════════════════════════════════

pub async fn get_tool_model_info(tool_id: &str) -> Option<ModelInfo> {
    match tool_id {
        "openclaw" => return read_openclaw(),
        "opencode" | "opencodedesktop" => return read_opencode(),
        "mimocode" => return read_mimocode(),
        "kilo" => return read_kilo(),
        "openscience" => return read_openscience(),
        "dsh" => return read_dsh(),
        "zcode" => return read_zcode(),
        "codex" | "chatgptdesktop" => return read_codex(),
        "claudedesktop" => return read_claudedesktop(),
        "claudecode" => return read_claudecode(),
        "aider" => return read_aider(),
        "grok" => return read_grok(),
        "qwencode" => return read_qwen_code(),
        "pi" => return read_pi(),
        "omp" => return omp::read(),
        "kimicode" => return read_kimicode(),
        "vibe-trading" => return read_vibe_trading(),
        "workbuddy" => return read_workbuddy(),
        // Plug-and-play: check config.json custom flag
        _ => {
            if let Some((def, _)) = tool_manager::get_tool_config_mapping(tool_id) {
                if def.config_mapping.custom {
                    return read_echobird_relay(tool_id);
                }
            }
        }
    }

    read_generic_json(tool_id)
}

// ════════════════════════════════════════════════════════════════
//  Type 1: Generic JSON mapping (ClaudeCode, etc.)
// ════════════════════════════════════════════════════════════════

async fn apply_generic_json(tool_id: &str, model_info: &ModelInfo) -> ApplyResult {
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

fn read_generic_json(tool_id: &str) -> Option<ModelInfo> {
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

// ════════════════════════════════════════════════════════════════
//  Type 2: Echobird relay JSON (OpenClaw + custom plug-and-play tools)
//  Write to ~/.echobird/{tool_id}.json
// ════════════════════════════════════════════════════════════════

fn apply_echobird_relay(
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

fn read_echobird_relay(tool_id: &str) -> Option<ModelInfo> {
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

// ════════════════════════════════════════════════════════════════
//  OpenClaw: Direct write to ~/.openclaw/openclaw.json
//  v2026.3.13+: models.providers custom provider, mode: "merge"
//  No longer patches openclaw.mjs — writes native config directly.
// ════════════════════════════════════════════════════════════════

fn apply_openclaw(model_info: &ModelInfo) -> ApplyResult {
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

fn read_openclaw() -> Option<ModelInfo> {
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

// ════════════════════════════════════════════════════════════════
//  Type 3b: OpenCode
//  ~/.config/opencode/opencode.json  {provider: {X: {npm, options, models}}}
// ════════════════════════════════════════════════════════════════

fn apply_opencode(model_info: &ModelInfo) -> ApplyResult {
    // Write echobird relay JSON — the patched launcher reads this
    let config_path = echobird_dir().join("opencode.json");
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
    let provider_name = model_id; // Use model ID instead of domain name

    let config = serde_json::json!({
        "apiKey": model_info.api_key.as_deref().unwrap_or(""),
        "baseUrl": base_url,
        "modelId": model_id,
        "modelName": model_info.name.as_deref().unwrap_or(model_id),
        "providerName": provider_name,
    });

    if let Err(e) = write_opencode_native_config(model_info, model_id, &base_url, provider_name) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] OpenCode config written to {:?}",
                config_path
            );
            crate::services::tool_patcher::patch_opencode();
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for OpenCode. Use /models in TUI to select echobird/{}.",
                    model_info.name.as_deref().unwrap_or(model_id), model_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

fn write_opencode_native_config(
    model_info: &ModelInfo,
    model_id: &str,
    base_url: &str,
    provider_name: &str,
) -> Result<(), String> {
    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("opencode")
        .join("opencode.jsonc");

    let mut config = read_jsonc_file(&config_path)
        .or_else(|| read_json_file(&config_path.with_extension("json")))
        .unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://opencode.ai/config.json");
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
        "name": provider_name,
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

    write_json_file(&config_path, &config)
}

fn read_opencode() -> Option<ModelInfo> {
    let native_path = dirs::home_dir()?
        .join(".config")
        .join("opencode")
        .join("opencode.jsonc");
    if let Some(info) = read_opencode_native_config(&native_path)
        .or_else(|| read_opencode_native_config(&native_path.with_extension("json")))
    {
        return Some(info);
    }

    // Read from echobird relay JSON
    let config_path = echobird_dir().join("opencode.json");
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
        protocol: None,
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

fn read_opencode_native_config(path: &Path) -> Option<ModelInfo> {
    let config = if path.extension().and_then(|e| e.to_str()) == Some("jsonc") {
        read_jsonc_file(path)?
    } else {
        read_json_file(path)?
    };
    let selected = config.get("model")?.as_str()?;
    let (provider_id, model_id) = selected.split_once('/')?;
    let provider = config.pointer(&format!("/provider/{}", provider_id))?;

    Some(ModelInfo {
        name: provider
            .pointer(&format!("/models/{}/name", model_id))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: Some(model_id.to_string()),
        base_url: provider
            .pointer("/options/baseURL")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        api_key: provider
            .pointer("/options/apiKey")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        anthropic_url: None,
        protocol: Some("openai".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

fn restore_opencode_to_official() -> ApplyResult {
    let relay_path = echobird_dir().join("opencode.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    let native_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("opencode")
        .join("opencode.jsonc");

    if !native_path.exists() {
        return ApplyResult {
            success: true,
            message: "OpenCode already at defaults - no config file to update.".to_string(),
        };
    }

    let mut updated_any = false;
    for path in [&native_path, &native_path.with_extension("json")] {
        if !path.exists() {
            continue;
        }

        let mut config = match read_jsonc_file(path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!("Failed to parse OpenCode config: {}", path.display()),
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

        if let Err(e) = write_json_file(path, &config) {
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
            message: "OpenCode restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "OpenCode already at defaults - no config file to update.".to_string(),
        }
    }
}

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

fn apply_mimocode(model_info: &ModelInfo) -> ApplyResult {
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

fn read_mimocode() -> Option<ModelInfo> {
    // Same provider schema as OpenCode — reuse its native-config reader.
    let dir = mimocode_config_dir();
    read_opencode_native_config(&dir.join("mimocode.jsonc"))
        .or_else(|| read_opencode_native_config(&dir.join("mimocode.json")))
}

// ════════════════════════════════════════════════════════════════
//  ZCode — Z.AI's desktop OpenCode fork. ~/.zcode/v2/config.json.
//  OpenCode config schema, but the provider uses a `kind` discriminator
//  ("openai-compatible" | "anthropic") instead of OpenCode's `npm`, and
//  it supports BOTH protocols. The native config write is the whole
//  mechanism — desktop app, no launcher patch, no ~/.echobird relay.
//  Default model is the OpenCode-standard top-level `model` selector.
// ════════════════════════════════════════════════════════════════

fn zcode_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".zcode")
        .join("v2")
        .join("config.json")
}

fn apply_zcode(model_info: &ModelInfo) -> ApplyResult {
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

    // Local llama-server needs no real key; mirror the dummy used elsewhere.
    let is_local = base_url.contains("127.0.0.1") || base_url.contains("localhost");
    let api_key = match model_info.api_key.as_deref() {
        Some(k) if !k.is_empty() => k.to_string(),
        _ if is_local => "local-no-auth".to_string(),
        _ => {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply config".to_string(),
            }
        }
    };

    // Protocol → provider `kind`. The frontend already collapsed the chosen
    // protocol's URL into base_url, so base_url is correct for either kind.
    let kind = if model_info.protocol.as_deref() == Some("anthropic") {
        "anthropic"
    } else {
        "openai-compatible"
    };

    let config_path = zcode_config_path();
    let mut config = read_jsonc_file(&config_path).unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://opencode.ai/config.json");
    }
    if !config
        .get("provider")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        config["provider"] = serde_json::json!({});
    }

    let provider_id = "echobird";
    let display_name = model_info.name.as_deref().unwrap_or(model_id);
    // Resolve the real input modalities for the selected model so ZCode's
    // `modalities.input` reflects image/video support when the model has it,
    // instead of declaring every model text-only.
    let input_modalities = model_input_modalities_for(model_id);
    config["provider"][provider_id] = serde_json::json!({
        "name": display_name,
        "kind": kind,
        "options": {
            "apiKey": api_key,
            "baseURL": base_url,
            "apiKeyRequired": true
        },
        "source": "custom",
        "models": {
            model_id: {
                "name": display_name,
                "modalities": { "input": input_modalities, "output": ["text"] }
            }
        }
    });
    // OpenCode-standard active-model selectors (UI stores its pick elsewhere;
    // these set the configured default that ZCode reads on launch).
    config["model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));
    config["small_model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] ZCode config written to {:?}",
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for ZCode. Restart ZCode to apply.",
                    display_name
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

fn read_zcode() -> Option<ModelInfo> {
    let config = read_jsonc_file(&zcode_config_path())?;
    let selected = config.get("model")?.as_str()?;
    let (provider_id, model_id) = selected.split_once('/')?;
    let provider = config.pointer(&format!("/provider/{}", provider_id))?;
    let kind = provider
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-compatible");
    let protocol = if kind == "anthropic" {
        "anthropic"
    } else {
        "openai"
    };

    Some(ModelInfo {
        name: provider
            .pointer(&format!("/models/{}/name", model_id))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(model_id.to_string())),
        model: Some(model_id.to_string()),
        base_url: provider
            .pointer("/options/baseURL")
            .and_then(|v| v.as_str())
            .map(String::from),
        api_key: provider
            .pointer("/options/apiKey")
            .and_then(|v| v.as_str())
            .map(String::from),
        anthropic_url: None,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

fn restore_zcode_to_official() -> ApplyResult {
    let path = zcode_config_path();
    if !path.exists() {
        return ApplyResult {
            success: true,
            message: "ZCode already at defaults — no config file to update.".to_string(),
        };
    }
    let mut config = match read_jsonc_file(&path) {
        Some(c) => c,
        None => {
            return ApplyResult {
                success: false,
                message: format!("Failed to parse ZCode config: {}", path.display()),
            }
        }
    };
    if let Some(provider) = config.get_mut("provider").and_then(|v| v.as_object_mut()) {
        provider.remove("echobird");
    }
    for key in ["model", "small_model"] {
        if config
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("echobird/"))
            .unwrap_or(false)
        {
            tool_manager::delete_nested_value(&mut config, key);
        }
    }
    match write_json_file(&path, &config) {
        Ok(_) => ApplyResult {
            success: true,
            message: "ZCode restored — Echobird provider removed.".to_string(),
        },
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
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

fn restore_mimocode_to_official() -> ApplyResult {
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

// ════════════════════════════════════════════════════════════════
//  Type 3c-2: Kilo Code — Kilo's fork of OpenCode (binary: `kilo`)
//  ~/.config/kilo/kilo.json(c)  {provider: {X: {npm, options, models}}}
//  Same provider schema as OpenCode / MiMo Code; no launcher patch and no
//  relay file — the curl install is a standalone binary, so the native
//  config write is the whole mechanism.
// ════════════════════════════════════════════════════════════════

fn kilo_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("kilo")
}

/// Kilo Code merges global configs in order config.json → kilo.json →
/// kilo.jsonc (later wins). Target the highest-priority file that exists so
/// our keys always take effect; default new installs to the documented
/// canonical name, kilo.json.
fn kilo_write_path() -> PathBuf {
    let jsonc = kilo_config_dir().join("kilo.jsonc");
    if jsonc.exists() {
        jsonc
    } else {
        kilo_config_dir().join("kilo.json")
    }
}

fn apply_kilo(model_info: &ModelInfo) -> ApplyResult {
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

    let config_path = kilo_write_path();
    let mut config = read_jsonc_file(&config_path).unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://app.kilo.ai/config.json");
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
                "[ToolConfigManager] Kilo Code config written to {:?}",
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for Kilo Code. Restart `kilo` or use /models to select {}/{}.",
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

fn read_kilo() -> Option<ModelInfo> {
    // Same provider schema as OpenCode — reuse its native-config reader.
    let dir = kilo_config_dir();
    read_opencode_native_config(&dir.join("kilo.jsonc"))
        .or_else(|| read_opencode_native_config(&dir.join("kilo.json")))
}

/// Model id for the launcher's `--model echobird/<id>` injection. Kilo Code
/// keeps no ~/.echobird relay file (the native config is the single source
/// of truth), so the launcher reads the selection from here instead. Some
/// only while the native config currently selects our provider — after a
/// restore or a hand-edit to another provider, no flag is injected and
/// Kilo Code's own config/model resolution applies.
pub fn kilo_echobird_model() -> Option<String> {
    let dir = kilo_config_dir();
    let config = read_jsonc_file(&dir.join("kilo.jsonc"))
        .or_else(|| read_jsonc_file(&dir.join("kilo.json")))?;
    let selected = config.get("model")?.as_str()?;
    selected.strip_prefix("echobird/").map(|s| s.to_string())
}

fn restore_kilo_to_official() -> ApplyResult {
    // Builds prior to the dedicated kilo arm fell through to the
    // relay side-channel; clean that file up so it can't linger.
    let relay_path = echobird_dir().join("kilo.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    let dir = kilo_config_dir();
    let mut updated_any = false;

    for path in [dir.join("kilo.jsonc"), dir.join("kilo.json")] {
        if !path.exists() {
            continue;
        }

        let mut config = match read_jsonc_file(&path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!("Failed to parse Kilo Code config: {}", path.display()),
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
            message: "Kilo Code restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "Kilo Code already at defaults - no config file to update.".to_string(),
        }
    }
}

// ════════════════════════════════════════════════════════════════
//  Type 3d: OpenScience - open-source Claude Science alternative.
//  ~/.config/openscience/openscience.json(c)  {provider: {X: {npm, options, models}}}
//  Same models.dev provider schema as OpenCode; model-agnostic (Anthropic +
//  OpenAI both native). Single binary (npm/curl install) - no launcher
//  patch, no relay file; the native config write is the whole mechanism.
//  `openscience_config_path()` picks the highest-precedence file the install
//  already owns (`.jsonc` wins by merge order), so a user who has touched any
//  global setting gets edits applied to the file that actually shadows the
//  rest — never a silent no-op against an out-ranked file.
// ════════════════════════════════════════════════════════════════

// OpenScience's GLOBAL config dir is ~/.config/openscience on every platform:
// it resolves `xdgConfig` from `xdg-basedir`, which has NO Windows APPDATA
// fallback (it joins os.homedir()/.config everywhere), so this path is correct
// on Windows / macOS / Linux alike.
fn openscience_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("openscience")
}

/// OpenScience merges its GLOBAL configs in order `config.json` →
/// `openscience.json` → `openscience.jsonc`, so `openscience.jsonc` has the
/// HIGHEST precedence (loaded last). Its own UI (`globalConfigFile`) defaults
/// to writing `.jsonc`, so a user who has touched any global setting already
/// owns a `.jsonc` that would shadow a plain `.json` write. Target the
/// `.jsonc` when it exists so our keys always take effect; default a fresh
/// install to `openscience.json` (the documented canonical name). Mirrors
/// `mimocode_write_path`. NB: writing a `.jsonc` strips the user's comments
/// (same tradeoff as mimicode) — accepted, because an invisible write is worse.
fn openscience_config_path() -> PathBuf {
    let jsonc = openscience_config_dir().join("openscience.jsonc");
    if jsonc.exists() {
        jsonc
    } else {
        openscience_config_dir().join("openscience.json")
    }
}

fn apply_openscience(model_info: &ModelInfo) -> ApplyResult {
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

    // OpenScience speaks both protocols natively. The chosen protocol picks
    // the AI SDK npm package: Anthropic -> @ai-sdk/anthropic (endpoint from
    // anthropic_url), OpenAI-compatible -> @ai-sdk/openai-compatible (base_url).
    // `endpoint` is always set after this block — the anthropic arm early-
    // returns on an empty URL, the openai arm defaults to api.openai.com — so
    // no Option wrapping is needed downstream.
    let is_anthropic = model_info.protocol.as_deref() == Some("anthropic");
    let endpoint = if is_anthropic {
        match model_info
            .anthropic_url
            .as_deref()
            .or(model_info.base_url.as_deref())
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(|u| u.trim_end_matches('/').to_string())
        {
            Some(u) => u,
            None => {
                return ApplyResult {
                    success: false,
                    message: "Base URL is empty. Pick a model first.".to_string(),
                };
            }
        }
    } else {
        model_info
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/')
            .to_string()
    };
    let npm = if is_anthropic {
        "@ai-sdk/anthropic"
    } else {
        "@ai-sdk/openai-compatible"
    };

    // Local llama-server / vllm proxies need no real key; mirror the dummy
    // used by apply_codex/apply_claudedesktop so an empty key on a loopback
    // endpoint is legitimate instead of a hard failure.
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider = endpoint.contains("127.0.0.1") || endpoint.contains("localhost");
    let api_key = if !raw_api_key.is_empty() {
        raw_api_key.to_string()
    } else if is_local_provider {
        "local-no-auth".to_string()
    } else {
        return ApplyResult {
            success: false,
            message: "API Key is empty, cannot apply OpenScience config.".to_string(),
        };
    };

    let config_path = openscience_config_path();
    let mut config = read_jsonc_file(&config_path).unwrap_or(serde_json::json!({}));

    if config.get("$schema").is_none() {
        config["$schema"] = serde_json::json!("https://syntheticsciences.ai/config.json");
    }
    if !config
        .get("provider")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        config["provider"] = serde_json::json!({});
    }

    let provider_id = "echobird";
    let display_name = model_info.name.as_deref().unwrap_or(model_id);
    // Provider `name` is the PROVIDER's human label in OpenScience's workspace
    // (provider.ts resolves `provider.name ?? providerID`); the model's own
    // name belongs only in models.<id>.name. Pin it to "EchoBird" so it isn't
    // relabeled to whatever model was last applied.
    config["provider"][provider_id] = serde_json::json!({
        "npm": npm,
        "name": "EchoBird",
        "options": {
            "apiKey": api_key,
            "baseURL": endpoint
        },
        "models": {
            model_id: {
                "name": display_name
            }
        }
    });
    // OpenScience-standard active-model selectors (model + small_model for
    // title generation etc.). NB a running `openscience serve` memoizes config
    // in State.create keyed by Instance.directory and only re-reads on restart
    // / dispose, so the new model takes effect the next time the server starts
    // — not on an already-running one.
    config["model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));
    config["small_model"] = serde_json::Value::String(format!("{}/{}", provider_id, model_id));

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] OpenScience config written to {:?}",
                config_path
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for OpenScience (echobird/{}) — start or restart `openscience serve` for the workspace to pick it up.",
                    display_name, model_id
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

fn read_openscience() -> Option<ModelInfo> {
    let config = read_jsonc_file(&openscience_config_path())?;
    let selected = config.get("model")?.as_str()?;
    let (provider_id, model_id) = selected.split_once('/')?;
    if provider_id != "echobird" {
        // Not our provider - show as unconfigured so the user can apply.
        return None;
    }
    let provider = config.pointer("/provider/echobird")?;
    let npm = provider
        .get("npm")
        .and_then(|v| v.as_str())
        .unwrap_or("@ai-sdk/openai-compatible");
    let protocol = if npm == "@ai-sdk/anthropic" {
        "anthropic"
    } else {
        "openai"
    };
    let options = provider.get("options");
    let base_url = options
        .and_then(|o| o.get("baseURL"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let api_key = options
        .and_then(|o| o.get("apiKey"))
        .and_then(|v| v.as_str())
        .map(String::from);
    // Echo back the endpoint via anthropic_url when the provider is
    // Anthropic-shaped so the protocol toggle stays in sync on reload. (Only
    // the anthropic path pays one clone — unavoidable since both fields hold
    // the same value; the common openai path moves base_url with no clone.)
    let anthropic_url = if protocol == "anthropic" {
        base_url.clone()
    } else {
        None
    };

    Some(ModelInfo {
        // Use direct .get() lookups instead of a JSON pointer for the model
        // name: model_id can contain '/' (e.g. "openai/gpt-4o-mini"), and a
        // pointer like "/models/openai/gpt-4o-mini/name" would traverse into a
        // nested path instead of the single "openai/gpt-4o-mini" key.
        name: provider
            .get("models")
            .and_then(|m| m.get(model_id))
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(model_id.to_string())),
        model: Some(model_id.to_string()),
        base_url,
        api_key,
        anthropic_url,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

fn restore_openscience_to_official() -> ApplyResult {
    // Loop BOTH precedence files — OpenScience deep-merges config.json →
    // openscience.json → openscience.jsonc, so a stale `echobird` block left
    // in a lower-precedence file re-emerges after we clean the top one. Mirrors
    // restore_mimocode_to_official / restore_opencode_to_official.
    let dir = openscience_config_dir();
    let mut updated_any = false;

    for path in [dir.join("openscience.jsonc"), dir.join("openscience.json")] {
        if !path.exists() {
            continue;
        }
        let mut config = match read_jsonc_file(&path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!("Failed to parse OpenScience config: {}", path.display()),
                };
            }
        };
        if let Some(provider) = config.get_mut("provider").and_then(|v| v.as_object_mut()) {
            provider.remove("echobird");
            // Prune the now-empty `provider` object: OpenScience's
            // defaultModel() gates every provider through
            // `!cfg.provider || Object.keys(cfg.provider).includes(id)`, so an
            // empty `provider: {}` is truthy yet matches no id → it rejects
            // ALL providers (env keys included) and throws NO_PROVIDER_HINT,
            // bricking the workspace. Drop the key entirely when nothing's left.
            if provider.is_empty() {
                if let Some(obj) = config.as_object_mut() {
                    obj.remove("provider");
                }
            }
        }
        for key in ["model", "small_model"] {
            if config
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.starts_with("echobird/"))
                .unwrap_or(false)
            {
                tool_manager::delete_nested_value(&mut config, key);
            }
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
            message: "OpenScience restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "OpenScience already at defaults - no config file to update.".to_string(),
        }
    }
}

// ════════════════════════════════════════════════════════════════
//  Type 3e: DeepSeek Harness (dsh) — DeepSeek's open-source agent
//  harness (developer preview). Config is split across TWO YAML files:
//    ~/.dsh/settings.yaml      llm-pi-ai.providers.<id> + agent-default-model
//    ~/.dsh/.credentials.yaml  <apiKeyEnv>: sk-...
//  Dual-protocol via the pi-ai adapter (anthropic-messages | openai-completions).
//  Same "write an echobird provider route + active selector" mechanism as
//  OpenScience; the route key (Provider ID) is a fixed lowercase constant.
//  dsh hot-reloads settings.yaml, but EchoBird still restarts the tracked
//  instance on start so a model switch + restart is deterministic.
// ════════════════════════════════════════════════════════════════

const DSH_API_KEY_ENV: &str = "ECHOBIRD_API_KEY";

fn dsh_config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".dsh")
}

fn dsh_settings_path() -> PathBuf {
    dsh_config_dir().join("settings.yaml")
}

fn dsh_credentials_path() -> PathBuf {
    dsh_config_dir().join(".credentials.yaml")
}

fn apply_dsh(model_info: &ModelInfo) -> ApplyResult {
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

    // pi-ai protocol per wire: Anthropic → anthropic-messages, else
    // openai-completions. `endpoint` is always set after this block (mirror
    // apply_openscience: the anthropic arm early-returns on an empty URL).
    let is_anthropic = model_info.protocol.as_deref() == Some("anthropic");
    let api = if is_anthropic {
        "anthropic-messages"
    } else {
        "openai-completions"
    };
    let endpoint = if is_anthropic {
        match model_info
            .anthropic_url
            .as_deref()
            .or(model_info.base_url.as_deref())
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(|u| u.trim_end_matches('/').to_string())
        {
            Some(u) => u,
            None => {
                return ApplyResult {
                    success: false,
                    message: "Base URL is empty. Pick a model first.".to_string(),
                };
            }
        }
    } else {
        model_info
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/')
            .to_string()
    };

    // Local llama-server / vllm proxies need no real key (mirror openscience).
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider = endpoint.contains("127.0.0.1") || endpoint.contains("localhost");
    let api_key = if !raw_api_key.is_empty() {
        raw_api_key.to_string()
    } else if is_local_provider {
        "local-no-auth".to_string()
    } else {
        return ApplyResult {
            success: false,
            message: "API Key is empty, cannot apply DeepSeek Harness config.".to_string(),
        };
    };

    let display_name = model_info.name.as_deref().unwrap_or(model_id);

    // ── settings.yaml: llm-pi-ai.providers.echobird + agent-default-model ──
    let settings_path = dsh_settings_path();
    let mut settings = read_yaml_file(&settings_path).unwrap_or_else(yaml_map);
    {
        let settings_map = yaml_as_map_mut(&mut settings);
        {
            let llm = yaml_child_map(settings_map, "llm-pi-ai");
            let providers = yaml_child_map(llm, "providers");

            let mut route = serde_yaml_ng::Mapping::new();
            route.insert(yaml_str("displayName"), yaml_str("EchoBird"));
            route.insert(yaml_str("apiKeyEnv"), yaml_str(DSH_API_KEY_ENV));
            route.insert(yaml_str("api"), yaml_str(api));
            route.insert(yaml_str("baseURL"), yaml_str(&endpoint));
            let mut model = serde_yaml_ng::Mapping::new();
            model.insert(yaml_str("id"), yaml_str(model_id));
            model.insert(yaml_str("name"), yaml_str(display_name));
            route.insert(
                yaml_str("models"),
                serde_yaml_ng::Value::Sequence(vec![serde_yaml_ng::Value::Mapping(model)]),
            );
            providers.insert(yaml_str("echobird"), serde_yaml_ng::Value::Mapping(route));
        }

        // Active selector — new sessions default to this route/model.
        let mut selector = serde_yaml_ng::Mapping::new();
        selector.insert(yaml_str("provider"), yaml_str("echobird"));
        selector.insert(yaml_str("model"), yaml_str(model_id));
        settings_map.insert(
            yaml_str("agent-default-model"),
            serde_yaml_ng::Value::Mapping(selector),
        );
    }

    // ── .credentials.yaml: ECHOBIRD_API_KEY ──
    let creds_path = dsh_credentials_path();
    let mut creds = read_yaml_file(&creds_path).unwrap_or_else(yaml_map);
    yaml_as_map_mut(&mut creds).insert(yaml_str(DSH_API_KEY_ENV), yaml_str(&api_key));

    if let Err(e) = write_yaml_file(&settings_path, &settings) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }
    if let Err(e) = write_yaml_file(&creds_path, &creds) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }

    log::info!(
        "[ToolConfigManager] DeepSeek Harness config written to {:?} + {:?}",
        settings_path,
        creds_path
    );
    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for DeepSeek Harness (echobird/{}) — start or restart `dsh web` to pick it up.",
            display_name, model_id
        ),
    }
}

fn read_dsh() -> Option<ModelInfo> {
    let settings = read_yaml_file(&dsh_settings_path())?;
    let selector = yaml_get(&settings, "agent-default-model")?;
    if yaml_get(selector, "provider")?.as_str()? != "echobird" {
        // Not our provider - show as unconfigured so the user can apply.
        return None;
    }
    let model_id = yaml_get(selector, "model")?.as_str()?;

    let route = yaml_get(
        yaml_get(yaml_get(&settings, "llm-pi-ai")?, "providers")?,
        "echobird",
    )?;
    let api = yaml_get(route, "api")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-completions");
    let protocol = if api == "anthropic-messages" {
        "anthropic"
    } else {
        "openai"
    };
    let base_url = yaml_get(route, "baseURL")
        .and_then(|v| v.as_str())
        .map(String::from);
    // Display name: read the models list for the entry matching `model_id`.
    let name = yaml_get(route, "models")
        .and_then(|m| m.as_sequence())
        .and_then(|seq| {
            seq.iter()
                .find(|e| yaml_get(e, "id").and_then(|v| v.as_str()) == Some(model_id))
        })
        .and_then(|e| yaml_get(e, "name").and_then(|v| v.as_str()))
        .map(String::from)
        .unwrap_or_else(|| model_id.to_string());

    let anthropic_url = if protocol == "anthropic" {
        base_url.clone()
    } else {
        None
    };

    Some(ModelInfo {
        name: Some(name),
        model: Some(model_id.to_string()),
        base_url,
        api_key: None,
        anthropic_url,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

fn restore_dsh_to_official() -> ApplyResult {
    // settings.yaml: drop the echobird route + the agent-default-model selector.
    let settings_path = dsh_settings_path();
    let mut updated_settings = false;
    if settings_path.exists() {
        let mut settings = match read_yaml_file(&settings_path) {
            Some(s) => s,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!(
                        "Failed to parse DeepSeek Harness config: {}",
                        settings_path.display()
                    ),
                };
            }
        };
        let settings_map = yaml_as_map_mut(&mut settings);
        if let Some(llm) = settings_map
            .get_mut(yaml_str("llm-pi-ai"))
            .and_then(|v| v.as_mapping_mut())
        {
            if let Some(providers) = llm
                .get_mut(yaml_str("providers"))
                .and_then(|v| v.as_mapping_mut())
            {
                if providers.remove(yaml_str("echobird")).is_some() {
                    updated_settings = true;
                }
            }
        }
        let is_echobird = settings_map
            .get(yaml_str("agent-default-model"))
            .and_then(|v| v.as_mapping())
            .and_then(|m| m.get(yaml_str("provider")))
            .and_then(|v| v.as_str())
            == Some("echobird");
        if is_echobird {
            settings_map.remove(yaml_str("agent-default-model"));
            updated_settings = true;
        }
        if updated_settings {
            if let Err(e) = write_yaml_file(&settings_path, &settings) {
                return ApplyResult {
                    success: false,
                    message: e,
                };
            }
        }
    }

    // .credentials.yaml: drop ECHOBIRD_API_KEY.
    let creds_path = dsh_credentials_path();
    let mut updated_creds = false;
    if creds_path.exists() {
        let mut creds = match read_yaml_file(&creds_path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!(
                        "Failed to parse DeepSeek Harness credentials: {}",
                        creds_path.display()
                    ),
                };
            }
        };
        if yaml_as_map_mut(&mut creds)
            .remove(yaml_str(DSH_API_KEY_ENV))
            .is_some()
        {
            updated_creds = true;
        }
        if updated_creds {
            if let Err(e) = write_yaml_file(&creds_path, &creds) {
                return ApplyResult {
                    success: false,
                    message: e,
                };
            }
        }
    }

    if updated_settings || updated_creds {
        ApplyResult {
            success: true,
            message: "DeepSeek Harness restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "DeepSeek Harness already at defaults - no config to update.".to_string(),
        }
    }
}

// ════════════════════════════════════════════════════════════════
//  Type 4a: Aider �?~/.aider.conf.yml (simple YAML key: value)
// ════════════════════════════════════════════════════════════════

// Codex CLI and ChatGPT desktop share ~/.codex/config.toml.

/// Apply our 10 canonical Codex fields surgically — overwrite if
/// present, insert if missing — and return the rewritten content.
/// Preserves everything else in the file (`[projects.*]` trust grants,
/// `[tui.*]` NUX progress, `[plugins.*]` state, comments, hand-edited
/// top-level keys).
///
/// This is the bottom-out for cases where a sibling model-switcher
/// (cc-switch, manual edits, a different tool) rewrote keys we own
/// (`model_provider`, `model`, `wire_api`, `requires_openai_auth`, etc.)
/// to point at a different provider. Without this, a v4.8.x `apply_codex`
/// that only flipped `base_url` would leave the rest of the sibling
/// tool's edits in place and Codex would behave wrong (wrong model id,
/// wrong wire protocol, wrong reasoning effort).
///
/// Used by both:
///   • `apply_codex` (this file) — every model switch
///   • `codex_proxy::config_manager::ensure_canonical_config` — every
///     Codex spawn (pre-launch self-heal)
///
/// `codex_base_url` is the URL Codex will see in config.toml. In Bridge
/// mode this is `http://127.0.0.1:53682/v1`; in Relay and Responses-
/// direct modes it's the real upstream URL (Codex skips our proxy).
/// Write the model id the caller chose — Bridge and Relay sessions pin
/// Codex's `gpt-5.5` display alias (CODEX_DISPLAY_MODEL); a Responses
/// direct-connect session passes the real upstream model id (e.g.
/// `glm-5.2`) so Codex talks to the third party in its own id.
///
/// `context_window` is the real token limit of the selected model. Codex
/// writes it as `model_context_window` and derives
/// `model_auto_compact_token_limit` as 90% of it, so a model whose window
/// is smaller than the historic 1M default is not over-claimed.
pub(crate) fn write_codex_canonical_fields(
    content: &str,
    codex_base_url: &str,
    model: &str,
    context_window: u64,
) -> String {
    // Preserve the input's trailing-newline convention. `toml_write_*`
    // helpers go through `content.lines().collect().join("\n")` which
    // strips trailing newlines; without re-adding it, a canonical-input
    // round-trip would always show as a one-byte diff and trigger
    // pointless rewrites (e.g. ensure_canonical_config flapping from
    // "already-canonical" to "drifted" on every Codex spawn).
    let trailing_nl = content.ends_with('\n');
    let mut c = content.to_string();

    // Top-level string keys.
    c = toml_write_top(&c, "model_provider", CODEX_PROVIDER);
    c = toml_write_top(&c, "model", model);
    c = toml_write_top(&c, "model_reasoning_effort", "high");
    // Evict legacy keys we no longer own. Older EchoBird versions wrote
    // `review_model = "gpt-5.5"`; we stopped writing it (Codex no longer
    // consumes it). But our TOML helpers are update-or-insert — they
    // never delete — so a stale line from an old version survives every
    // switch and self-heal. Under a Responses direct-connect session
    // (model = the real upstream id, e.g. "glm-5.2"; base_url = the real
    // upstream) Codex would still send `gpt-5.5` on its review pass to a
    // gateway that only knows the real id → 4xx. Strip it on every write
    // so the canonical set we write below is the full top-level truth.
    c = toml_delete_top(&c, "review_model");
    // Evict any stale `model_catalog_json` from a previous Responses-direct
    // session. The line is conditional (only written by apply_codex for
    // passthrough + bundled-vendor catalogs), so a plain overwrite-or-insert
    // helper would never remove it after switching back to bridge/relay mode
    // or to a non-catalog vendor — leaving config.toml pointing at a catalog
    // that no longer matches the selected model. apply_codex re-adds it when
    // the catalog applies.
    c = toml_delete_top(&c, "model_catalog_json");
    // Top-level raw (bool, int).
    c = toml_write_top_raw(&c, "disable_response_storage", "true");
    c = toml_write_top_raw(&c, "model_context_window", &context_window.to_string());
    c = toml_write_top_raw(
        &c,
        "model_auto_compact_token_limit",
        &codex_compact_limit_for(context_window).to_string(),
    );

    // [model_providers.OpenAI] string keys.
    let table = format!("model_providers.{}", CODEX_PROVIDER);
    c = toml_write_table_value(&c, &table, "name", CODEX_PROVIDER);
    c = toml_write_table_value(&c, &table, "base_url", codex_base_url);
    c = toml_write_table_value(&c, &table, "wire_api", "responses");
    // [model_providers.OpenAI] raw (bool).
    c = toml_write_table_value_raw(&c, &table, "requires_openai_auth", "true");

    if trailing_nl && !c.ends_with('\n') {
        c.push('\n');
    }
    c
}

/// Whether `content` (a config.toml) references OUR canonical catalog path in
/// `model_catalog_json`. Used by `apply_codex` to decide when to delete the
/// stale `~/.codex/models.json` file after leaving catalog mode. The check is
/// exact-path, not substring: a user's own catalog pointed at via a different
/// path (e.g. MiniMax docs' `~/.codex/model-catalogs/custom-catalog.json`)
/// must NOT trigger deletion of our file. Accepts both the absolute
/// forward-slash form we write and the `~/.codex/models.json` shorthand the
/// vendor docs use.
fn codex_catalog_referenced(content: &str, our_path: &str) -> bool {
    let referenced = toml_read_top(content, "model_catalog_json");
    !referenced.is_empty() && (referenced == our_path || referenced == "~/.codex/models.json")
}

fn apply_codex(tool_id: &str, model_info: &ModelInfo) -> ApplyResult {
    // Two write modes, picked by `model_info.relay_mode`:
    //
    // • Bridge (default): config.toml's base_url is permanently
    //   "http://127.0.0.1:53682/v1" (CODEX_PROXY_PORT). The proxy
    //   reads ~/.echobird/codex.json on every request and forwards
    //   with Responses ↔ Chat translation as needed. Same shape
    //   across model switches, so Codex's runtime state in config.toml
    //   ([projects.*] trust, [tui.*] NUX) survives switches.
    //
    // • Relay (relay_mode = true): config.toml's base_url is the
    //   provider's REAL upstream URL. Codex talks to it directly. Used
    //   for relay stations (cc-vibe.com etc.) that already speak the
    //   Responses protocol — no proxy hop, no translation. The local
    //   proxy stays running but Codex doesn't touch it for this
    //   provider.

    let codex_dir = dirs::home_dir().unwrap_or_default().join(".codex");
    let config_path = codex_dir.join("config.toml");
    let auth_path = codex_dir.join("auth.json");

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

    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string();

    // Reject ONLY the codex_proxy's own port (53682). Applying that as the
    // upstream would make the proxy forward every request back to itself —
    // an infinite loop. Other 127.0.0.1 ports are legitimate upstreams
    // (most importantly 127.0.0.1:11434, EchoBird's local-LLM proxy that
    // sits in front of llama-server), so the broader "no localhost"
    // blanket-ban previously here was too aggressive and made Codex +
    // local-LLM combinations impossible.
    if base_url.contains(":53682") {
        return ApplyResult {
            success: false,
            message: "Cannot use EchoBird's own Codex proxy (127.0.0.1:53682) as the provider — that would create a forwarding loop. Pick a real provider, or use the local LLM endpoint (127.0.0.1:11434).".to_string(),
        };
    }

    // For local-LLM endpoints (127.0.0.1 / localhost — but NOT our own
    // codex_proxy port 53682, which we already rejected above), llama-server
    // ignores the API key entirely. Codex CLI, on the other hand, refuses to
    // start when OPENAI_API_KEY is empty. Substitute a non-empty dummy so
    // users don't have to invent a fake key in the Model Center just to use
    // their own local model.
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider = base_url.contains("127.0.0.1") || base_url.contains("localhost");
    let api_key = if raw_api_key.is_empty() {
        if is_local_provider {
            "local-no-auth"
        } else {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Codex config".to_string(),
            };
        }
    } else {
        raw_api_key
    };

    // Resolve the real context window for the selected model so Codex writes
    // `model_context_window` / `model_auto_compact_token_limit` matching the
    // model's actual token budget rather than the historic 1M default.
    let context_window = model_context_window_for(model_id);

    // Resolve the URL + model id Codex itself will see in its config.toml.
    // Three routing modes:
    //   • Bridge (default): base_url = our proxy port; model = "gpt-5.5"
    //     display alias. The proxy reads ~/.echobird/codex.json per request
    //     and forwards with Responses ↔ Chat translation as needed.
    //   • Relay (relay_mode): base_url = real upstream; model = "gpt-5.5"
    //     display alias (relay stations accept the alias and map it
    //     themselves — keep the model-id deception moat). Codex skips the
    //     proxy entirely.
    //   • Responses direct (responses_passthrough): base_url = real
    //     upstream; model = the REAL upstream model id (e.g. "glm-5.2").
    //     For third parties that natively speak the Responses protocol —
    //     Codex connects straight to them, no proxy hop, no translation,
    //     no id spoof. The two direct modes are mutually exclusive (UI
    //     auto-flips), so we force passthrough off when relay is on.
    let relay_mode = model_info.relay_mode.unwrap_or(false);
    let responses_passthrough = !relay_mode && model_info.responses_passthrough.unwrap_or(false);
    let direct_mode = relay_mode || responses_passthrough;
    let proxy_base_url = format!("http://127.0.0.1:{}/v1", CODEX_PROXY_PORT);
    let codex_base_url = if direct_mode {
        base_url.clone()
    } else {
        proxy_base_url.clone()
    };
    // Direct Responses connect needs the real model id; every other mode
    // pins Codex's "gpt-5.5" display alias.
    let codex_model = if responses_passthrough {
        model_id
    } else {
        CODEX_DISPLAY_MODEL
    };

    ensure_parent(&config_path);

    // Canonicalize ALL 10 fields we own, every time. Overwrite-in-place
    // if present, insert if missing. This is the bottom-out for sibling
    // model-switchers (cc-switch, manual edits, etc.) that may have
    // rewritten our keys to point at a different provider — we restore
    // canonical shape end-to-end, not just `base_url`. Codex's own
    // runtime state (`[projects.*]` trust, `[tui.*]` NUX, `[plugins.*]`)
    // and any unrelated user-edited top-level keys stay untouched.
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let mut new_content =
        write_codex_canonical_fields(&existing, &codex_base_url, codex_model, context_window);

    // web_search: user toggle. `Some(false)` → "disabled" (Codex removes
    // its built-in search tool); ON → "live" — unrestricted live retrieval,
    // i.e. actual real-time web search. NOT Codex's default "cached": that
    // uses an OpenAI-maintained index with NO external web access, which is
    // meaningless for our third-party upstreams (OpenAI's index doesn't
    // cover them) and isn't "web search on" as a user expects the toggle to
    // mean. Written here (not in write_codex_canonical_fields) so the
    // pre-spawn self-heal leaves the user's choice untouched.
    let web_search_value = if model_info.web_search == Some(false) {
        "disabled"
    } else {
        "live"
    };
    new_content = toml_write_top(&new_content, "web_search", web_search_value);

    // Model catalog — Responses-direct third parties (DeepSeek / MiniMax /
    // MiMo) need `model_catalog_json` so Codex knows the real model's
    // context window, reasoning levels, and tool capabilities. Only written
    // in passthrough mode (Codex talks to the upstream directly with the
    // real model id); bridge + relay keep the display-alias shape and get no
    // catalog line. Vendors without a bundled catalog keep today's behavior.
    // The stale line is evicted by `write_codex_canonical_fields` on every
    // canonicalize, so switching away from passthrough (or to a non-catalog
    // vendor) can't leave a dangling pointer.
    let catalog_template = if responses_passthrough {
        codex_catalog::template_for_url(&base_url)
    } else {
        None
    };
    if let Some(template_str) = catalog_template {
        let catalog_path = codex_catalog::models_json_path();
        let template = serde_json::from_str(template_str).unwrap_or_default();
        // Stamp the SELECTED model onto the vendor capability template and
        // write a single-entry catalog — we never enumerate a vendor's model
        // versions, so `deepseek-v5-flash` / `mimo-v2.6` need no bundled
        // asset change.
        let catalog = codex_catalog::build_catalog(
            &template,
            model_id,
            model_info.name.as_deref().unwrap_or(model_id),
            context_window,
        );
        // Only add the config line if the file write succeeded — a dangling
        // model_catalog_json pointing at a missing file makes Codex error on
        // startup.
        if write_json_file(&catalog_path, &catalog).is_ok() {
            new_content = toml_write_top(
                &new_content,
                "model_catalog_json",
                &catalog_path.to_string_lossy(),
            );
        }
    } else {
        // Leaving catalog mode (passthrough off, or a non-bundled vendor):
        // the canonical write evicted the `model_catalog_json` line, so Codex
        // no longer reads the file. Delete the stale file at OUR canonical
        // path so the switch is disk-clean too — but ONLY when the previous
        // config actually referenced OUR canonical path. A user's own catalog
        // pointed at a different path (e.g. MiniMax docs' custom-catalog.json)
        // must not trigger deletion of our file, and a config that never
        // mentioned a catalog must leave whatever's on disk alone.
        let catalog_path = codex_catalog::models_json_path();
        if codex_catalog_referenced(&existing, &catalog_path.to_string_lossy())
            && catalog_path.exists()
        {
            let _ = fs::remove_file(&catalog_path);
        }
    }

    // Only write if content actually changed — avoids touching mtime
    // for no-op applies and avoids unnecessary fs traffic.
    if new_content != existing {
        if let Err(e) = fs::write(&config_path, &new_content) {
            return ApplyResult {
                success: false,
                message: format!("Codex config error: {}", e),
            };
        }
    }

    // Back up any existing auth.json (OAuth-token sign-ins, prior api-key
    // configs, etc.) before overwriting so restore-to-official can put it
    // back. We keep one snapshot per session — apply_codex called multiple
    // times in a row preserves the FIRST snapshot, not the most recent
    // (which would clobber the original OAuth state with our apikey state).
    let auth_backup_path = echobird_dir().join("codex-auth.bak.json");
    if auth_path.exists() && !auth_backup_path.exists() {
        if let Ok(existing) = fs::read(&auth_path) {
            ensure_parent(&auth_backup_path);
            let _ = fs::write(&auth_backup_path, existing);
        }
    }

    // Write the api-key auth.json that Codex v0.130+ expects.
    let auth_payload = serde_json::json!({ "OPENAI_API_KEY": api_key });
    ensure_parent(&auth_path);
    if let Err(e) = fs::write(
        &auth_path,
        serde_json::to_string_pretty(&auth_payload).unwrap_or_default(),
    ) {
        return ApplyResult {
            success: false,
            message: format!("Codex auth.json error: {}", e),
        };
    }

    // The live relay — in Bridge mode the proxy reads this on every
    // request, so it must reflect the upstream we want forwarded to.
    // In Relay mode the proxy is bypassed for Codex, but we still
    // write the same file so `read_codex` (used by the model-picker
    // UI to round-trip the current selection) and ensure_canonical_config
    // (which reads `relayMode` to decide whether to self-heal config.toml)
    // both see consistent state.
    let relay_path = echobird_dir().join("codex.json");
    let relay = serde_json::json!({
        "apiKey": api_key,
        "baseUrl": base_url,
        "displayModel": CODEX_DISPLAY_MODEL,
        "actualModel": model_id,
        "modelName": model_info.name.as_deref().unwrap_or(model_id),
        "providerId": CODEX_PROVIDER,
        "relayMode": relay_mode,
        "responsesPassthrough": responses_passthrough,
        "contextWindow": context_window,
    });
    let _ = write_json_file(&relay_path, &relay);

    let display = if tool_id == "chatgptdesktop" {
        "ChatGPT"
    } else {
        "Codex CLI"
    };
    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for {}.",
            model_info.name.as_deref().unwrap_or(model_id),
            display
        ),
    }
}

fn read_codex() -> Option<ModelInfo> {
    let codex_dir = dirs::home_dir()?.join(".codex");
    let content = fs::read_to_string(codex_dir.join("config.toml")).ok()?;

    let model_from_toml = toml_read_top(&content, "model");
    if model_from_toml.is_empty() {
        return None;
    }

    let provider_id = toml_read_top(&content, "model_provider");
    let mut base_url = if provider_id.is_empty() {
        None
    } else {
        let value = toml_read_table_value(
            &content,
            &format!("model_providers.{}", provider_id),
            "base_url",
        );
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    };

    // If base_url is EchoBird's own codex_proxy (port 53682), read the real
    // provider URL from the relay file for UI display. The launcher rewrites
    // config.toml to point at 127.0.0.1:53682 while running, but the UI should
    // show users the actual provider they configured (e.g., api.xiaomimimo.com).
    // This does NOT affect Codex's runtime behavior — Codex always reads from
    // config.toml, which the launcher controls. Matching ONLY :53682 (not all
    // 127.0.0.1) means a legitimate local-LLM endpoint such as
    // 127.0.0.1:11434 stays visible as-is — that IS the real provider.
    if let Some(ref url) = base_url {
        if url.contains(":53682") {
            base_url = read_codex_relay_base_url();
        }
    }

    // When we wrote the canonical OpenAI provider, config.toml's `model`
    // is the display alias ("gpt-5.5"), not the real third-party model.
    // The UI needs the real one to round-trip a meaningful selection back
    // to the user — read it from the relay file. Fall back to the
    // config.toml value for non-canonical setups (e.g., user manually
    // edited their config to point at a different provider).
    let model = if provider_id == CODEX_PROVIDER {
        read_codex_relay_model().unwrap_or(model_from_toml)
    } else {
        model_from_toml
    };

    // API key now lives in ~/.codex/auth.json (preferred_auth_method=apikey).
    // Fall back to the legacy env_key path for configs written before this change.
    let api_key = read_codex_auth_key(&codex_dir).or_else(|| {
        let env_key = if provider_id.is_empty() {
            String::new()
        } else {
            toml_read_table_value(
                &content,
                &format!("model_providers.{}", provider_id),
                "env_key",
            )
        };
        if env_key.is_empty() {
            None
        } else {
            std::env::var(&env_key).ok()
        }
    });

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

fn read_codex_relay_base_url() -> Option<String> {
    let relay_path = echobird_dir().join("codex.json");
    let content = fs::read_to_string(relay_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("baseUrl").and_then(|x| x.as_str()).map(String::from)
}

fn read_codex_relay_model() -> Option<String> {
    let relay_path = echobird_dir().join("codex.json");
    let content = fs::read_to_string(relay_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("actualModel")
        .or_else(|| v.get("modelName"))
        .and_then(|x| x.as_str())
        .map(String::from)
}

fn read_codex_auth_key(codex_dir: &Path) -> Option<String> {
    let content = fs::read_to_string(codex_dir.join("auth.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("OPENAI_API_KEY")
        .and_then(|x| x.as_str())
        .map(String::from)
}

fn restore_codex_to_official(tool_id: &str, config_path: &Path) -> ApplyResult {
    // Full-file overwrite. ChatGPT's model picker shows the
    // `model_provider` id VERBATIM, so the built-in lowercase id
    // "openai" rendered as a lowercase "openai" chip — inconsistent with
    // the third-party path, which uses a capitalized "OpenAI" provider.
    // Point at a capitalized "OpenAI" provider so the chip casing
    // matches everywhere.
    //
    // We deliberately do NOT set `base_url`. Codex's `to_api_provider`
    // resolves an unset base_url from the AUTH MODE — chatgpt.com/
    // backend-api/codex for a ChatGPT login, api.openai.com otherwise —
    // which is exactly the built-in openai behavior that keeps
    // ChatGPT-account users working. `requires_openai_auth = true`
    // reproduces the built-in's login-screen / auth.json handling. (A
    // table keyed lowercase "openai" would be silently dropped: Codex's
    // merge does `entry(key).or_insert`, and the built-in already owns
    // that key — only a new "OpenAI" key takes effect.)
    //
    // We also write no `model` line — pinning one (we used to pin
    // "gpt-4o") breaks ChatGPT-account users because OpenAI rejects
    // gpt-4o for that auth path. Without it Codex selects an
    // auth-appropriate default (gpt-5-codex for ChatGPT, otherwise its
    // built-in default). Codex regenerates everything else
    // (projects/marketplaces/tui state) on next launch.
    let content = "model_provider = \"OpenAI\"\n\
                   \n\
                   [model_providers.OpenAI]\n\
                   name = \"OpenAI\"\n\
                   wire_api = \"responses\"\n\
                   requires_openai_auth = true\n";

    ensure_parent(config_path);
    match fs::write(config_path, content) {
        Ok(_) => {
            // Restore auth.json from our backup if we have one (OAuth
            // tokens, prior api-key from before the third-party detour).
            //
            // We do NOT delete auth.json when no backup exists. The old
            // behavior was "fall through to Codex's own login flow",
            // but deleting auth.json out from under a running Codex
            // process produces a worse failure mode: the in-memory
            // React state and the on-disk auth become inconsistent,
            // which surfaces as a "fake account"
            // displayed in the Codex sidebar and silently partitions
            // any chats the user created during the third-party
            // session into an inaccessible namespace. Leaving the
            // third-party apikey in place at worst causes a 401 on
            // next Codex request, which Codex handles loudly via its
            // own re-login UI — better than silent data loss. Users
            // who actually want to log out should use Codex's own
            // logout button.
            let auth_path = config_path
                .parent()
                .unwrap_or(Path::new(""))
                .join("auth.json");
            let auth_backup_path = echobird_dir().join("codex-auth.bak.json");
            if auth_backup_path.exists() {
                if let Ok(bak) = fs::read(&auth_backup_path) {
                    let _ = fs::write(&auth_path, bak);
                    let _ = fs::remove_file(&auth_backup_path);
                }
            }

            let relay_path = echobird_dir().join("codex.json");
            if relay_path.exists() {
                let _ = fs::remove_file(&relay_path);
            }
            ApplyResult {
                success: true,
                message: format!(
                    "{} restored to OpenAI official provider.",
                    if tool_id == "chatgptdesktop" {
                        "ChatGPT"
                    } else {
                        "Codex CLI"
                    }
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: format!("Failed to restore Codex config: {}", e),
        },
    }
}

// ════════════════════════════════════════════════════════════════
//  Claude Desktop — 3P provider profile writer.
//
// Claude Desktop has an officially-supported "third-party profile"
// (3P) mechanism that's completely separate from Claude Code's
// ~/.claude/settings.json. Setting `deploymentMode = "3p"` in
// claude_desktop_config.json + writing a profile JSON to
// Claude-3p/configLibrary/ tells Desktop to route /v1/messages
// traffic to a custom inferenceGatewayBaseUrl instead of Anthropic.
//
// We only support providers that natively speak the Anthropic
// Messages API (i.e. `model.anthropicUrl` is set in
// modelDirectory.json — DeepSeek, GLM, Kimi, Qwen, MiniMax,
// Xiaomi, WorldRouter, Anthropic itself). Non-Anthropic providers
// would need a translation proxy, which we deliberately skip —
// the AppManager UI filters out incompatible models via the
// existing apiProtocol/anthropicUrl gate.
// ════════════════════════════════════════════════════════════════

const CLAUDE_DESKTOP_PROFILE_ID: &str = "7d8f4e2a-9c3b-4f1a-b0e5-1a2b3c4d5e6f";
const CLAUDE_DESKTOP_PROFILE_NAME: &str = "EchoBird";

struct ClaudeDesktopLayout {
    cfg_official: PathBuf,
    cfg_threep: PathBuf,
    lib_dir: PathBuf,
}

#[cfg(any(target_os = "macos", windows))]
fn resolve_claudedesktop_paths() -> Option<ClaudeDesktopLayout> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    let (official_dir, threep_dir) = {
        let app_support = home.join("Library").join("Application Support");
        (app_support.join("Claude"), app_support.join("Claude-3p"))
    };

    #[cfg(windows)]
    let (official_dir, threep_dir) = {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        (local.join("Claude"), local.join("Claude-3p"))
    };

    Some(ClaudeDesktopLayout {
        cfg_official: official_dir.join("claude_desktop_config.json"),
        cfg_threep: threep_dir.join("claude_desktop_config.json"),
        lib_dir: threep_dir.join("configLibrary"),
    })
}

#[cfg(not(any(target_os = "macos", windows)))]
fn resolve_claudedesktop_paths() -> Option<ClaudeDesktopLayout> {
    None
}

/// Flip the `deploymentMode` key on one of Claude Desktop's top-level
/// config files. Preserves every other key the user (or Desktop itself)
/// has stashed there — `mcpServers`, telemetry opt-outs, window size, etc.
fn set_claude_deployment_mode(path: &Path, mode: &str) -> Result<(), String> {
    let mut cfg = read_json_file(path).unwrap_or_else(|| serde_json::json!({}));
    if !cfg.is_object() {
        cfg = serde_json::json!({});
    }
    if let Some(obj) = cfg.as_object_mut() {
        obj.insert(
            "deploymentMode".to_string(),
            serde_json::Value::String(mode.to_string()),
        );
    }
    write_json_file(path, &cfg)
}

fn apply_claudedesktop(model_info: &ModelInfo) -> ApplyResult {
    let paths = match resolve_claudedesktop_paths() {
        Some(p) => p,
        None => {
            return ApplyResult {
                success: false,
                message: "Claude Desktop is only supported on macOS and Windows.".to_string(),
            };
        }
    };

    // The frontend's AppManagerProvider.applyModel collapses the chosen
    // protocol's URL into `base_url` when sending to the backend (the
    // long-standing claudecode convention — see line `const apiUrl =
    // useAnthropicUrl ? model.anthropicUrl! : model.baseUrl`). For
    // claudedesktop the picked model's Anthropic endpoint will arrive
    // in `base_url` with `protocol == "anthropic"`. Accept either field
    // so we work today and stay compatible if the frontend ever sends
    // `anthropic_url` explicitly. The AppManager protocol filter is the
    // real gate that ensures the URL is actually Anthropic-shaped.
    let anthropic_url = model_info
        .anthropic_url
        .as_deref()
        .or(model_info.base_url.as_deref())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(|u| u.trim_end_matches('/').to_string());
    let anthropic_url = match anthropic_url {
        Some(u) => u,
        None => {
            return ApplyResult {
                success: false,
                message: "Base URL is empty. Pick a model first.".to_string(),
            };
        }
    };

    // Mirror the local-provider fallback in `apply_codex` (line 1153):
    // local LLM proxies (llama.cpp / vllm / sglang under our unified
    // proxy) don't require an API key, so an empty user-supplied key
    // is legitimate when the upstream is loopback. Substitute a
    // non-empty sentinel so anthropic_proxy still has something to
    // forward as Bearer / x-api-key (the local server ignores it).
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider =
        anthropic_url.contains("127.0.0.1") || anthropic_url.contains("localhost");
    let api_key = if raw_api_key.is_empty() {
        if is_local_provider {
            "local-no-auth".to_string()
        } else {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Claude Desktop config.".to_string(),
            };
        }
    } else {
        raw_api_key.to_string()
    };

    let real_model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("");

    if let Err(e) = set_claude_deployment_mode(&paths.cfg_official, "3p") {
        return ApplyResult {
            success: false,
            message: format!("Failed to set Claude config to 3p mode: {}", e),
        };
    }
    if let Err(e) = set_claude_deployment_mode(&paths.cfg_threep, "3p") {
        return ApplyResult {
            success: false,
            message: format!("Failed to set Claude-3p config to 3p mode: {}", e),
        };
    }

    let _ = fs::create_dir_all(&paths.lib_dir);

    // Two routing modes, picked by `model_info.relay_mode`:
    //
    // • Bridge (default): Desktop's gateway hits our local Anthropic
    //   proxy on `127.0.0.1:CODEX_PROXY_PORT/v1/messages`. The proxy
    //   reads the real upstream URL, API key, and model id fresh from
    //   ~/.echobird/claudedesktop.json on every request, rewrites the
    //   Anthropic-only model id (Desktop hardcodes claude-sonnet-4-*
    //   etc.) to whatever the EchoBird user actually picked, and
    //   forwards. The `inferenceGatewayApiKey` is a non-empty sentinel
    //   — Desktop refuses an empty value but the proxy ignores what
    //   Desktop sends here, using the real key from the relay file.
    //
    // • Relay (relay_mode = true): Desktop's gateway hits the upstream
    //   directly. We write the real URL + real API key into the
    //   profile JSON; the proxy is bypassed for /v1/messages. Used
    //   for relay stations (cc-vibe.com etc.) that natively serve
    //   Anthropic Messages and do their own model-id mapping. Caveat:
    //   model-id rewrite is lost, so the upstream sees whatever id
    //   Desktop chose (claude-sonnet-4-*, …) — fine for stations that
    //   accept those, broken for raw Chat-only providers.
    let proxy_base = format!("http://127.0.0.1:{}", CODEX_PROXY_PORT);
    let relay_mode = model_info.relay_mode.unwrap_or(false);
    let (gateway_base_url, gateway_api_key) = if relay_mode {
        (anthropic_url.clone(), api_key.clone())
    } else {
        (proxy_base.clone(), "echobird-local-proxy".to_string())
    };

    let profile_path = paths
        .lib_dir
        .join(format!("{}.json", CLAUDE_DESKTOP_PROFILE_ID));
    // Profile body — Desktop's gateway reads these on launch. The profile's
    // display name is NOT a field of this object; it belongs in _meta.json
    // entries[].name (Desktop's profile picker reads it from there).
    //
    // `inferenceModels` populates Desktop's in-app model picker directly
    // and, crucially, BYPASSES Desktop's `/v1/models` discovery probe.
    // Without this field, Desktop 1.7.x falls back to GET /v1/models on
    // our gateway (which we don't implement) and surfaces a
    // "Gateway returned an error" banner on every fresh install. We
    // write a single entry whose `name` is the id Desktop sends on
    // `/v1/messages`. In bridge mode that's the canonical claude-* id
    // (messages_handler rewrites it to the real upstream model); in relay
    // mode Desktop hits the upstream directly with no rewrite, so `name`
    // must instead carry the real upstream id (computed below). `labelOverride` is
    // the upstream model id (`real_model_id`, source = model_info.model
    // with fallback to model_info.name) — NOT the user's editable card
    // display name, which can be anything ("deepseek你好" etc.) and
    // would surface garbage in Desktop's picker. cc-switch surfaces the
    // upstream id here too.
    // Bridge mode rewrites the id downstream, so the canonical claude-opus-5
    // is correct (and clears Desktop's Claude-name filter). Relay mode bypasses
    // the proxy — Desktop talks to the upstream directly — so the real upstream
    // id (e.g. "fable-5") must be sent as-is, or the station receives a model
    // name it never advertised. Fall back to the canonical id when no real id
    // is known, so we never emit an empty name.
    let base_model_id = if relay_mode && !real_model_id.is_empty() {
        real_model_id
    } else {
        "claude-opus-5"
    };
    // 1M context: Claude Desktop expresses the long-context variant via a
    // `supports1m: true` flag on the model entry — NOT a `[1m]` name suffix
    // (Desktop's profile schema rejects the suffix). This flag is exactly what
    // Desktop's "Offer 1M-context variant" UI toggle sets; the name stays the
    // plain id. Set in BOTH routing modes. In bridge mode the entry name is
    // the canonical claude-* id and every request passes our proxy, which
    // strips any `[1m]` the client attaches. In relay mode Desktop talks to
    // the third-party upstream directly with the real model id; but Desktop's
    // 1M variant is itself a secondary, user-selected pick inside Desktop, so
    // `<real-id>[1m]` only reaches the upstream when the user explicitly
    // chooses it there (mirroring Desktop's native behavior). We just
    // advertise support; whether to actually send `[1m]` is the user's call.
    //
    // We deliberately do NOT set `anthropicFamilyTier` / `isFamilyDefault`.
    // They were added to try to make the 1M variant the default, which turned
    // out to be an unfixable Desktop-side bug. Worse, hardcoding a tier
    // mislabels the model in relay mode: there the name is the real upstream id
    // (e.g. a Sonnet model), so tagging it "opus" makes Desktop's opus alias
    // resolve to a Sonnet model. The tier alias is optional — Desktop keeps the
    // model usable without it.
    let mut model_entry = serde_json::Map::new();
    model_entry.insert(
        "name".to_string(),
        serde_json::Value::String(base_model_id.to_string()),
    );
    model_entry.insert(
        "labelOverride".to_string(),
        serde_json::Value::String(real_model_id.to_string()),
    );
    model_entry.insert("supports1m".to_string(), serde_json::Value::Bool(true));
    let model_entry = serde_json::Value::Object(model_entry);
    let profile = serde_json::json!({
        "disableDeploymentModeChooser": true,
        "inferenceGatewayApiKey": gateway_api_key,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": gateway_base_url,
        "inferenceModels": [model_entry],
        "inferenceProvider": "gateway",
        // Claude Desktop 3P mode derives the built-in web_fetch egress policy
        // from this field. Without it, egress falls back to the gateway host
        // only (127.0.0.1), so web_fetch can't reach external URLs. ["*"]
        // lets web_fetch fetch any URL the user asks Claude to fetch.
        "coworkEgressAllowedHosts": ["*"],
    });
    if let Err(e) = write_json_file(&profile_path, &profile) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Desktop profile: {}", e),
        };
    }

    // Meta — Desktop reads `appliedId` to know which profile is active,
    // and `entries[]` to populate the in-app profile picker. Without the
    // entries array the picker won't render our profile as named.
    let meta_path = paths.lib_dir.join("_meta.json");
    let meta = serde_json::json!({
        "appliedId": CLAUDE_DESKTOP_PROFILE_ID,
        "entries": [
            {
                "id": CLAUDE_DESKTOP_PROFILE_ID,
                "name": CLAUDE_DESKTOP_PROFILE_NAME,
            }
        ],
    });
    let _ = write_json_file(&meta_path, &meta);

    // Relay — the real upstream config that anthropic_proxy reads on
    // every /v1/messages request. Updates here take effect on the next
    // request without restarting anything.
    // Relay file always reflects the real upstream config (even in Relay
    // mode where the proxy is bypassed for the network path), so `read_
    // claudedesktop` can round-trip the active model back to the UI
    // and so future debug tooling has a consistent source of truth.
    let relay_path = echobird_dir().join("claudedesktop.json");
    let relay = serde_json::json!({
        "baseUrl": anthropic_url,
        "apiKey": api_key,
        "actualModel": real_model_id,
        "modelName": model_info.name.as_deref().unwrap_or(real_model_id),
        "relayMode": relay_mode,
    });
    if let Err(e) = write_json_file(&relay_path, &relay) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Desktop relay file: {}", e),
        };
    }

    ApplyResult {
        success: true,
        message:
            "Claude Desktop configured. Please fully quit and reopen Claude Desktop for the change to take effect."
                .to_string(),
    }
}

fn restore_claudedesktop_to_official() -> ApplyResult {
    let paths = match resolve_claudedesktop_paths() {
        Some(p) => p,
        None => {
            return ApplyResult {
                success: true,
                message: "Claude Desktop is not supported on this OS — nothing to restore."
                    .to_string(),
            };
        }
    };

    let _ = set_claude_deployment_mode(&paths.cfg_official, "1p");
    let _ = set_claude_deployment_mode(&paths.cfg_threep, "1p");

    let profile_path = paths
        .lib_dir
        .join(format!("{}.json", CLAUDE_DESKTOP_PROFILE_ID));
    if profile_path.exists() {
        let _ = fs::remove_file(&profile_path);
    }

    let meta_path = paths.lib_dir.join("_meta.json");
    if meta_path.exists() {
        let _ = fs::remove_file(&meta_path);
    }

    // Drop the relay file so anthropic_proxy stops accepting requests
    // for the previously-applied provider. (The proxy keeps running and
    // returns 503 on a missing relay — graceful no-op for any stale
    // request still in flight.)
    let relay_path = echobird_dir().join("claudedesktop.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    ApplyResult {
        success: true,
        message:
            "Claude Desktop restored to official provider. Please fully quit and reopen Claude Desktop."
                .to_string(),
    }
}

fn read_claudedesktop() -> Option<ModelInfo> {
    // Source of truth is the relay file, NOT Claude Desktop's profile
    // JSON. The profile only holds the proxy URL (127.0.0.1:53682) and
    // a sentinel key; the real upstream URL / key / model id live in
    // ~/.echobird/claudedesktop.json so apply_claudedesktop can update
    // them without touching Desktop's config and triggering a restart.
    let relay_path = echobird_dir().join("claudedesktop.json");
    let relay = read_json_file(&relay_path)?;

    let anthropic_url = relay
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let api_key = relay
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Some(ModelInfo {
        name: None,
        model: None,
        base_url: None,
        api_key,
        anthropic_url: Some(anthropic_url),
        protocol: Some("anthropic".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

// Model-tier env vars Claude Code consults. Bridge mode clears them so Claude
// Code falls back to its built-in claude-* ids (the proxy rewrites them);
// relay mode pins them all to the real upstream model. Hoisted to module
// scope so the set is lockable by tests — the apply path writes to
// ~/.claude/settings.json via dirs::home_dir(), which isn't injectable.
//
// `ANTHROPIC_SMALL_FAST_MODEL` is deliberately NOT here — Claude Code
// deprecated it in favor of `ANTHROPIC_DEFAULT_HAIKU_MODEL` (still pinned).
// `CLAUDE_CODE_SUBAGENT_MODEL` IS here so relay mode pins subagents to the
// upstream model too (otherwise subagents fall back to claude-* ids and
// bypass the third-party router, breaking the "全量" write-in).
// https://code.claude.com/docs/en/model-config
const CLAUDECODE_MODEL_VARS: [&str; 6] = [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
];

/// The 1M-capable subset of [`CLAUDECODE_MODEL_VARS`]: env vars that receive
/// a `[1m]` suffix when the user opts into the 1M context window. `HAIKU` and
/// `CLAUDE_CODE_SUBAGENT_MODEL` are deliberately excluded — Haiku has no 1M
/// tier, and subagents pin to the bare upstream id.
const CLAUDECODE_MODEL_VARS_1M: &[&str] = &[
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
];

/// Value written to a `CLAUDECODE_MODEL_VARS` entry in relay mode. Appends
/// `[1m]` only when the user opted in (`one_m`) AND `var` is in the 1M-capable
/// tier; `ANTHROPIC_DEFAULT_HAIKU_MODEL` / `CLAUDE_CODE_SUBAGENT_MODEL` always
/// return the bare id. Bridge mode never calls this — it removes the vars
/// instead of writing them, so the `relay` flag is the caller's responsibility.
fn claudecode_env_model_id(real_id: &str, var: &str, one_m: bool) -> String {
    if one_m && CLAUDECODE_MODEL_VARS_1M.contains(&var) {
        format!("{}[1m]", real_id)
    } else {
        real_id.to_string()
    }
}

/// Claude Code 3P config. Mirrors `apply_claudedesktop` but targets Claude
/// Code's env-var config (~/.claude/settings.json) instead of Desktop's
/// profile JSON. Two routing modes, picked by `model_info.relay_mode`:
///
/// • Bridge (default): point ANTHROPIC_BASE_URL at our local Anthropic proxy
///   (127.0.0.1:<port>/claudecode) and write NO model id, so Claude Code
///   sends its built-in claude-* ids and the proxy rewrites every request to
///   the real upstream model (read fresh from ~/.echobird/claudecode.json).
///   This is the "first-class citizen" path — strict upstreams never see a
///   claude-* name they'd reject, matching how Claude Desktop already works.
///
/// • Relay (relay_mode = true): write the real upstream URL + key + model id
///   straight into settings.json so Claude Code talks to the upstream
///   directly, bypassing the proxy. For relay stations that natively serve
///   Anthropic Messages and map the ids themselves. (≈ the legacy full-write.)
///
/// The relay side-channel (~/.echobird/claudecode.json) is written in BOTH
/// modes so `read_claudecode` can round-trip the active model back to the UI.
fn apply_claudecode(model_info: &ModelInfo) -> ApplyResult {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            return ApplyResult {
                success: false,
                message: "Cannot resolve home directory.".to_string(),
            }
        }
    };
    let settings_path = home.join(".claude").join("settings.json");

    // Frontend collapses the chosen protocol's URL into base_url (same
    // convention as apply_claudedesktop). Accept either field.
    let anthropic_url = model_info
        .anthropic_url
        .as_deref()
        .or(model_info.base_url.as_deref())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(|u| {
            // Trim a trailing '/' and a trailing '/v1' so relay mode (where
            // Claude Code's SDK appends '/v1/messages' to ANTHROPIC_BASE_URL)
            // can't produce a doubled '/v1/v1/messages'. normalize already does
            // this for base_url; do it here too so the anthropic_url branch is
            // equally safe. Bridge mode is immune either way (the proxy
            // recomposes the path), but this keeps both modes consistent.
            let u = u.trim_end_matches('/');
            u.strip_suffix("/v1")
                .unwrap_or(u)
                .trim_end_matches('/')
                .to_string()
        });
    let anthropic_url = match anthropic_url {
        Some(u) => u,
        None => {
            return ApplyResult {
                success: false,
                message: "Base URL is empty. Pick a model first.".to_string(),
            }
        }
    };

    // Local providers (loopback) legitimately have no key — substitute a
    // sentinel so the relay always has something to forward as Bearer.
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider =
        anthropic_url.contains("127.0.0.1") || anthropic_url.contains("localhost");
    let api_key = if raw_api_key.is_empty() {
        if is_local_provider {
            "local-no-auth".to_string()
        } else {
            return ApplyResult {
                success: false,
                message: "API Key is empty, cannot apply Claude Code config.".to_string(),
            };
        }
    } else {
        raw_api_key.to_string()
    };

    let real_model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();

    let relay_mode = model_info.relay_mode.unwrap_or(false);
    let proxy_base = format!("http://127.0.0.1:{}/claudecode", CODEX_PROXY_PORT);

    // ── settings.json env block (preserve every other key the user has) ──
    // Only start from a fresh object when the file genuinely does NOT exist. If
    // it exists but won't parse as a JSON object (hand-edited typo, half-written
    // by another tool), ABORT — defaulting to {} here and writing back would
    // wipe the user's allowedTools / permissions / hooks / MCP config.
    let mut config = if settings_path.exists() {
        match read_json_file(&settings_path) {
            Some(v) if v.is_object() => v,
            _ => {
                return ApplyResult {
                    success: false,
                    message:
                        "~/.claude/settings.json exists but isn't valid JSON. Fix or remove it, then apply again — EchoBird won't overwrite it to avoid losing your Claude Code settings."
                            .to_string(),
                };
            }
        }
    } else {
        serde_json::json!({})
    };
    {
        let obj = config.as_object_mut().expect("config is an object");
        if !obj.get("env").map(|v| v.is_object()).unwrap_or(false) {
            obj.insert("env".to_string(), serde_json::json!({}));
        }
    }
    let env = config
        .get_mut("env")
        .and_then(|v| v.as_object_mut())
        .expect("env is an object");

    // Shared knobs (match the long-standing generic claudecode mapping).
    env.insert(
        "API_TIMEOUT_MS".to_string(),
        serde_json::Value::String("3000000".to_string()),
    );
    env.insert(
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
        serde_json::Value::String("1".to_string()),
    );

    // Both modes authenticate via ANTHROPIC_AUTH_TOKEN (Bearer), so always drop
    // any ANTHROPIC_API_KEY (x-api-key) the user may have left in settings.json —
    // a stale real key would otherwise conflict. We *remove* the key rather than
    // write an empty string (the old generic mapping could only set "", not
    // delete) so settings.json stays clean.
    env.remove("ANTHROPIC_API_KEY");

    if relay_mode {
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            serde_json::Value::String(anthropic_url.clone()),
        );
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            serde_json::Value::String(api_key.clone()),
        );
        let one_m = model_info.one_m_context.unwrap_or(false);
        for var in CLAUDECODE_MODEL_VARS {
            env.insert(
                var.to_string(),
                serde_json::Value::String(claudecode_env_model_id(&real_model_id, var, one_m)),
            );
        }
    } else {
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            serde_json::Value::String(proxy_base),
        );
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            serde_json::Value::String("echobird-local-proxy".to_string()),
        );
        for var in CLAUDECODE_MODEL_VARS {
            env.remove(var);
        }
    }

    if let Err(e) = write_json_file(&settings_path, &config) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Code settings: {}", e),
        };
    }

    // Relay side-channel — real upstream, read fresh by anthropic_proxy on
    // every /claudecode/v1/messages request and by read_claudecode for the UI.
    let relay_path = echobird_dir().join("claudecode.json");
    let relay = serde_json::json!({
        "baseUrl": anthropic_url,
        "apiKey": api_key,
        "actualModel": real_model_id,
        "modelName": model_info.name.as_deref().unwrap_or(real_model_id.as_str()),
        "relayMode": relay_mode,
    });
    if let Err(e) = write_json_file(&relay_path, &relay) {
        return ApplyResult {
            success: false,
            message: format!("Failed to write Claude Code relay file: {}", e),
        };
    }

    ApplyResult {
        success: true,
        message:
            "Claude Code configured. Restart Claude Code (or /exit and reopen) for the change to take effect."
                .to_string(),
    }
}

fn read_claudecode() -> Option<ModelInfo> {
    // Source of truth is the relay file (written in both modes), NOT
    // settings.json — mirrors read_claudedesktop. Surfaces the real upstream
    // model so the App Desktop card shows it after a rescan.
    let relay_path = echobird_dir().join("claudecode.json");
    let relay = read_json_file(&relay_path)?;

    let anthropic_url = relay
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let api_key = relay
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let model = relay
        .get("actualModel")
        .and_then(|v| v.as_str())
        // Legacy tolerance: relay files written by ≤v5.3.8 (which had a 1M
        // toggle) may carry a `[1m]` suffix on actualModel. Strip it so the
        // surfaced active-model id matches the user's stored modelId —
        // otherwise the App Desktop card's "currently applied" highlight
        // fails to match after a rescan. Nothing writes the suffix any more.
        .map(|s| s.strip_suffix("[1m]").unwrap_or(s))
        .filter(|s| !s.is_empty())
        .map(String::from);
    let name = relay
        .get("modelName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Some(ModelInfo {
        name,
        model,
        base_url: None,
        api_key,
        anthropic_url: Some(anthropic_url),
        protocol: Some("anthropic".to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

/// Surgically remove the env keys we own from ~/.claude/settings.json and
/// drop the relay file. Unlike the generic restore (which deletes the whole
/// config file), this preserves allowedTools / hooks / anything else the user
/// keeps in settings.json — deleting it wholesale would wipe their Claude Code
/// setup, not just our model config.
fn restore_claudecode_to_official() -> ApplyResult {
    const OUR_ENV_KEYS: [&str; 12] = [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_MODEL",
        // Kept for migration cleanup: older applies wrote this now-deprecated
        // var, so restore must still strip it from legacy settings.json even
        // though apply_claudecode no longer writes it.
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        // Pins subagents to the upstream model in relay mode (new env var,
        // replaces the small-fast tier's role for subagent selection).
        "CLAUDE_CODE_SUBAGENT_MODEL",
        "API_TIMEOUT_MS",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    ];

    if let Some(home) = dirs::home_dir() {
        let settings_path = home.join(".claude").join("settings.json");
        if settings_path.exists() {
            match read_json_file(&settings_path) {
                Some(mut config) => {
                    if let Some(env) = config.get_mut("env").and_then(|v| v.as_object_mut()) {
                        for key in OUR_ENV_KEYS {
                            env.remove(key);
                        }
                        let _ = write_json_file(&settings_path, &config);
                    }
                }
                None => {
                    // Can't parse settings.json → can't strip our env keys. Abort
                    // BEFORE deleting the relay file: in bridge mode ANTHROPIC_BASE_URL
                    // still points at the proxy, so dropping the relay would leave
                    // Claude Code 503-ing on every request while we claim success.
                    return ApplyResult {
                        success: false,
                        message:
                            "~/.claude/settings.json couldn't be parsed, so EchoBird's keys can't be cleanly removed. Restore aborted to avoid leaving Claude Code pointed at a stopped proxy — fix the file and try again."
                                .to_string(),
                    };
                }
            }
        }
    }

    let relay_path = echobird_dir().join("claudecode.json");
    if relay_path.exists() {
        let _ = fs::remove_file(&relay_path);
    }

    ApplyResult {
        success: true,
        message: "Claude Code restored to official provider. Restart Claude Code for the change to take effect."
            .to_string(),
    }
}

fn apply_aider(model_info: &ModelInfo) -> ApplyResult {
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
fn apply_vibe_trading(model_info: &ModelInfo) -> ApplyResult {
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

fn read_aider() -> Option<ModelInfo> {
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

/// Read the active model back from `~/.vibe-trading/.env`. EchoBird always
/// writes the OpenAI-compatible block, so the model lives in
/// `LANGCHAIN_MODEL_NAME` with `OPENAI_BASE_URL` / `OPENAI_API_KEY`.
fn read_vibe_trading() -> Option<ModelInfo> {
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

fn apply_workbuddy(model_info: &ModelInfo) -> ApplyResult {
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

fn read_workbuddy() -> Option<ModelInfo> {
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

fn apply_grok(model_info: &ModelInfo) -> ApplyResult {
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

fn restore_grok_to_official() -> ApplyResult {
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

fn read_grok() -> Option<ModelInfo> {
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

// ════════════════════════════════════════════════════════════════
//  Qwen Code: direct write to ~/.qwen/settings.json
//  Format: { modelProviders: { openai: [...] }, env: {...},
//           security: { auth: { selectedType } }, model: { name } }
// ════════════════════════════════════════════════════════════════

fn apply_qwen_code(model_info: &ModelInfo) -> ApplyResult {
    let config_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".qwen")
        .join("settings.json");

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

    let base_url = model_info
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string();
    let api_key = model_info.api_key.as_deref().unwrap_or("");
    let protocol = model_info.protocol.as_deref().unwrap_or("openai");

    // Qwen Code supports: openai, anthropic, gemini
    let selected_type = match protocol {
        "anthropic" => "anthropic",
        "gemini" => "gemini",
        _ => "openai",
    };

    // Env key name derived from domain to avoid collisions
    let domain = extract_domain_name(&base_url);
    let env_key = format!("ECHOBIRD_{}_API_KEY", domain.to_uppercase());
    let display_name = model_info.name.as_deref().unwrap_or(model_id);

    // Read existing config or start fresh
    let mut config = read_json_file(&config_path).unwrap_or(serde_json::json!({}));

    // Build the model provider entry
    let provider_entry = serde_json::json!({
        "id": model_id,
        "name": display_name,
        "baseUrl": base_url,
        "description": model_id,  // Use model ID instead of domain name
        "envKey": env_key
    });

    // Write modelProviders — replace the protocol array with single entry
    config["modelProviders"][selected_type] = serde_json::json!([provider_entry]);

    // Write env — set the API key
    config["env"][&env_key] = serde_json::Value::String(api_key.to_string());

    // Write security auth type
    config["security"]["auth"]["selectedType"] =
        serde_json::Value::String(selected_type.to_string());

    // Write active model
    config["model"]["name"] = serde_json::Value::String(model_id.to_string());

    match write_json_file(&config_path, &config) {
        Ok(_) => {
            log::info!(
                "[ToolConfigManager] QwenCode config written: {} ({})",
                model_id,
                selected_type
            );
            ApplyResult {
                success: true,
                message: format!(
                    "Model \"{}\" configured for Qwen Code. Restart qwen to apply.",
                    display_name
                ),
            }
        }
        Err(e) => ApplyResult {
            success: false,
            message: e,
        },
    }
}

fn read_qwen_code() -> Option<ModelInfo> {
    let config_path = dirs::home_dir()?.join(".qwen").join("settings.json");
    let config = read_json_file(&config_path)?;

    // Read active model name
    let model_name = config.pointer("/model/name")?.as_str()?.to_string();
    if model_name.is_empty() {
        return None;
    }

    // Read auth type to determine which provider array to look at
    let selected_type = config
        .pointer("/security/auth/selectedType")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");

    // Find the model entry in the provider array
    let providers = config
        .pointer(&format!("/modelProviders/{}", selected_type))
        .and_then(|v| v.as_array())?;

    let entry = providers
        .iter()
        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(&model_name))
        .or_else(|| providers.first())?;

    let base_url = entry
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Resolve API key from env object via envKey reference
    let api_key = entry
        .get("envKey")
        .and_then(|v| v.as_str())
        .and_then(|env_key| config.pointer(&format!("/env/{}", env_key)))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(ModelInfo {
        name: entry
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: Some(model_name),
        base_url,
        api_key,
        anthropic_url: None,
        protocol: Some(selected_type.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

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

fn apply_pi(model_info: &ModelInfo) -> ApplyResult {
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

fn read_pi() -> Option<ModelInfo> {
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

fn restore_pi_to_official() -> ApplyResult {
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

fn apply_kimicode(model_info: &ModelInfo) -> ApplyResult {
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

fn read_kimicode() -> Option<ModelInfo> {
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

fn restore_kimicode_to_official() -> ApplyResult {
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

// ════════════════════════════════════════════════════════════════
//  Simple TOML helpers (top-level key = "value" only)
// ════════════════════════════════════════════════════════════════

fn toml_read_top(content: &str, key: &str) -> String {
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') || t.starts_with('#') || t.is_empty() {
            if t.starts_with('[') {
                break;
            } // Entered sections, stop
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            if k.trim() == key {
                let v = v.trim();
                if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
                    return v[1..v.len() - 1].to_string();
                }
                return v.to_string();
            }
        }
    }
    String::new()
}

/// Surgically remove a top-level `key = ...` line from a TOML document,
/// preserving every other line and section verbatim. Mirrors the
/// top-level-only scan of `toml_write_top`: only keys before the first
/// `[section]` are candidates (the write helpers never touch keys
/// inside a section, so a top-level key is the only shape we'd ever
/// need to delete). No-op if the key is absent. The trailing-newline
/// convention is re-applied by `write_codex_canonical_fields` at the
/// end of its pipeline, so this helper — like `toml_write_top` — does
/// not re-add it.
///
/// Used to evict legacy keys we no longer write (e.g. `review_model`)
/// so a stale value left by an older EchoBird version can't survive a
/// model switch / pre-spawn self-heal.
fn toml_delete_top(content: &str, key: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut first_section: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if first_section.is_none() && t.starts_with('[') {
            first_section = Some(i);
        }
        // Once we've entered the first [section], the remaining top-level
        // keys are exhausted — stop scanning (a same-named key inside a
        // section belongs to that section, not the top level).
        if first_section.is_some() && i >= first_section.unwrap() {
            break;
        }
        if let Some((k, _)) = t.split_once('=') {
            if k.trim() == key {
                lines.remove(i);
                break;
            }
        }
        i += 1;
    }
    lines.join("\n")
}

fn toml_write_top(content: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found = false;
    let mut first_section: Option<usize> = None;

    for (i, line) in lines.iter_mut().enumerate() {
        let t = line.trim();
        if first_section.is_none() && t.starts_with('[') {
            first_section = Some(i);
        }
        if first_section.is_some() && i >= first_section.unwrap() {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            if k.trim() == key {
                *line = format!("{} = \"{}\"", key, toml_escape(value));
                found = true;
                break;
            }
        }
    }

    if !found {
        let new_line = format!("{} = \"{}\"", key, toml_escape(value));
        match first_section {
            Some(i) => lines.insert(i, new_line),
            None => lines.push(new_line),
        }
    }
    lines.join("\n")
}

/// Variant of `toml_write_top` that writes the value verbatim, without
/// wrapping it in `"..."`. Use for booleans (`true`/`false`) and integers
/// — TOML rejects them when quoted. Mirrors the line-based, overwrite-
/// or-insert semantics of the string variant.
fn toml_write_top_raw(content: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found = false;
    let mut first_section: Option<usize> = None;

    for (i, line) in lines.iter_mut().enumerate() {
        let t = line.trim();
        if first_section.is_none() && t.starts_with('[') {
            first_section = Some(i);
        }
        if first_section.is_some() && i >= first_section.unwrap() {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            if k.trim() == key {
                *line = format!("{} = {}", key, value);
                found = true;
                break;
            }
        }
    }

    if !found {
        let new_line = format!("{} = {}", key, value);
        match first_section {
            Some(i) => lines.insert(i, new_line),
            None => lines.push(new_line),
        }
    }
    lines.join("\n")
}

/// Surgically write `key = "value"` inside `[table]` of a TOML
/// document, preserving every other line and section verbatim. If the
/// table doesn't exist, append it at end-of-file. If the key doesn't
/// exist inside the table, insert it just after the table header.
/// Mirrors `toml_write_top` line-based semantics — no full parse, no
/// reformatting, no comment loss. Used by `apply_codex` to canonicalize
/// `[model_providers.OpenAI]` fields without clobbering Codex's own
/// runtime state (`[projects.*]` trust, `[tui.*]` NUX, etc.) that sits
/// in the same file. Also reused by `codex_proxy::config_manager::
/// ensure_canonical_config` for its drift-recovery branch.
pub(crate) fn toml_write_table_value(content: &str, table: &str, key: &str, value: &str) -> String {
    let header = format!("[{}]", table);
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    let table_start = lines.iter().position(|l| l.trim() == header.as_str());

    let table_start = match table_start {
        Some(i) => i,
        None => {
            // Table missing — append. Pad with a blank line if the
            // existing file doesn't already end with one.
            if !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push(header);
            lines.push(format!("{} = \"{}\"", key, toml_escape(value)));
            return lines.join("\n");
        }
    };

    // Find table's end (next section header or EOF).
    let table_end = lines
        .iter()
        .enumerate()
        .skip(table_start + 1)
        .find_map(|(i, l)| {
            let t = l.trim();
            if t.starts_with('[') && t.ends_with(']') {
                Some(i)
            } else {
                None
            }
        })
        .unwrap_or(lines.len());

    // Look for the key inside the table's range.
    let key_line = (table_start + 1..table_end).find(|&i| {
        let t = lines[i].trim();
        if t.starts_with('#') || t.is_empty() {
            return false;
        }
        match t.split_once('=') {
            Some((k, _)) => k.trim() == key,
            None => false,
        }
    });

    let replacement = format!("{} = \"{}\"", key, toml_escape(value));
    match key_line {
        Some(i) => lines[i] = replacement,
        None => lines.insert(table_start + 1, replacement),
    }

    lines.join("\n")
}

/// Variant of `toml_write_table_value` that writes the value verbatim,
/// without wrapping it in `"..."`. For booleans / integers inside a
/// table (e.g. `requires_openai_auth = true`). Same surgical line-based
/// semantics; same preservation of unrelated sections.
fn toml_write_table_value_raw(content: &str, table: &str, key: &str, value: &str) -> String {
    let header = format!("[{}]", table);
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    let table_start = lines.iter().position(|l| l.trim() == header.as_str());

    let table_start = match table_start {
        Some(i) => i,
        None => {
            if !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push(header);
            lines.push(format!("{} = {}", key, value));
            return lines.join("\n");
        }
    };

    let table_end = lines
        .iter()
        .enumerate()
        .skip(table_start + 1)
        .find_map(|(i, l)| {
            let t = l.trim();
            if t.starts_with('[') && t.ends_with(']') {
                Some(i)
            } else {
                None
            }
        })
        .unwrap_or(lines.len());

    let key_line = (table_start + 1..table_end).find(|&i| {
        let t = lines[i].trim();
        if t.starts_with('#') || t.is_empty() {
            return false;
        }
        match t.split_once('=') {
            Some((k, _)) => k.trim() == key,
            None => false,
        }
    });

    let replacement = format!("{} = {}", key, value);
    match key_line {
        Some(i) => lines[i] = replacement,
        None => lines.insert(table_start + 1, replacement),
    }

    lines.join("\n")
}

fn toml_read_table_value(content: &str, table: &str, key: &str) -> String {
    let header = format!("[{}]", table);
    let mut in_table = false;

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_table = t == header;
            continue;
        }
        if !in_table || t.starts_with('#') || t.is_empty() {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            if k.trim() == key {
                return toml_unquote(v.trim());
            }
        }
    }

    String::new()
}

fn toml_unquote(value: &str) -> String {
    let v = value.trim();
    if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
        v[1..v.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        v.to_string()
    }
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_delete_top_removes_a_top_level_key() {
        // The key we own sits before the first [section]: delete it, keep
        // the rest verbatim (whitespace, comments, sections all untouched).
        let content = "model_provider = \"OpenAI\"\n\
                       model = \"gpt-5.5\"\n\
                       review_model = \"gpt-5.5\"\n\
                       \n\
                       [model_providers.OpenAI]\n\
                       name = \"OpenAI\"\n";
        let out = toml_delete_top(content, "review_model");
        assert!(!out.contains("review_model"));
        assert!(out.contains("model = \"gpt-5.5\""));
        assert!(out.contains("[model_providers.OpenAI]"));
        assert!(out.contains("name = \"OpenAI\""));
    }

    #[test]
    fn toml_delete_top_is_noop_when_key_absent() {
        // No key removed → only the join-strips-trailing-newline behavior
        // of `content.lines().collect().join("\n")` changes the string (the
        // caller, write_codex_canonical_fields, re-adds it). Assert the scan
        // matched nothing by checking the lines content survives.
        let content = "model = \"gpt-5.5\"\n[model_providers.OpenAI]\n";
        let out = toml_delete_top(content, "review_model");
        assert!(out.contains("model = \"gpt-5.5\""));
        assert!(out.contains("[model_providers.OpenAI]"));
        assert!(!out.contains("review_model"));
    }

    #[test]
    fn toml_delete_top_ignores_same_named_key_inside_a_section() {
        // A `review_model` that lives INSIDE a [section] belongs to that
        // section — not a top-level key we own — so it must survive. Only
        // the pre-section scan should match.
        let content = "model = \"gpt-5.5\"\n\
                       [model_providers.OpenAI]\n\
                       review_model = \"leave-me\"\n";
        let out = toml_delete_top(content, "review_model");
        assert!(out.contains("review_model = \"leave-me\""));
        assert!(out.contains("model = \"gpt-5.5\""));
    }

    #[test]
    fn write_codex_canonical_fields_evicts_stale_review_model_on_direct_connect() {
        // Regression: an older EchoBird version wrote `review_model =
        // "gpt-5.5"`. Our write helpers never delete, so it survived every
        // switch. Under a Responses direct-connect session (real model id
        // + real upstream base_url) Codex would still send `gpt-5.5` on its
        // review pass to a gateway that only knows the real id → 4xx. The
        // canonical write must strip the stale line so `model` is the only
        // model key the upstream ever sees.
        let stale = "model_provider = \"OpenAI\"\n\
                     model = \"gpt-5.5\"\n\
                     review_model = \"gpt-5.5\"\n\
                     [model_providers.OpenAI]\n\
                     name = \"OpenAI\"\n";
        let out = write_codex_canonical_fields(
            stale,
            "https://ark.cn-beijing.volces.com/api/coding/v1",
            "glm-5.2",
            DEFAULT_CODEX_CONTEXT_WINDOW,
        );
        assert!(
            !out.contains("review_model"),
            "stale review_model survived: {out}"
        );
        assert!(out.contains("model = \"glm-5.2\""));
        assert!(out.contains("base_url = \"https://ark.cn-beijing.volces.com/api/coding/v1\""));
    }

    #[test]
    fn write_codex_canonical_fields_evicts_review_model_in_bridge_mode_too() {
        // Bridge mode (proxy base_url + gpt-5.5 alias) must ALSO strip a
        // stale review_model — the pre-spawn self-heal runs write_codex
        // _canonical_fields too, and a cold start shouldn't leave a stale
        // value lying around even when not on direct connect.
        let stale = "model = \"gpt-5.5\"\n\
                     review_model = \"gpt-5.5\"\n\
                     [model_providers.OpenAI]\n";
        let out = write_codex_canonical_fields(
            stale,
            "http://127.0.0.1:53682/v1",
            "gpt-5.5",
            DEFAULT_CODEX_CONTEXT_WINDOW,
        );
        assert!(!out.contains("review_model"));
    }

    #[test]
    fn write_codex_canonical_fields_evicts_stale_model_catalog_json() {
        // Regression: `model_catalog_json` is conditional (apply_codex writes
        // it only for Responses passthrough + bundled-vendor catalogs). After
        // switching back to bridge mode — or to a non-catalog vendor — the
        // stale line must not survive, or config.toml points at a catalog that
        // no longer matches the selected model. Same never-delete-helper
        // problem as review_model: the canonical write owns the eviction.
        let stale = "model_provider = \"OpenAI\"\n\
                     model = \"gpt-5.5\"\n\
                     model_catalog_json = \"C:/Users/x/.codex/models.json\"\n\
                     [model_providers.OpenAI]\n\
                     name = \"OpenAI\"\n";
        let out = write_codex_canonical_fields(
            stale,
            "http://127.0.0.1:53682/v1",
            "gpt-5.5",
            DEFAULT_CODEX_CONTEXT_WINDOW,
        );
        assert!(
            !out.contains("model_catalog_json"),
            "stale model_catalog_json survived: {out}"
        );
    }

    #[test]
    fn codex_catalog_referenced_matches_only_our_canonical_path() {
        // Our absolute forward-slash form.
        let ours = "C:/Users/x/.codex/models.json";
        let abs = "model_provider = \"OpenAI\"\n\
                   model_catalog_json = \"C:/Users/x/.codex/models.json\"\n\
                   [model_providers.OpenAI]\n";
        assert!(codex_catalog_referenced(abs, ours));
        // Vendor-doc tilde shorthand resolves to the same file.
        let tilde = "model_catalog_json = \"~/.codex/models.json\"\n";
        assert!(codex_catalog_referenced(tilde, ours));
        // A user's own catalog pointed at a DIFFERENT path must NOT match.
        let other_path = "model_catalog_json = \"~/.codex/model-catalogs/custom-catalog.json\"\n";
        assert!(!codex_catalog_referenced(other_path, ours));
        // Config with no catalog line must not match.
        let no_line = "model = \"gpt-5.5\"\n[model_providers.OpenAI]\n";
        assert!(!codex_catalog_referenced(no_line, ours));
    }

    fn claudecode_model_info(relay_mode: Option<bool>) -> ModelInfo {
        ModelInfo {
            name: Some("MiMo v2.5 Pro".to_string()),
            model: Some("mimo-v2.5-pro".to_string()),
            base_url: Some("https://api.example.com/v1".to_string()),
            api_key: Some("test-key".to_string()),
            anthropic_url: None,
            protocol: Some("anthropic".to_string()),
            display_model: None,
            relay_mode,
            responses_passthrough: None,
            web_search: None,
            one_m_context: None,
        }
    }

    #[test]
    fn claudecode_normalize_never_decorates_model_id() {
        // The model id must reach the upstream verbatim in every mode. Claude
        // decorations like the `[1m]` suffix are gone entirely: in bridge mode
        // Claude Code's own built-in claude-* ids already budget the full
        // window, and the rewritten upstream id must be exactly what the
        // third-party provider advertises.
        for relay_mode in [None, Some(false), Some(true)] {
            let info =
                normalize_model_info_for_tool("claudecode", claudecode_model_info(relay_mode));
            assert_eq!(info.model.as_deref(), Some("mimo-v2.5-pro"));
        }
    }

    #[test]
    fn claudecode_normalize_strips_trailing_v1_from_base_url() {
        // Relay mode appends `/v1/messages` client-side, so a `/v1` left on the
        // base URL would double into `/v1/v1/messages`.
        let info = normalize_model_info_for_tool("claudecode", claudecode_model_info(None));
        assert_eq!(info.base_url.as_deref(), Some("https://api.example.com"));
    }

    // API Router (relay) full write-in pins every model tier to the real
    // upstream model so Claude Code uses the third-party model end-to-end.
    // Two requirements locked here, per https://code.claude.com/docs/en/model-config:
    //   * CLAUDE_CODE_SUBAGENT_MODEL must be written — otherwise subagents
    //     fall back to claude-* ids and bypass the router (breaks "全量").
    //   * ANTHROPIC_SMALL_FAST_MODEL must NOT be written — Claude Code
    //     deprecated it in favor of ANTHROPIC_DEFAULT_HAIKU_MODEL (still pinned).
    #[test]
    fn claudecode_model_vars_pin_subagent_and_drop_small_fast() {
        assert!(
            CLAUDECODE_MODEL_VARS.contains(&"CLAUDE_CODE_SUBAGENT_MODEL"),
            "relay mode must pin CLAUDE_CODE_SUBAGENT_MODEL so subagents use the third-party model"
        );
        assert!(
            !CLAUDECODE_MODEL_VARS.contains(&"ANTHROPIC_SMALL_FAST_MODEL"),
            "ANTHROPIC_SMALL_FAST_MODEL is deprecated (favor ANTHROPIC_DEFAULT_HAIKU_MODEL, also pinned) — stop writing it"
        );
    }

    // ── claudecode_env_model_id: [1m] opt-in, 1M-tier-only ──
    // The 1M-context toggle is relay-only and Claude-Code-only. When opted in,
    // `[1m]` is appended to the 1M-capable tier (MODEL / SONNET / OPUS / FABLE) so CC
    // budgets the 1M window; HAIKU + SUBAGENT stay bare (no 1M concept). CC
    // strips the suffix before sending upstream, so the provider sees the bare id.
    #[test]
    fn claudecode_env_model_id_appends_1m_for_1m_tier_when_opt_in() {
        for &var in CLAUDECODE_MODEL_VARS_1M {
            assert_eq!(
                claudecode_env_model_id("mimo-v2.5-pro", var, true),
                "mimo-v2.5-pro[1m]",
                "opt-in 1M must decorate the 1M-capable tier var {}",
                var
            );
        }
    }

    #[test]
    fn claudecode_env_model_id_leaves_haiku_and_subagent_bare_even_when_opt_in() {
        for var in [
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ] {
            assert_eq!(
                claudecode_env_model_id("mimo-v2.5-pro", var, true),
                "mimo-v2.5-pro",
                "opt-in must NOT decorate the non-1M tier var {}",
                var
            );
        }
    }

    #[test]
    fn claudecode_env_model_id_bare_when_opt_out() {
        // Opt-out (or unset) → every var stays bare, regardless of tier.
        for var in CLAUDECODE_MODEL_VARS {
            assert_eq!(
                claudecode_env_model_id("mimo-v2.5-pro", var, false),
                "mimo-v2.5-pro",
                "opt-out must leave {} bare",
                var
            );
        }
    }

    // ── Per-model context window + compaction (Codex) ──
    // apply_codex must write the real model_context_window and a proportional
    // model_auto_compact_token_limit (90% of the window) so a model whose
    // window is smaller than the historic 1M default is not over-claimed.

    #[test]
    fn model_context_window_for_known_model() {
        assert_eq!(model_context_window_for("MiniMax-M2.7"), 204_800);
    }

    #[test]
    fn model_context_window_for_unknown_model_defaults_to_1m() {
        assert_eq!(
            model_context_window_for("glm-5.2"),
            DEFAULT_CODEX_CONTEXT_WINDOW
        );
    }

    #[test]
    fn codex_compact_limit_is_90_percent_of_window() {
        assert_eq!(codex_compact_limit_for(1_000_000), 900_000);
        assert_eq!(codex_compact_limit_for(204_800), 184_320);
    }

    #[test]
    fn write_codex_canonical_fields_writes_model_context_window() {
        // A 204,800-token model must get model_context_window = 204800 and
        // model_auto_compact_token_limit = 184320 (90%), not the historic
        // 1,000,000 / 900,000 defaults.
        let out = write_codex_canonical_fields(
            "model = \"gpt-5.5\"\n[model_providers.OpenAI]\n",
            "http://127.0.0.1:53682/v1",
            "gpt-5.5",
            204_800,
        );
        assert!(out.contains("model_context_window = 204800"), "got: {out}");
        assert!(
            out.contains("model_auto_compact_token_limit = 184320"),
            "got: {out}"
        );
        assert!(
            !out.contains("model_context_window = 1000000"),
            "got: {out}"
        );
        assert!(
            !out.contains("model_auto_compact_token_limit = 900000"),
            "got: {out}"
        );
    }

    // ── Per-model input modalities (ZCode) ──
    // apply_zcode must declare the real input modalities for a multimodal
    // model instead of forcing text-only.

    #[test]
    fn model_input_modalities_for_multimodal_model() {
        let m = model_input_modalities_for("MiniMax-M3");
        assert_eq!(m, &["text", "image", "video"]);
    }

    #[test]
    fn model_input_modalities_for_unknown_model_defaults_to_text() {
        assert_eq!(model_input_modalities_for("glm-5.2"), &["text"]);
    }
}
