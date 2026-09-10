// Model configuration entry points and shared file-format helpers.
// Tool-specific implementations live in tool_config_manager/<tool>.rs.

mod aider;
mod claudecode;
mod claudedesktop;
mod codex;
mod dsh;
mod generic;
mod grok;
mod kilo;
mod kimicode;
mod mimocode;
mod mimodesktop;
mod omp;
mod openclaw;
mod opencode;
mod openscience;
mod pi;
mod qwencode;
mod relay;
mod vibe_trading;
mod workbuddy;
mod zcode;

use crate::services::tool_manager;
use aider::{apply_aider, read_aider};
use claudecode::{
    apply_claudecode, normalize_model_info_for_tool, read_claudecode,
    restore_claudecode_to_official,
};
use claudedesktop::{apply_claudedesktop, read_claudedesktop, restore_claudedesktop_to_official};
use codex::{apply_codex, read_codex, restore_codex_to_official};
pub(crate) use codex::{
    write_codex_canonical_fields, CODEX_DISPLAY_MODEL, DEFAULT_CODEX_CONTEXT_WINDOW,
};
use dsh::{apply_dsh, read_dsh, restore_dsh_to_official};
use generic::{apply_generic_json, read_generic_json};
use grok::{apply_grok, read_grok, restore_grok_to_official};
pub use kilo::kilo_echobird_model;
use kilo::{apply_kilo, read_kilo, restore_kilo_to_official};
use kimicode::{apply_kimicode, read_kimicode, restore_kimicode_to_official};
pub use mimocode::mimocode_echobird_model;
use mimocode::{apply_mimocode, read_mimocode, restore_mimocode_to_official};
use openclaw::{apply_openclaw, read_openclaw};
use opencode::{apply_opencode, read_opencode, restore_opencode_to_official};
use openscience::{apply_openscience, read_openscience, restore_openscience_to_official};
use pi::{apply_pi, read_pi, restore_pi_to_official};
use qwencode::{apply_qwen_code, read_qwen_code};
use relay::{apply_echobird_relay, read_echobird_relay};
use std::fs;
use std::path::{Path, PathBuf};
use vibe_trading::{apply_vibe_trading, read_vibe_trading};
use workbuddy::{apply_workbuddy, read_workbuddy};
use zcode::{apply_zcode, read_zcode, restore_zcode_to_official};

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

/// Look up a model's real input modalities. Returns `["text"]` for models the
/// registry does not list so ZCode's `modalities.input` keeps the safe
/// text-only default for unknown ids.
fn model_input_modalities_for(model_id: &str) -> &'static [&'static str] {
    match model_id {
        "MiniMax-M3" => &["text", "image", "video"],
        _ => &["text"],
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
        "mimodesktop" => return mimodesktop::apply(&model_info),

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
    if tool_id == "mimodesktop" {
        return mimodesktop::restore();
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
        "mimodesktop" => return mimodesktop::read(),
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
