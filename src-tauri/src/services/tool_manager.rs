// Tool manager �?loads tool definitions, detects installed tools, manages configs
// Mirrors tools/loader.ts architecture:
// Each tool = directory with paths.json (detection) + config.json (config mapping)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::tool::{
    ConfigMapping, DetectedTool, InstallHints, PathsConfig, ToolCategory, ToolDefinition,
};
use crate::utils::platform;

// ─── Path expansion (mirrors tools/utils.ts expandPath) ───

/// Normalise a path string for UI display.
/// - Strip Windows UNC prefix `\\?\` that Rust's canonicalize / Tauri resource_dir adds
///   (without this, paths in the UI look like `\\?\C:\Users\...`).
/// - On Windows, replace any forward slashes with native backslashes so the UI shows
///   the same separator users see in Explorer / cmd / file dialogs. Tool JSON configs
///   use `/` for cross-platform portability and `expand_path` doesn't normalise, which
///   would otherwise leak mixed-separator paths like `C:\Users\eben/.codex/config.toml`
///   into the App Desktop cards.
///
/// On non-Windows the function is a no-op (forward slash is already native).
fn normalize_for_display(s: String) -> String {
    #[cfg(target_os = "windows")]
    let s = {
        let stripped = s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s);
        stripped.replace('/', "\\")
    };
    s
}

/// Expand ~ and %ENV_VAR% in path strings
pub fn expand_path(p: &str) -> PathBuf {
    let mut result = p.to_string();

    // Expand ~ to home directory
    if result.starts_with("~/") || result.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            result = format!("{}{}", home.display(), &result[1..]);
        }
    }

    // Expand %ENV_VAR% (Windows style)
    while let Some(start) = result.find('%') {
        if let Some(end) = result[start + 1..].find('%') {
            let var_name = &result[start + 1..start + 1 + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[start + 2 + end..]
            );
        } else {
            break;
        }
    }

    PathBuf::from(result)
}

// ─── Tauri resource_dir (set at startup via init_resource_dir) ───

use std::sync::Mutex as ResDirMutex;
static RESOURCE_DIR: ResDirMutex<Option<PathBuf>> = ResDirMutex::new(None);

/// Called once at app startup to store Tauri's resource_dir for correct platform paths.
/// resource_dir() returns the right location on every OS:
///   macOS  → <app>.app/Contents/Resources
///   Windows → install dir
///   Linux   → /usr/lib/com.echobird.ai  (deb)  or  $APPDIR/usr/lib/com.echobird.ai  (AppImage)
pub fn init_resource_dir(path: PathBuf) {
    if let Ok(mut guard) = RESOURCE_DIR.lock() {
        log::info!("[ToolManager] resource_dir = {:?}", path);
        *guard = Some(path);
    }
}

// ─── Tool directory resolution ───

/// Find the tools directory (tools/)
/// In dev: relative to project root
/// In production: bundled with the app binary
pub fn find_tools_dir() -> Option<PathBuf> {
    // DEV-MODE PRIORITY: if the exe path contains /target/debug/ or
    // /target/release/, we're running from a Cargo build dir (not an
    // installed binary). Tauri only mirrors ../tools/ into _up_/tools/
    // at the START of `tauri dev`, so subdirs added during a dev session
    // won't appear in the mirror until the user restarts. Prefer the
    // SOURCE tree's tools/ directly — it's always the freshest copy.
    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy().replace('\\', "/");
        let is_cargo_target =
            exe_str.contains("/target/debug/") || exe_str.contains("/target/release/");
        if is_cargo_target {
            // exe at <repo>/src-tauri/target/{debug,release}/echobird.exe
            // Walk up 4 parents: target/debug → target → src-tauri → <repo>
            if let Some(repo_root) = exe
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                let src_tools = repo_root.join("tools");
                if src_tools.is_dir() {
                    log::info!(
                        "[ToolManager] dev mode — using source tree tools dir: {:?}",
                        src_tools
                    );
                    return Some(src_tools);
                }
            }
        }
    }

    // 0. Tauri-native resource_dir (most reliable — set at startup via init_resource_dir)
    //
    // Tauri v2 bundles resources relative to src-tauri/ using "_up_" for "../" paths:
    //   config: "../tools/"  →  resource_dir/_up_/tools/  (confirmed on Linux deb)
    //   config: "tools"      →  resource_dir/tools/
    //
    // Reality (confirmed via dpkg -L echobird on Ubuntu):
    //   binary: /usr/bin/echobird
    //   tools:  /usr/lib/Echobird/_up_/tools/
    //   resource_dir = /usr/lib/Echobird/
    if let Ok(guard) = RESOURCE_DIR.lock() {
        if let Some(ref res_dir) = *guard {
            // Case 1: standard subdirectory
            let tools_dir = res_dir.join("tools");
            if tools_dir.is_dir() {
                log::info!(
                    "[ToolManager] Found tools dir (subdirectory): {:?}",
                    tools_dir
                );
                return Some(tools_dir);
            }
            // Case 2: Tauri encodes "../tools/" as _up_/tools/ inside resource_dir
            let up_tools = res_dir.join("_up_").join("tools");
            if up_tools.is_dir() {
                log::info!("[ToolManager] Found tools dir (_up_/tools): {:?}", up_tools);
                return Some(up_tools);
            }
            // Case 3: tool contents placed directly in resource_dir (rare)
            if res_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(res_dir) {
                    let has_tool = entries
                        .filter_map(|e| e.ok())
                        .any(|e| e.path().is_dir() && e.path().join("paths.json").exists());
                    if has_tool {
                        log::info!(
                            "[ToolManager] Found tools dir (resource_dir itself): {:?}",
                            res_dir
                        );
                        return Some(res_dir.clone());
                    }
                }
            }
        }
    }

    // 1. Try relative to current exe (production fallback)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let tools_dir = exe_dir.join("tools");
            if tools_dir.exists() {
                return Some(tools_dir);
            }
            let up_tools = exe_dir.join("_up_").join("tools");
            if up_tools.exists() {
                return Some(up_tools);
            }
            if let Some(parent) = exe_dir.parent() {
                let tools_dir = parent.join("tools");
                if tools_dir.exists() {
                    return Some(tools_dir);
                }
            }
        }
    }

    // 2. Linux: Tauri bundles resources under /usr/lib/{identifier}/
    //    - deb/rpm: /usr/lib/com.echobird.ai/tools/
    //    - AppImage: $APPDIR/usr/lib/com.echobird.ai/tools/
    #[cfg(target_os = "linux")]
    {
        // AppImage mounts at $APPDIR
        if let Ok(appdir) = std::env::var("APPDIR") {
            let tools_dir = PathBuf::from(&appdir).join("usr/lib/com.echobird.ai/tools");
            if tools_dir.exists() {
                return Some(tools_dir);
            }
        }
        // deb/rpm install path
        for prefix in &["/usr/lib/com.echobird.ai", "/usr/lib/echobird"] {
            let tools_dir = PathBuf::from(prefix).join("tools");
            if tools_dir.exists() {
                return Some(tools_dir);
            }
        }
    }

    // 3. Try relative to CARGO_MANIFEST_DIR (dev mode)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let project_root = PathBuf::from(manifest_dir)
            .parent()
            .map(|p| p.to_path_buf());
        if let Some(root) = project_root {
            let tools_dir = root.join("tools");
            if tools_dir.exists() {
                return Some(tools_dir);
            }
        }
    }

    // 4. Try cwd-based lookup (fallback)
    let cwd_tools = PathBuf::from("tools");
    if cwd_tools.exists() {
        return Some(cwd_tools);
    }

    log::warn!("[ToolManager] Cannot find tools directory");
    None
}

// ─── Load tool definitions from directory ───

/// Load all tool definitions by scanning tools/*/paths.json
// Fill in the optional parts of a models.json that an author left out.
// Lets users ship a 5-line `write`-only file and still get the full mapping
// at runtime; full-schema files written before this loosening are no-ops
// here because the relevant fields are already populated.
fn backfill_config_mapping(
    cm: &mut crate::models::tool::ConfigMapping,
    paths_config: &crate::models::tool::PathsConfig,
) {
    use crate::models::tool::ConfigReadMapping;

    // config_file: paths.json already has this for every bundled tool;
    // models.json doesn't need to repeat it.
    if cm.config_file.is_empty() && !paths_config.config_file.is_empty() {
        cm.config_file = paths_config.config_file.clone();
    }

    // read: invert the write map. For each (config_path, model_field) write
    // entry, "model_field" tells us which ModelInfo field flows into that
    // config_path. Reading is just the reverse — ModelInfo.<field> can come
    // from any of the config_paths that wrote it. Multiple writes per field
    // (e.g. anthropic.baseUrl + openai.baseUrl both holding baseUrl) become
    // priority-ordered read paths.
    if cm.read.is_none() {
        if let Some(write_map) = &cm.write {
            let mut model: Vec<String> = Vec::new();
            let mut base_url: Vec<String> = Vec::new();
            let mut api_key: Vec<String> = Vec::new();
            // Sort write entries by config_path for stable derived order
            // across runs (HashMap iteration order is random).
            let mut entries: Vec<(&String, &String)> = write_map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (config_path, model_field) in entries {
                match model_field.as_str() {
                    "model" => model.push(config_path.clone()),
                    "baseUrl" => base_url.push(config_path.clone()),
                    "apiKey" => api_key.push(config_path.clone()),
                    _ => { /* ignore unknown ModelInfo fields */ }
                }
            }
            cm.read = Some(ConfigReadMapping {
                model: if model.is_empty() { None } else { Some(model) },
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
            });
        }
    }
}

// ─── User path overrides (~/.echobird/tool-paths.json) ───
//
// Lets users add install paths for tools whose bundled paths.json missed the
// install (non-default directory, portable layout, etc.) WITHOUT editing the
// bundled files — those are part of the app resources and get overwritten on
// every update. This file lives in the user data dir, so edits survive
// updates; deleting it restores pure defaults. Fully fault-tolerant: a missing
// file, invalid JSON, or an unexpected shape all degrade to "no overrides" and
// never break the scan.
//
// Shape: { "<toolId>": ["full/path/to/binary", ...], ... }
//   • A bare string is accepted as a one-element list.
//   • Keys starting with '_' are skipped, so a user may add their own "_note"
//     field without it being read as a tool id. (The seed itself writes no
//     "_"-prefixed keys — it's pure "<id>": [paths] data.)
fn load_user_path_overrides() -> HashMap<String, Vec<String>> {
    let Some(home) = dirs::home_dir() else {
        return HashMap::new();
    };
    let file = home.join(".echobird").join("tool-paths.json");

    let content = match fs::read_to_string(&file) {
        Ok(c) => c,
        Err(_) => return HashMap::new(), // no file → no overrides (the norm)
    };

    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[ToolManager] ~/.echobird/tool-paths.json is not valid JSON — ignoring overrides: {}",
                e
            );
            return HashMap::new();
        }
    };
    let Some(obj) = parsed.as_object() else {
        return HashMap::new();
    };

    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (tool_id, val) in obj {
        if tool_id.starts_with('_') {
            continue; // doc/template field, not a tool id
        }
        let mut paths: Vec<String> = Vec::new();
        match val {
            serde_json::Value::String(s) => {
                if !s.trim().is_empty() {
                    paths.push(s.clone());
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        if !s.trim().is_empty() {
                            paths.push(s.to_string());
                        }
                    }
                }
            }
            _ => {} // tolerate anything else by ignoring this key
        }
        if !paths.is_empty() {
            out.insert(tool_id.clone(), paths);
        }
    }

    if !out.is_empty() {
        log::info!(
            "[ToolManager] Applied user path overrides for {} tool(s)",
            out.len()
        );
    }
    out
}

/// Prepend the current-OS user override paths to a tool's candidate list, so
/// detection (and the launcher, which reads the same field) also try the
/// user's custom location. User paths go FIRST: someone who bothered to set
/// this almost certainly means that exact binary. Dedupes against existing
/// entries so re-runs don't grow the list.
fn apply_user_path_overrides(paths_config: &mut PathsConfig, extra: &[String]) {
    if extra.is_empty() {
        return;
    }

    #[cfg(target_os = "windows")]
    let slot = &mut paths_config.paths.win32;
    #[cfg(target_os = "macos")]
    let slot = &mut paths_config.paths.darwin;
    #[cfg(target_os = "linux")]
    let slot = &mut paths_config.paths.linux;
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let slot = &mut paths_config.paths.win32;

    let existing = slot.get_or_insert_with(Vec::new);
    let mut merged: Vec<String> = Vec::with_capacity(existing.len() + extra.len());
    for p in extra {
        if !merged.contains(p) {
            merged.push(p.clone());
        }
    }
    for p in existing.iter() {
        if !merged.contains(p) {
            merged.push(p.clone());
        }
    }
    *existing = merged;
}

fn load_tool_definitions() -> Vec<ToolDefinition> {
    load_tool_definitions_with_overrides(true)
}

/// Core loader. `apply_overrides` gates the user `~/.echobird/tool-paths.json`
/// merge: scanning and launching want it ON (via the cached `get_definitions`),
/// while `default_override_seed` wants it OFF — the seed must reflect PURE
/// bundled defaults, or re-creating the file (e.g. after the user deleted it to
/// reset) would bake the user's own already-cached overrides back in as
/// "defaults".
fn load_tool_definitions_with_overrides(apply_overrides: bool) -> Vec<ToolDefinition> {
    let tools_dir = match find_tools_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };

    log::info!("[ToolManager] Scanning tools directory: {:?}", tools_dir);
    let user_overrides = if apply_overrides {
        load_user_path_overrides()
    } else {
        HashMap::new()
    };
    let mut definitions = Vec::new();

    let entries = match fs::read_dir(&tools_dir) {
        Ok(e) => e,
        Err(e) => {
            log::error!("[ToolManager] Failed to read tools directory: {}", e);
            return Vec::new();
        }
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let tool_id = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let paths_file = path.join("paths.json");
        // Prefer models.json (canonical name for the model-field mapping file).
        // Fall back to config.json so legacy bundled or user-installed tools
        // that haven't migrated still load. The new name is documented in
        // the "我的AI项目" / Vibe Coding flow as the file users author.
        let models_file = path.join("models.json");
        let config_file = if models_file.exists() {
            models_file
        } else {
            path.join("config.json")
        };

        if !paths_file.exists() || !config_file.exists() {
            continue;
        }

        // Parse paths.json
        let mut paths_config: PathsConfig = match fs::read_to_string(&paths_file) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(pc) => pc,
                Err(e) => {
                    log::warn!(
                        "[ToolManager] Failed to parse {}/paths.json: {}",
                        tool_id,
                        e
                    );
                    continue;
                }
            },
            Err(e) => {
                log::warn!("[ToolManager] Failed to read {}/paths.json: {}", tool_id, e);
                continue;
            }
        };

        // Merge any user-supplied custom paths for this tool (current OS) on
        // top of the bundled defaults, so an install at a non-default location
        // is still detected. See load_user_path_overrides.
        if let Some(extra) = user_overrides.get(&tool_id) {
            apply_user_path_overrides(&mut paths_config, extra);
        }

        // Parse models.json (or config.json fallback — `config_file` already
        // points at whichever one we found above).
        let mut config_mapping: ConfigMapping = match fs::read_to_string(&config_file) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(cm) => cm,
                Err(e) => {
                    let fname = config_file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("models.json");
                    log::warn!("[ToolManager] Failed to parse {}/{}: {}", tool_id, fname, e);
                    continue;
                }
            },
            Err(e) => {
                let fname = config_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("models.json");
                log::warn!("[ToolManager] Failed to read {}/{}: {}", tool_id, fname, e);
                continue;
            }
        };

        // Auto-fill the parts of models.json a Vibe-Coding user shouldn't have
        // to type. The loader lets them author just the `write` block (5
        // lines for the canonical OpenAI / Anthropic shape); we infer
        // everything else from paths.json + the write map itself.
        backfill_config_mapping(&mut config_mapping, &paths_config);

        log::info!(
            "[ToolManager] Loaded tool: {} ({}) | start_command={:?}, command={}",
            tool_id,
            paths_config.name,
            paths_config.start_command,
            paths_config.command
        );

        definitions.push(ToolDefinition {
            id: tool_id,
            paths_config,
            config_mapping,
            tool_dir: path.to_string_lossy().to_string(),
        });
    }

    log::info!(
        "[ToolManager] Loaded {} tool definitions",
        definitions.len()
    );
    definitions
}

// ─── Tool detection (mirrors loader.ts detect()) ───

/// Get platform-specific paths from PlatformPaths
fn get_platform_paths(paths: &crate::models::tool::PlatformPaths) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        paths.win32.clone().unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        paths.darwin.clone().unwrap_or_default()
    }
    #[cfg(target_os = "linux")]
    {
        paths.linux.clone().unwrap_or_default()
    }
    #[cfg(target_os = "android")]
    {
        let _ = paths;
        Vec::new()
    }
}

// ─── Install-hints scan (per platform) ───
//
// Fallback when paths.json's hardcoded locations miss the install — user
// chose a non-default directory, OS uses a different package layout, etc.
// Authoritative source per platform:
//   Windows: HKLM/HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*
//   macOS:   /Applications, ~/Applications, then mdfind fallback
//   Linux:   /usr/share/applications/*.desktop, ~/.local/share/applications, flatpak exports

/// True when a registry `DisplayName` (already lowercased) is a hit: an EXACT
/// match against any `names_lower`, OR a PREFIX match against any
/// `prefixes_lower` (equal, or starts with `<prefix> ` so a trailing version
/// like "workbuddy 4.24.2" still matches the prefix "workbuddy"). Pure string
/// logic so it stays unit-testable on every platform (the registry walk that
/// uses it is Windows-only).
#[allow(dead_code)]
fn registry_display_name_matches(
    dn_lower: &str,
    names_lower: &[String],
    prefixes_lower: &[String],
) -> bool {
    if names_lower.iter().any(|n| n == dn_lower) {
        return true;
    }
    prefixes_lower
        .iter()
        .any(|p| dn_lower == p || dn_lower.starts_with(&format!("{p} ")))
}

/// Returns true when `path` has a Windows executable extension (`.exe`).
/// Registry `DisplayIcon` values can point at standalone `.ico` files (or
/// `.dll` icon resources); passing a non-executable to Start-Process opens
/// it with the default image viewer instead of launching the app, so we
/// gate on this before trusting a registry-discovered path.
#[cfg(windows)]
fn is_windows_exe(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
}

/// True when an exe file stem (lowercased, no extension) corresponds to the
/// tool's known display names / prefixes — the rule used to pick the real app
/// exe out of an install directory that also contains uninstallers. Mirrors
/// `registry_display_name_matches` but for file stems: exact name match, or a
/// prefix match with a word boundary (space/dash/dot/underscore after the
/// prefix) so "zcode" / "zcode-beta" match prefix "zcode" but "zcodeextra"
/// does not. Pure string logic — unit-testable on every platform.
//
// Only `#[cfg(windows)]` call sites (find_name_matching_exe / scan_windows_registry)
// reference this, so on non-Windows CI the compiler flags it dead — relax
// dead_code there so the cross-platform unit tests (which do call it) keep
// building everywhere while Windows stays strict. Mirrors the cfg-gate pattern
// on the Windows-only registry scan helpers.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn exe_stem_matches_hints(
    stem_lower: &str,
    names_lower: &[String],
    prefixes_lower: &[String],
) -> bool {
    if names_lower.iter().any(|n| n == stem_lower) {
        return true;
    }
    prefixes_lower.iter().any(|p| {
        stem_lower == *p
            || (stem_lower.starts_with(p)
                && stem_lower[p.len()..]
                    .chars()
                    .next()
                    .map(|c| !c.is_alphanumeric())
                    .unwrap_or(false))
    })
}

/// True for exe stems that are clearly uninstallers / helpers, not the app
/// itself. Used to reject e.g. "Uninstall ZCode.exe" when globbing an install
/// dir whose name also begins with the tool prefix.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_uninstaller_stem(stem_lower: &str) -> bool {
    stem_lower.starts_with("uninstall") || stem_lower.starts_with("unins")
}

/// Extract the executable path from a registry `UninstallString` value.
/// Handles quoted paths with trailing args (`"E:\ZCode\Uninstall ZCode.exe"
/// /currentuser`) and bare paths (`C:\App\uninst.exe`). Returns None for
/// non-exe uninstallers like `winget uninstall --product-code ...` (no .exe
/// token) so callers don't treat a winget CLI string as an install dir.
/// Pure string parsing — no filesystem access.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_uninstall_exe_path(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let token = if let Some(rest) = s.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        s.split_whitespace().next().unwrap_or("")
    };
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let is_exe = Path::new(token)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);
    if is_exe {
        Some(token.to_string())
    } else {
        None
    }
}

/// Glob an install directory for an exe whose name matches the tool's hints,
/// returning its full path. Used as a last-resort fallback when a registry
/// Uninstall entry exposes the install dir (via DisplayIcon's parent or
/// UninstallString) but neither DisplayIcon nor InstallLocation points at the
/// real app exe directly — e.g. electron-builder NSIS installs at a custom
/// path: DisplayIcon is a standalone `.ico`, InstallLocation is empty, and the
/// app exe sits next to `Uninstall <Name>.exe`. Uninstallers are excluded by
/// `is_uninstaller_stem` so we never return the uninstaller itself.
#[cfg(windows)]
fn find_name_matching_exe(
    dir: &Path,
    names_lower: &[String],
    prefixes_lower: &[String],
) -> Option<String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return None,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_windows_exe(name) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if is_uninstaller_stem(&stem) {
            continue;
        }
        if exe_stem_matches_hints(&stem, names_lower, prefixes_lower) {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

#[cfg(windows)]
fn scan_windows_registry(hints: &InstallHints) -> Option<String> {
    if hints.windows_display_names.is_empty() && hints.windows_display_name_prefixes.is_empty() {
        return None;
    }
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let names_lower: Vec<String> = hints
        .windows_display_names
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let prefixes_lower: Vec<String> = hints
        .windows_display_name_prefixes
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let publisher_filter = hints.windows_publisher.as_ref().map(|p| p.to_lowercase());

    // Standard Uninstall hives. WOW6432Node catches 32-bit installers on 64-bit Windows.
    let hives: &[(_, &str)] = &[
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ];

    for (hive, path) in hives {
        let key = RegKey::predef(*hive);
        let uninstall = match key.open_subkey_with_flags(path, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };
        for subkey_name in uninstall.enum_keys().filter_map(|x| x.ok()) {
            let entry = match uninstall.open_subkey(&subkey_name) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let display_name: String = entry.get_value("DisplayName").unwrap_or_default();
            if display_name.is_empty() {
                continue;
            }
            let dn_lower = display_name.to_lowercase();
            // EXACT case-insensitive match by default (windowsDisplayNames) —
            // we don't blanket substring-match because "Trae" would then match
            // "Trae CN" and the wrong card would claim a non-default install.
            // Apps whose DisplayName embeds a version ("WorkBuddy 4.24.2") opt
            // into PREFIX matching via windowsDisplayNamePrefixes instead.
            if !registry_display_name_matches(&dn_lower, &names_lower, &prefixes_lower) {
                continue;
            }
            if let Some(ref pub_filter) = publisher_filter {
                let pub_val: String = entry.get_value("Publisher").unwrap_or_default();
                if !pub_val.to_lowercase().contains(pub_filter) {
                    continue;
                }
            }
            // Collect install-dir candidates for the exe-glob fallback below.
            // Populated when DisplayIcon is a non-exe file (its parent dir is
            // still a strong install-dir signal) and/or UninstallString exposes
            // the install dir — covers installers (electron-builder NSIS at a
            // custom path) that leave InstallLocation empty.
            let mut install_dir_candidates: Vec<PathBuf> = Vec::new();

            // DisplayIcon usually points at the main exe directly. Strip ",N" icon-index
            // suffix if present (Windows convention for selecting an icon from a multi-icon exe).
            if let Ok(icon) = entry.get_value::<String, _>("DisplayIcon") {
                let icon_path = icon.split(',').next().unwrap_or(&icon).trim();
                let unquoted = icon_path.trim_matches('"');
                if !unquoted.is_empty() && Path::new(unquoted).exists() {
                    // Reject non-executable targets (standalone .ico files,
                    // .dll icon resources). Start-Process on one would open
                    // it with the default image viewer instead of launching
                    // the app — remember its parent dir as an install-dir
                    // candidate, then fall through.
                    if is_windows_exe(unquoted) {
                        log::info!(
                            "[InstallHints] Registry hit (DisplayIcon): {} → {}",
                            display_name,
                            unquoted
                        );
                        return Some(unquoted.to_string());
                    }
                    if let Some(parent) = Path::new(unquoted).parent() {
                        install_dir_candidates.push(parent.to_path_buf());
                    }
                    log::info!(
                        "[InstallHints] DisplayIcon is not an exe, using its dir as install-dir candidate: {} → {}",
                        display_name,
                        unquoted
                    );
                }
            }
            // Fallback: InstallLocation is a directory; we return it as-is.
            // detect_tool's caller knows how to handle both file and directory results.
            if let Ok(install_loc) = entry.get_value::<String, _>("InstallLocation") {
                let trimmed = install_loc.trim().trim_matches('"');
                if !trimmed.is_empty() && Path::new(trimmed).exists() {
                    log::info!(
                        "[InstallHints] Registry hit (InstallLocation): {} → {}",
                        display_name,
                        trimmed
                    );
                    return Some(trimmed.to_string());
                }
            }
            // Fallback 2: installers that leave InstallLocation empty but record
            // an UninstallString like `"E:\ZCode\Uninstall ZCode.exe" /currentuser`.
            // The uninstaller sits next to the real app exe, so its parent dir
            // is the install dir — glob it for a name-matching exe. This is what
            // makes a custom-path ZCode install (E:\ZCode\ZCode.exe) detectable
            // when DisplayIcon is a .ico and InstallLocation is blank.
            if let Ok(uninst) = entry.get_value::<String, _>("UninstallString") {
                if let Some(exe_path) = parse_uninstall_exe_path(&uninst) {
                    if let Some(parent) = Path::new(&exe_path).parent() {
                        if !install_dir_candidates.iter().any(|c| c == parent) {
                            install_dir_candidates.push(parent.to_path_buf());
                        }
                    }
                }
            }
            for dir in &install_dir_candidates {
                if let Some(exe) = find_name_matching_exe(dir, &names_lower, &prefixes_lower) {
                    log::info!(
                        "[InstallHints] Registry hit (exe glob in install dir): {} → {}",
                        display_name,
                        exe
                    );
                    return Some(exe);
                }
            }
        }
    }
    None
}

/// Read the clean `DisplayVersion` from the Windows registry Uninstall entry
/// matching `hints`. Desktop GUI apps don't answer `--version` on the CLI
/// (their `command` is empty), but their installer records a version here —
/// e.g. WorkBuddy's entry carries DisplayVersion="4.24.2". Same name/publisher
/// matching as `scan_windows_registry`; that path-finding walk is deliberately
/// left untouched, so this is a small parallel read rather than a refactor of
/// proven multi-tool detection code. Returns None when nothing matches or the
/// matched entry has no DisplayVersion.
#[cfg(windows)]
fn scan_windows_registry_version(hints: &InstallHints) -> Option<String> {
    if hints.windows_display_names.is_empty() && hints.windows_display_name_prefixes.is_empty() {
        return None;
    }
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let names_lower: Vec<String> = hints
        .windows_display_names
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let prefixes_lower: Vec<String> = hints
        .windows_display_name_prefixes
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let publisher_filter = hints.windows_publisher.as_ref().map(|p| p.to_lowercase());

    let hives: &[(_, &str)] = &[
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ];

    for (hive, path) in hives {
        let key = RegKey::predef(*hive);
        let uninstall = match key.open_subkey_with_flags(path, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };
        for subkey_name in uninstall.enum_keys().filter_map(|x| x.ok()) {
            let entry = match uninstall.open_subkey(&subkey_name) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let display_name: String = entry.get_value("DisplayName").unwrap_or_default();
            if display_name.is_empty() {
                continue;
            }
            let dn_lower = display_name.to_lowercase();
            if !registry_display_name_matches(&dn_lower, &names_lower, &prefixes_lower) {
                continue;
            }
            if let Some(ref pub_filter) = publisher_filter {
                let pub_val: String = entry.get_value("Publisher").unwrap_or_default();
                if !pub_val.to_lowercase().contains(pub_filter) {
                    continue;
                }
            }
            let version: String = entry.get_value("DisplayVersion").unwrap_or_default();
            let version = version.trim().to_string();
            if !version.is_empty() {
                log::info!(
                    "[InstallHints] Registry version hit: {} → {}",
                    display_name,
                    version
                );
                return Some(version);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn scan_macos_applications(hints: &InstallHints) -> Option<String> {
    let app_name = hints.macos_app_name.as_ref()?;
    let normalized = if app_name.ends_with(".app") {
        app_name.clone()
    } else {
        format!("{}.app", app_name)
    };
    // Standard install roots first — fast filesystem stat.
    for root in ["/Applications", "/Applications/Utilities"] {
        let candidate = PathBuf::from(root).join(&normalized);
        if candidate.exists() {
            log::info!("[InstallHints] macOS hit: {}", candidate.display());
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join("Applications").join(&normalized);
        if candidate.exists() {
            log::info!("[InstallHints] macOS hit (user): {}", candidate.display());
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    // Fallback: mdfind covers non-/Applications installs (e.g. ~/Tools/Foo.app).
    if let Ok(out) = std::process::Command::new("mdfind")
        .args(["-name", &normalized])
        .output()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let p = line.trim();
            if !p.is_empty() && p.ends_with(&normalized) && Path::new(p).exists() {
                log::info!("[InstallHints] mdfind hit: {}", p);
                return Some(p.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn scan_linux_desktop(hints: &InstallHints) -> Option<String> {
    if hints.linux_desktop_names.is_empty() {
        return None;
    }
    let names_lower: Vec<String> = hints
        .linux_desktop_names
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut search_dirs: Vec<PathBuf> = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        search_dirs.push(home.join(".local/share/applications"));
    }

    for dir in &search_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Parse the [Desktop Entry] Name= and Exec= keys (first occurrence wins).
            let mut name = String::new();
            let mut exec = String::new();
            for line in content.lines() {
                if name.is_empty() {
                    if let Some(v) = line.strip_prefix("Name=") {
                        name = v.trim().to_string();
                    }
                }
                if exec.is_empty() {
                    if let Some(v) = line.strip_prefix("Exec=") {
                        exec = v.trim().to_string();
                    }
                }
                if !name.is_empty() && !exec.is_empty() {
                    break;
                }
            }
            let name_lower = name.to_lowercase();
            let filename_lower = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            // Match against either visible Name= or the .desktop filename stem.
            let hit = names_lower
                .iter()
                .any(|n| name_lower == *n || filename_lower == *n);
            if !hit {
                continue;
            }
            // Exec= often contains %U/%F field codes — keep only the command itself.
            let exec_clean = exec
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"');
            if !exec_clean.is_empty() {
                log::info!("[InstallHints] .desktop hit: {} → {}", name, exec_clean);
                return Some(exec_clean.to_string());
            }
        }
    }
    None
}

/// Cross-platform dispatcher. Returns Some(path) if the install hints
/// found the tool on disk; None otherwise (caller falls through to next
/// detection step).
fn scan_install_hints(pc: &PathsConfig) -> Option<String> {
    let hints = pc.install_hints.as_ref()?;
    #[cfg(windows)]
    {
        scan_windows_registry(hints)
    }
    #[cfg(target_os = "macos")]
    {
        scan_macos_applications(hints)
    }
    #[cfg(target_os = "linux")]
    {
        scan_linux_desktop(hints)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = hints;
        None
    }
}

/// Whether a tool has a detector stronger than its config directory — i.e.
/// platform paths or install hints (registry / Applications / .desktop scan).
/// When this is true, a lingering config directory must NOT be read as
/// "installed": those detectors are authoritative and have already run in
/// `detect_tool`, so reaching the config-dir step means they all failed — the
/// app is uninstalled and the config is stale (most uninstallers leave
/// ~/.<tool> behind on purpose). Only tools with NO such detector may use the
/// config directory as proof of installation.
fn has_authoritative_detector(pc: &PathsConfig) -> bool {
    !get_platform_paths(&pc.paths).is_empty() || pc.install_hints.is_some()
}

/// Match an installed MSIX/Store package by identity, scanning `packages_dir`
/// (normally `%LOCALAPPDATA%\Packages`). A package family name is
/// `<Identity>_<PublisherHash>`; matching on the identity (not the full name)
/// keeps detection working when the package is re-signed under a new publisher
/// hash. Also accepts the `<Identity>Beta` channel sibling, so a beta-channel
/// install (e.g. `OpenAI.CodexBeta`) registers as present. Returns the matched
/// package's per-user data dir.
#[cfg(windows)]
fn match_installed_msix(
    packages_dir: &std::path::Path,
    launch_uri: &str,
) -> Option<std::path::PathBuf> {
    let aumid = launch_uri
        .strip_prefix("shell:AppsFolder\\")
        .or_else(|| launch_uri.strip_prefix("shell:AppsFolder/"))
        .unwrap_or(launch_uri);
    let pfn = aumid.split('!').next()?;
    let identity = pfn.rsplit_once('_').map(|(id, _)| id).unwrap_or(pfn);
    let beta = format!("{identity}Beta");
    for entry in std::fs::read_dir(packages_dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let dir_identity = name
            .rsplit_once('_')
            .map(|(id, _)| id)
            .unwrap_or(name.as_ref());
        if dir_identity == identity || dir_identity == beta.as_str() {
            return Some(entry.path());
        }
    }
    None
}

/// Detect if a tool is installed, returns executable path
async fn detect_tool(pc: &PathsConfig) -> Option<String> {
    // 0. Built-in tools (always installed)
    if pc.always_installed {
        return Some("built-in".to_string());
    }

    // 0.5. MSIX / Store apps (Windows): match an installed package under
    // %LOCALAPPDATA%\Packages by identity (publisher-hash-agnostic), also
    // accepting the `<Identity>Beta` channel — see match_installed_msix.
    #[cfg(windows)]
    if let Some(ref aumid) = pc.launch_uri {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let packages = std::path::PathBuf::from(local).join("Packages");
            if let Some(dir) = match_installed_msix(&packages, aumid) {
                return Some(normalize_for_display(dir.to_string_lossy().to_string()));
            }
        }
    }

    // 1. Check custom env var
    if let Some(ref env_var) = pc.env_var {
        if let Ok(custom_path) = std::env::var(env_var) {
            let expanded = expand_path(&custom_path);
            if expanded.exists() {
                return Some(expanded.to_string_lossy().to_string());
            }
        }
    }

    // 2. Check PATH for command
    if !pc.command.is_empty() {
        let found_in_path = platform::command_exists(&pc.command).await;
        if found_in_path {
            if pc.require_config_file {
                if config_file_exists(pc) {
                    let path = platform::get_command_path(&pc.command).await;
                    return path.or(Some(pc.command.clone()));
                }
                log::info!(
                    "[{}] Command found in PATH but config file missing",
                    pc.name
                );
            } else {
                let path = platform::get_command_path(&pc.command).await;
                return path.or(Some(pc.command.clone()));
            }
        }
    }

    // 2.5. Check Python module (pip-installed CLI tools)
    if let Some(ref py_module) = pc.python_module {
        let found = platform::python_module_exists(py_module).await;
        if found {
            log::info!(
                "[{}] Python module '{}' detected (python -m {})",
                pc.name,
                py_module,
                py_module
            );
            return Some(format!("python -m {}", py_module));
        }
    }

    // 3. Check platform-specific paths
    let platform_paths = get_platform_paths(&pc.paths);
    for p in &platform_paths {
        let expanded = expand_path(p);
        if expanded.exists() {
            if pc.require_config_file {
                if config_file_exists(pc) {
                    return Some(expanded.to_string_lossy().to_string());
                }
            } else {
                return Some(expanded.to_string_lossy().to_string());
            }
        }
    }

    // 3.5. Install-hints fallback — catch installs at non-default paths.
    // Windows scans the registry Uninstall hive; macOS checks /Applications +
    // mdfind; Linux scans .desktop files. Only triggers when the hardcoded
    // paths above missed, so default installs don't pay the lookup cost.
    if let Some(hit) = scan_install_hints(pc) {
        if pc.require_config_file {
            if config_file_exists(pc) {
                return Some(hit);
            }
        } else {
            return Some(hit);
        }
    }

    // 4. Check VS Code extension paths (glob matching)
    if let Some(ref ext_paths) = pc.extension_paths {
        let ext_platform = get_platform_paths(ext_paths);
        for pattern in &ext_platform {
            let expanded = expand_path(pattern);
            let base_dir = expanded.parent();
            let glob_part = expanded.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if let Some(base) = base_dir {
                if base.exists() {
                    if let Ok(entries) = fs::read_dir(base) {
                        let prefix = glob_part.replace('*', "");
                        for entry in entries.filter_map(|e| e.ok()) {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with(&prefix) {
                                let full_path = base.join(&name);
                                log::info!("[{}] Extension found: {:?}", pc.name, full_path);
                                return Some(full_path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. Detect by config directory existence (GUI desktop apps without a fixed
    // install location, e.g. install-anywhere desktop apps).
    //
    // A lingering config directory is a WEAK signal: most uninstallers leave
    // ~/.<tool> behind on purpose (user data is preserved across reinstalls).
    // So config-dir presence may only stand in for "installed" when there is NO
    // stronger detector to contradict it. If the tool defines platform paths OR
    // install hints (registry / Applications / .desktop scan), those are
    // authoritative and have ALREADY been checked above — reaching here means
    // they all failed, i.e. the app is uninstalled and the config dir is stale.
    // Only treat the config dir as proof of installation for tools that have no
    // other detector at all.
    if pc.detect_by_config_dir && !pc.config_dir.is_empty() {
        let has_authoritative = has_authoritative_detector(pc);

        let config_dir = expand_path(&pc.config_dir);
        let found_dir = if config_dir.exists() {
            Some(config_dir)
        } else {
            // Fall back to the alternate config dir (e.g. Windows
            // %LOCALAPPDATA%\hermes vs Unix ~/.hermes).
            pc.config_dir_alt
                .as_ref()
                .map(|alt| expand_path(alt))
                .filter(|dir| dir.exists())
        };

        if let Some(dir) = found_dir {
            if has_authoritative {
                log::info!(
                    "[{}] Config directory {:?} exists but no executable / install-hint match found — app likely uninstalled (stale config), skipping",
                    pc.name,
                    dir
                );
            } else {
                log::info!(
                    "[{}] Config directory found: {:?}, treated as installed",
                    pc.name,
                    dir
                );
                return Some(dir.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Check if the tool's config file exists
fn config_file_exists(pc: &PathsConfig) -> bool {
    if !pc.config_file.is_empty() {
        let main_config = expand_path(&pc.config_file);
        if main_config.exists() {
            return true;
        }
    }
    if let Some(ref alt) = pc.config_file_alt {
        let alt_config = expand_path(alt);
        if alt_config.exists() {
            return true;
        }
    }
    false
}

/// Get the skills path for a tool
async fn find_skills_path(pc: &PathsConfig) -> Option<String> {
    let sp = pc.skills_path.as_ref()?;

    // 1. Environment variable
    if let Some(ref env_var) = sp.env_var {
        if let Ok(path) = std::env::var(env_var) {
            return Some(path);
        }
    }

    // 2. Platform-specific paths
    let platform_paths = {
        #[cfg(target_os = "windows")]
        {
            sp.win32.clone().unwrap_or_default()
        }
        #[cfg(target_os = "macos")]
        {
            sp.darwin.clone().unwrap_or_default()
        }
        #[cfg(target_os = "linux")]
        {
            sp.linux.clone().unwrap_or_default()
        }
        #[cfg(target_os = "android")]
        {
            Vec::<String>::new()
        }
    };
    for p in &platform_paths {
        let expanded = expand_path(p);
        if expanded.exists() {
            return Some(expanded.to_string_lossy().to_string());
        }
    }

    // 3. npm global module
    if let Some(ref npm_module) = sp.npm_module {
        #[cfg(windows)]
        let npm_output = {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            tokio::process::Command::new("npm")
                .args(["root", "-g"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .await
        };
        #[cfg(not(windows))]
        let npm_output = tokio::process::Command::new("npm")
            .args(["root", "-g"])
            .output()
            .await;
        if let Ok(output) = npm_output {
            let global_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !global_root.is_empty() {
                let module_path: PathBuf = npm_module
                    .split('/')
                    .fold(PathBuf::from(&global_root), |acc, part| acc.join(part));
                if module_path.exists() {
                    return Some(module_path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// Count installed skills in a directory
fn count_skills(skills_path: &str) -> u32 {
    let path = Path::new(skills_path);
    if !path.exists() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                        && !e.file_name().to_string_lossy().starts_with('.')
                })
                .count() as u32
        })
        .unwrap_or(0)
}

/// Read the current active model from a tool's config.
/// Delegates to tool_config_manager which has proper readers for every tool type
/// (generic JSON, echobird relay, YAML, TOML, custom formats, etc.)
async fn read_active_model(def: &ToolDefinition) -> Option<String> {
    use crate::services::tool_config_manager;

    let info = tool_config_manager::get_tool_model_info(&def.id).await?;

    // Prefer model ID; fall back to display name
    info.model.or(info.name)
}

/// Parse category string to ToolCategory enum
fn parse_category(s: &str) -> ToolCategory {
    match s {
        "Agents" | "CLI Agent" | "AgentOS" => ToolCategory::Agents,
        "IDE" => ToolCategory::IDE,
        "CLI Code" | "CLI" => ToolCategory::CLI,
        "AutoTrading" => ToolCategory::AutoTrading,
        "Game" => ToolCategory::Game,
        "Desktop" => ToolCategory::Desktop,
        "Utility" => ToolCategory::Utility,
        "Science" => ToolCategory::Science,
        _ => ToolCategory::Custom,
    }
}

// ─── JSON nested value helpers (mirrors tools/utils.ts) ───

/// Get a nested value from a JSON object by dot-separated path (e.g. "env.OPENAI_API_KEY")
pub fn get_nested_value(obj: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = obj;
    for part in &parts {
        current = current.get(*part)?;
    }
    Some(current.clone())
}

/// Set a nested value in a JSON object by dot-separated path
pub fn set_nested_value(obj: &mut serde_json::Value, path: &str, value: serde_json::Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = obj;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current[*part] = value;
            return;
        }
        if !current.get(*part).map(|v| v.is_object()).unwrap_or(false) {
            current[*part] = serde_json::json!({});
        }
        current = current.get_mut(*part).unwrap();
    }
}

/// Delete a nested value from a JSON object by dot-separated path
pub fn delete_nested_value(obj: &mut serde_json::Value, path: &str) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }
    let mut current = obj;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Some(map) = current.as_object_mut() {
                map.remove(*part);
            }
            return;
        }
        match current.get_mut(*part) {
            Some(next) if next.is_object() => current = next,
            _ => return,
        }
    }
}

// ─── Global tool definitions cache ───

use std::sync::Mutex;

static TOOL_DEFINITIONS: Mutex<Option<Vec<ToolDefinition>>> = Mutex::new(None);

/// Get or load tool definitions (cached)
fn get_definitions() -> Vec<ToolDefinition> {
    let mut cache = TOOL_DEFINITIONS.lock().unwrap();
    if cache.is_none() {
        *cache = Some(load_tool_definitions());
    }
    cache.as_ref().unwrap().clone()
}

/// Build the pre-filled seed for the user override file: every overridable
/// tool (built-in games excluded) paired with its bundled candidate paths for
/// the CURRENT OS, expanded to concrete locations and normalised to forward
/// slashes so they read cleanly in JSON and stay easy to edit. The user opens
/// this already-populated list and just corrects the one path that's wrong —
/// no format to learn, no line to author from scratch. Sorted by tool id for a
/// stable, scannable file.
///
/// Loads defs fresh with overrides OFF (not the cached `get_definitions`): the
/// seed must be pure bundled defaults, so re-creating the file after a reset
/// can't fold the user's own overrides back in as "defaults".
pub fn default_override_seed() -> Vec<(String, Vec<String>)> {
    let mut defs = load_tool_definitions_with_overrides(false);
    defs.sort_by(|a, b| a.id.cmp(&b.id));

    let mut out = Vec::new();
    for def in defs {
        // Built-in tools (Reversi, Translator, ...) are always installed and
        // have no real install path to override — skip them.
        if def.paths_config.always_installed {
            continue;
        }
        let mut paths: Vec<String> = Vec::new();
        for p in get_platform_paths(&def.paths_config.paths) {
            // Forward slashes (not normalize_for_display's backslashes) so the
            // JSON has no escaped "\\" and reads naturally; expand_path/exists
            // accept "/" on Windows too.
            let concrete = expand_path(&p).to_string_lossy().replace('\\', "/");
            if !concrete.is_empty() && !paths.contains(&concrete) {
                paths.push(concrete);
            }
        }
        out.push((def.id, paths));
    }
    out
}

/// Merge the default-override seed into an existing `~/.echobird/tool-paths.json`'s
/// parsed contents, returning the map to write back (or `None` when the file is
/// already complete / unreadable, so the caller leaves it alone).
///
/// `existing` is:
/// - `Some(empty object)` to force a full seed (file missing — every tool is
///   "missing", so all are inserted);
/// - `Some(object)` to self-heal an existing file that predates a tool shipped
///   in a later release (e.g. Kimi Code, added in v5.4.3);
/// - `None` when the existing file couldn't be parsed as a JSON object — the
///   user's malformed-but-edited file is left untouched rather than clobbered.
///
/// Only ADDS tool ids the seed provides that the file is missing. Existing
/// entries, user edits, and "_"-prefixed note keys (ignored by the scanner —
/// see `load_user_path_overrides`) are preserved verbatim, so a user's
/// customizations always survive an EchoBird update that ships new tools.
pub fn merge_override_seed(
    existing: Option<&serde_json::Value>,
    seed: &[(String, Vec<String>)],
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut map = match existing? {
        serde_json::Value::Object(m) => m.clone(),
        _ => return None,
    };
    let mut changed = false;
    for (tool_id, paths) in seed {
        if !map.contains_key(tool_id) {
            map.insert(
                tool_id.clone(),
                serde_json::Value::Array(
                    paths
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            changed = true;
        }
    }
    if changed {
        Some(map)
    } else {
        None
    }
}

/// Get the config mapping for a specific tool
pub fn get_tool_config_mapping(tool_id: &str) -> Option<(ToolDefinition, PathBuf)> {
    let defs = get_definitions();
    defs.into_iter().find(|d| d.id == tool_id).map(|def| {
        let config_path = expand_path(&def.config_mapping.config_file);
        (def, config_path)
    })
}

/// Get the CLI command for a tool (from paths.json "command" field)
pub fn get_tool_command(tool_id: &str) -> Option<String> {
    let defs = get_definitions();
    defs.iter().find(|d| d.id == tool_id).and_then(|def| {
        let cmd = &def.paths_config.command;
        if cmd.is_empty() {
            None
        } else {
            Some(cmd.clone())
        }
    })
}

/// Get the explicit start command for launching a tool (from paths.json "startCommand" field).
/// Only returns a value if startCommand is explicitly defined �?does NOT fall back to "command"
/// (which is used for detection only). Matches old Electron getStartCommand() behavior.
pub fn get_tool_start_command(tool_id: &str) -> Option<String> {
    let defs = get_definitions();
    defs.iter().find(|d| d.id == tool_id).and_then(|def| {
        def.paths_config
            .start_command
            .as_ref()
            .filter(|sc| !sc.is_empty())
            .cloned()
    })
}

/// Get the executable path for a GUI tool (checks platform-specific paths from paths.json)
pub fn get_tool_exe_path(tool_id: &str) -> Option<String> {
    let defs = get_definitions();
    let def = defs.iter().find(|d| d.id == tool_id)?;
    let platform_paths = get_platform_paths(&def.paths_config.paths);
    for p in &platform_paths {
        let expanded = expand_path(p);
        if expanded.exists() {
            return Some(expanded.to_string_lossy().to_string());
        }
    }
    // Fallback: registry install-hints (Squirrel apps like Claude Desktop
    // where Claude.exe lives under app-<version>/). When the registry only
    // has InstallLocation (a directory), join the declared exe image name.
    let hit = scan_install_hints(&def.paths_config)?;
    let hit_path = std::path::Path::new(&hit);
    if hit_path.is_file() {
        // Defense-in-depth: scan_windows_registry already gates on .exe, but
        // reject any non-executable file here too so a future regression in
        // the registry path can't slip a .ico/.dll through to Start-Process
        // (which would open it with the default image viewer, not launch it).
        #[cfg(windows)]
        {
            if !is_windows_exe(&hit) {
                log::warn!(
                    "[InstallHints] Skipping non-exe path for {}: {}",
                    tool_id,
                    hit
                );
                return None;
            }
        }
        return Some(hit);
    }
    if hit_path.is_dir() {
        for name in filenames_of(&platform_paths) {
            let candidate = hit_path.join(&name);
            if candidate.is_file() {
                log::info!(
                    "[InstallHints] resolved {} exe via InstallLocation + image name: {}",
                    tool_id,
                    candidate.display()
                );
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Like [`get_tool_exe_path`] but ONLY checks the explicit platform paths,
/// skipping the registry install-hints fallback. The launcher uses this to
/// decide whether a *directly launchable* exe exists on disk (e.g. a Squirrel
/// install of Claude Desktop at %LOCALAPPDATA%\AnthropicClaude\Claude.exe)
/// that should be preferred over a Store AUMID. A true MSIX/Store install has
/// no exe at the declared paths (it lives under WindowsApps), so this returns
/// None and the caller keeps using the AUMID — no regression for Store
/// installs.
pub fn get_tool_declared_exe_path(tool_id: &str) -> Option<String> {
    let defs = get_definitions();
    let def = defs.iter().find(|d| d.id == tool_id)?;
    let platform_paths = get_platform_paths(&def.paths_config.paths);
    for p in &platform_paths {
        let expanded = expand_path(p);
        if expanded.exists() {
            return Some(expanded.to_string_lossy().to_string());
        }
    }
    None
}

/// True if the tool is a GUI desktop app whose provider config EchoBird
/// manages — paths.json `category: "Desktop"` AND not `noModelConfig`. These
/// load provider config at startup, so EchoBird kills + relaunches them on
/// launch; otherwise switching the model while the app is open silently fails
/// (the running instance keeps the old config). Deliberately excluded:
///   • CLI tools — never `category: "Desktop"`, and they re-read config every
///     invocation anyway.
///   • No-model-config desktop viewers (Gemini Desktop, Coffee CLI) — EchoBird
///     doesn't switch their provider, so a restart would pick up nothing.
pub fn is_managed_desktop_tool(tool_id: &str) -> bool {
    get_definitions()
        .iter()
        .find(|d| d.id == tool_id)
        .map(|d| {
            d.paths_config.category.eq_ignore_ascii_case("desktop")
                && !d.paths_config.no_model_config
        })
        .unwrap_or(false)
}

/// Candidate process image names for a desktop tool on the current OS — the
/// filenames of its declared exe paths (e.g. "Claude.exe" / "Codex.exe" on
/// Windows, "Codex" on macOS). Used to terminate instances the *user* launched
/// (which we have no tracked PID for) before relaunching with fresh config.
/// Derived from paths.json so it needs no hardcoding, and matches MSIX/Store
/// installs too — their running image name equals the declared exe filename.
pub fn get_tool_process_names(tool_id: &str) -> Vec<String> {
    let defs = get_definitions();
    let Some(def) = defs.iter().find(|d| d.id == tool_id) else {
        return Vec::new();
    };
    filenames_of(&get_platform_paths(&def.paths_config.paths))
}

/// Pure helper: unique filenames extracted from a list of full paths.
fn filenames_of(paths: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for p in paths {
        if let Some(name) = std::path::Path::new(p).file_name().and_then(|n| n.to_str()) {
            let name = name.to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Check if a tool is a VS Code extension (detected via extensionPaths)
pub fn is_vscode_extension(tool_id: &str) -> bool {
    let defs = get_definitions();
    defs.iter()
        .find(|d| d.id == tool_id)
        .map(|def| def.paths_config.extension_paths.is_some())
        .unwrap_or(false)
}

// ─── Main entry point ───

/// Prefer the install catalog's homepage over legacy website values, which
/// sometimes point at a repository. A repository is the final fallback.
fn resolve_tool_website(website: Option<&str>, install_ref: Option<&str>) -> Option<String> {
    let install = install_ref.and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());
    let url = [
        install
            .as_ref()
            .and_then(|value| value["homepage"].as_str()),
        website,
        install.as_ref().and_then(|value| value["github"].as_str()),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|url| !url.is_empty())
    .map(str::to_owned);
    url
}

/// Scan a single tool definition — runs detection, skills, version, and model reads.
/// Extracted so scan_tools() can run all tools concurrently via tokio::spawn.
async fn scan_single_tool(def: ToolDefinition) -> DetectedTool {
    let pc = &def.paths_config;
    let installed_path = detect_tool(pc).await;
    let installed = installed_path.is_some();

    let mut skills_path_str = None;
    let mut skills_count = 0u32;
    let mut version = pc.version.clone();

    if installed {
        if let Some(sp) = find_skills_path(pc).await {
            skills_count = count_skills(&sp);
            skills_path_str = Some(sp);
        }
        if skills_path_str.is_none() {
            if let Some(ref rel_path) = pc.default_skills_path {
                let default_path = PathBuf::from(&def.tool_dir).join(rel_path);
                if default_path.exists() {
                    let p = default_path.to_string_lossy().to_string();
                    skills_count = count_skills(&p);
                    skills_path_str = Some(p);
                }
            }
        }
        // Skip --version probe for desktop GUI apps: their binary doesn't
        // implement a CLI fast-path for --version, so invoking it just opens
        // the main window. noModelConfig is the right gate — it's already the
        // marker for "this is a launch-only desktop app, not a CLI we query".
        if version.is_none() && !pc.command.is_empty() && !pc.no_model_config {
            version = platform::get_version(&pc.command).await;
        }
        // Desktop GUI apps have no CLI `--version` (their `command` is empty),
        // but Windows records a clean DisplayVersion in the registry Uninstall
        // entry. When the tool is registry-detectable (install hints present)
        // and we still have no version, read it — so WorkBuddy and, equally,
        // Claude/Codex/Gemini Desktop surface their version on the card.
        #[cfg(windows)]
        if version.is_none() {
            if let Some(ref hints) = pc.install_hints {
                version = scan_windows_registry_version(hints);
            }
        }
    }

    let active_model = if installed {
        read_active_model(&def).await
    } else {
        None
    };

    let config_path = if installed && !def.config_mapping.config_file.is_empty() {
        let cp = expand_path(&def.config_mapping.config_file);
        Some(normalize_for_display(cp.to_string_lossy().to_string()))
    } else {
        None
    };

    let detected_path = if pc.always_installed {
        // Bundled tools (Reversi, Translator, etc.). For most of them launch_file
        // points at an HTML file that doesn't actually live in tools/<id>/ —
        // the real HTML is in public/tools/<id>.html and launch_file is just a
        // truthy flag. So we fall back to def.tool_dir (e.g. <resource>/tools/
        // reversi/), which is what users want to see on the card: the folder
        // containing paths.json + models.json they can read as a Vibe Coding
        // reference when authoring their own AI project under "我的AI项目".
        if let Some(ref launch) = pc.launch_file {
            let tools_dir = find_tools_dir().unwrap_or_default();
            let launch_path = tools_dir.join(&def.id).join(launch);
            if launch_path.exists() {
                Some(normalize_for_display(
                    launch_path.to_string_lossy().to_string(),
                ))
            } else {
                Some(normalize_for_display(def.tool_dir.clone()))
            }
        } else {
            Some(normalize_for_display(def.tool_dir.clone()))
        }
    } else {
        // PATH-discovered CLI tools (e.g. `which coffee-cli` under Git Bash)
        // return forward-slash paths. Normalize so the card shows `\` on
        // Windows — same treatment the always_installed branch already gets.
        installed_path.map(normalize_for_display)
    };

    DetectedTool {
        id: def.id.clone(),
        name: pc.name.clone(),
        category: parse_category(&pc.category),
        official: true,
        installed,
        detected_path,
        config_path,
        skills_path: skills_path_str,
        version,
        installed_skills_count: Some(skills_count),
        active_model,
        website: resolve_tool_website(
            pc.website.as_deref(),
            super::bundled_assets::get_install_ref(&def.id),
        ),
        api_protocol: if pc.api_protocol.is_empty() {
            None
        } else {
            Some(pc.api_protocol.clone())
        },
        launch_file: pc.launch_file.clone(),
        names: pc.names.clone(),
        start_command: pc.start_command.clone(),
        command: if pc.command.is_empty() {
            None
        } else {
            Some(pc.command.clone())
        },
        no_model_config: pc.no_model_config,
        no_cwd_prompt: pc.no_cwd_prompt,
        launch_uri: pc.launch_uri.clone(),
    }
}

/// Get the launch URI (e.g. "shell:AppsFolder\\<AUMID>") for an MSIX/Store app.
pub fn get_tool_launch_uri(tool_id: &str) -> Option<String> {
    let defs = get_definitions();
    defs.iter()
        .find(|d| d.id == tool_id)?
        .paths_config
        .launch_uri
        .clone()
}

/// Scan all installed tools — runs all detections in parallel for fast completion.
pub async fn scan_tools() -> Vec<DetectedTool> {
    // Clear cache to pick up newly added tool directories
    {
        let mut cache = TOOL_DEFINITIONS.lock().unwrap();
        *cache = None;
    }
    let definitions = get_definitions();

    // Spawn a task per tool so all detections run concurrently
    let mut handles = Vec::with_capacity(definitions.len());
    for def in definitions {
        handles.push(tokio::spawn(async move { scan_single_tool(def).await }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(tool) => results.push(tool),
            Err(e) => log::warn!("[ToolManager] Tool scan task panicked: {}", e),
        }
    }

    log::info!("[ToolManager] Scan complete: {} tools found", results.len());
    results
}

#[cfg(test)]
mod tests {
    use super::{
        exe_stem_matches_hints, has_authoritative_detector, is_uninstaller_stem,
        merge_override_seed, parse_uninstall_exe_path, registry_display_name_matches,
    };
    // is_windows_exe is #[cfg(windows)]-gated, so the import must be too —
    // otherwise the Linux CI build fails with an unresolved import even
    // though the only test using it is also #[cfg(windows)].
    #[cfg(windows)]
    use super::is_windows_exe;
    use crate::models::tool::PathsConfig;

    #[test]
    fn tool_website_prefers_homepage_over_legacy_repository() {
        assert_eq!(
            super::resolve_tool_website(
                Some("https://github.com/owner/tool"),
                Some(r#"{"homepage":"https://tool.example/","github":"https://github.com/owner/tool"}"#),
            ).as_deref(),
            Some("https://tool.example/")
        );
    }

    #[test]
    fn tool_website_keeps_configured_site_without_catalog_homepage() {
        assert_eq!(
            super::resolve_tool_website(
                Some("https://tool.example/"),
                Some(r#"{"github":"https://github.com/owner/tool"}"#),
            )
            .as_deref(),
            Some("https://tool.example/")
        );
    }

    #[test]
    fn tool_website_falls_back_to_github_when_homepage_is_missing_or_blank() {
        for install_ref in [
            r#"{"github":"https://github.com/owner/tool"}"#,
            r#"{"homepage":"  ","github":"https://github.com/owner/tool"}"#,
        ] {
            assert_eq!(
                super::resolve_tool_website(Some(" "), Some(install_ref)).as_deref(),
                Some("https://github.com/owner/tool")
            );
        }
    }

    #[test]
    fn tool_website_has_no_link_without_website_or_repository() {
        assert_eq!(super::resolve_tool_website(None, None), None);
        assert_eq!(
            super::resolve_tool_website(None, Some(r#"{"docs":"https://docs.example/"}"#)),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn msix_match_detects_beta_channel_ignoring_publisher_hash() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let pkgs = std::env::temp_dir().join(format!(
            "echobird_msix_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        // Beta channel installed, with a publisher hash different from the
        // hardcoded stable one in the launch URI — must still match.
        let beta = pkgs.join("OpenAI.CodexBeta_zzzzzzzzzzzzz");
        std::fs::create_dir_all(&beta).unwrap();

        let got =
            super::match_installed_msix(&pkgs, "shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App");
        assert_eq!(got.as_deref(), Some(beta.as_path()));

        let _ = std::fs::remove_dir_all(&pkgs);
    }

    #[cfg(windows)]
    #[test]
    fn msix_match_returns_none_when_no_codex_package() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let pkgs = std::env::temp_dir().join(format!(
            "echobird_msix_none_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(pkgs.join("Microsoft.Unrelated_abcdefghijklm")).unwrap();

        let got =
            super::match_installed_msix(&pkgs, "shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App");
        assert!(got.is_none());

        let _ = std::fs::remove_dir_all(&pkgs);
    }

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn paths_config_from(json: serde_json::Value) -> PathsConfig {
        serde_json::from_value(json).expect("valid PathsConfig json")
    }

    #[test]
    fn filenames_of_strips_dirs_and_dedupes() {
        // Two install locations share the same exe filename → one entry; a name
        // with a space ("OpenCode Beta.exe") survives intact.
        let paths = v(&[
            "C:/Users/x/AppData/Local/AnthropicClaude/Claude.exe",
            "C:/Users/x/AppData/Local/Programs/Claude/Claude.exe",
            "C:/Users/x/AppData/Local/Programs/OpenCode Beta/OpenCode Beta.exe",
        ]);
        assert_eq!(
            super::filenames_of(&paths),
            v(&["Claude.exe", "OpenCode Beta.exe"])
        );
        assert!(super::filenames_of(&[]).is_empty());
    }

    // ── config-dir detection: a lingering config dir must not count as
    //    "installed" when a stronger detector exists and already failed.
    //    Regression: WorkBuddy showed installed after uninstall because
    //    ~/.workbuddy survived and was treated as proof of installation. ──

    #[test]
    fn install_hints_are_authoritative_over_config_dir() {
        // WorkBuddy shape: no platform paths (installs anywhere → registry is
        // the real detector), installHints present. A leftover config dir must
        // NOT override the registry's "uninstalled" verdict.
        let pc = paths_config_from(serde_json::json!({
            "name": "WorkBuddy",
            "category": "Agents",
            "detectByConfigDir": true,
            "configDir": "~/.workbuddy",
            "paths": { "win32": [], "darwin": [], "linux": [] },
            "installHints": { "windowsDisplayNamePrefixes": ["WorkBuddy"] }
        }));
        assert!(has_authoritative_detector(&pc));
    }

    #[test]
    fn platform_paths_are_authoritative_over_config_dir() {
        // Tools with real exe paths (e.g. hermes/openclaw) — paths are the
        // detector; a stale config dir alone never means installed.
        let pc = paths_config_from(serde_json::json!({
            "name": "Hermes",
            "category": "Agents",
            "detectByConfigDir": true,
            "configDir": "~/.hermes",
            "paths": {
                "win32": ["%USERPROFILE%/.hermes/bin/hermes.exe"],
                "darwin": ["~/.hermes/bin/hermes"],
                "linux": ["~/.hermes/bin/hermes"]
            }
        }));
        assert!(has_authoritative_detector(&pc));
    }

    #[test]
    fn config_only_tool_has_no_authoritative_detector() {
        // No paths and no installHints → the config dir is the only signal,
        // so it IS allowed to imply installation.
        let pc = paths_config_from(serde_json::json!({
            "name": "ConfigOnly",
            "category": "Agents",
            "detectByConfigDir": true,
            "configDir": "~/.configonly",
            "paths": { "win32": [], "darwin": [], "linux": [] }
        }));
        assert!(!has_authoritative_detector(&pc));
    }

    #[test]
    fn exact_name_matches() {
        let names = v(&["trae cn"]);
        assert!(registry_display_name_matches("trae cn", &names, &[]));
    }

    #[test]
    fn exact_name_does_not_substring_match() {
        // The whole reason exact match exists: "trae" must NOT match "trae cn".
        let names = v(&["trae"]);
        assert!(!registry_display_name_matches("trae cn", &names, &[]));
    }

    #[test]
    fn prefix_matches_versioned_display_name() {
        // WorkBuddy's registry DisplayName is "WorkBuddy 4.24.2".
        let prefixes = v(&["workbuddy"]);
        assert!(registry_display_name_matches(
            "workbuddy 4.24.2",
            &[],
            &prefixes
        ));
    }

    #[test]
    fn prefix_matches_bare_name_without_version() {
        let prefixes = v(&["workbuddy"]);
        assert!(registry_display_name_matches("workbuddy", &[], &prefixes));
    }

    #[test]
    fn prefix_requires_word_boundary_not_raw_substring() {
        // "workbuddy" prefix must NOT match "workbuddyextra" (no space) — the
        // boundary guard is what keeps prefix matching from over-firing.
        let prefixes = v(&["workbuddy"]);
        assert!(!registry_display_name_matches(
            "workbuddyextra 1.0",
            &[],
            &prefixes
        ));
    }

    #[test]
    fn no_hints_never_matches() {
        assert!(!registry_display_name_matches("anything 1.2.3", &[], &[]));
    }

    // ── exe-glob fallback: a registry Uninstall entry whose DisplayIcon is a
    //    standalone .ico and whose InstallLocation is blank (electron-builder
    //    NSIS at a custom path, e.g. ZCode at E:\ZCode) must still be detected
    //    by deriving the install dir from DisplayIcon's parent / UninstallString
    //    and globbing it for a name-matching exe. These test the pure-string
    //    pieces on every platform; the dir walk itself is Windows-only. ──

    #[test]
    fn parse_uninstall_string_quoted_with_args() {
        // ZCode's real value: `"E:\ZCode\Uninstall ZCode.exe" /currentuser`
        let got = parse_uninstall_exe_path(r#""E:\ZCode\Uninstall ZCode.exe" /currentuser"#);
        assert_eq!(got.as_deref(), Some(r"E:\ZCode\Uninstall ZCode.exe"));
    }

    #[test]
    fn parse_uninstall_string_bare() {
        let got = parse_uninstall_exe_path(r"C:\App\uninst.exe");
        assert_eq!(got.as_deref(), Some(r"C:\App\uninst.exe"));
    }

    #[test]
    fn parse_uninstall_string_rejects_winget_cli() {
        // winget uninstall strings have no .exe token — must not be treated as
        // an install dir (would otherwise resolve to a bogus path).
        let got = parse_uninstall_exe_path(
            "winget uninstall --product-code Anthropic.ClaudeCode_8wekyb3d8bbwe",
        );
        assert!(got.is_none());
    }

    #[test]
    fn parse_uninstall_string_rejects_empty() {
        assert!(parse_uninstall_exe_path("").is_none());
        assert!(parse_uninstall_exe_path("   ").is_none());
    }

    #[test]
    fn exe_stem_matches_exact_prefix() {
        // ZCode.exe stem "zcode" vs prefix "ZCode".
        let prefixes = v(&["zcode"]);
        assert!(exe_stem_matches_hints("zcode", &[], &prefixes));
    }

    #[test]
    fn exe_stem_matches_exact_name() {
        let names = v(&["trae cn"]);
        assert!(exe_stem_matches_hints("trae cn", &names, &[]));
    }

    #[test]
    fn exe_stem_prefix_matches_separator_suffix() {
        // "zcode-beta" matches prefix "zcode" (boundary after prefix).
        let prefixes = v(&["zcode"]);
        assert!(exe_stem_matches_hints("zcode-beta", &[], &prefixes));
        assert!(exe_stem_matches_hints("zcode beta", &[], &prefixes));
        assert!(exe_stem_matches_hints("zcode.beta", &[], &prefixes));
    }

    #[test]
    fn exe_stem_prefix_rejects_no_boundary() {
        // "zcodeextra" must NOT match prefix "zcode" — no word boundary.
        let prefixes = v(&["zcode"]);
        assert!(!exe_stem_matches_hints("zcodeextra", &[], &prefixes));
    }

    #[test]
    fn uninstaller_stem_is_flagged() {
        // "Uninstall ZCode.exe" stem must be excluded from the glob result.
        assert!(is_uninstaller_stem("uninstall zcode"));
        assert!(is_uninstaller_stem("unins000"));
        assert!(!is_uninstaller_stem("zcode"));
    }

    #[cfg(windows)]
    #[test]
    fn find_name_matching_exe_picks_app_not_uninstaller() {
        // Mimic a custom-path ZCode install dir: the real app exe sits next to
        // an `Uninstall ZCode.exe` uninstaller and a standalone .ico. The glob
        // must return the app exe and never the uninstaller.
        let dir = std::env::temp_dir().join(format!("echobird_zcode_test_{}", line!()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ZCode.exe"), b"").unwrap();
        std::fs::write(dir.join("Uninstall ZCode.exe"), b"").unwrap();
        std::fs::write(dir.join("uninstallerIcon.ico"), b"").unwrap();

        let prefixes = v(&["zcode"]);
        let got = super::find_name_matching_exe(&dir, &[], &prefixes);
        assert_eq!(
            got.as_deref().and_then(|p| {
                let p = std::path::Path::new(p);
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            }),
            Some("ZCode.exe".to_string())
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "machine-specific: only passes where ZCode is installed at a custom path"]
    fn real_registry_finds_custom_path_zcode() {
        // End-to-end: the real ZCode Uninstall entry has DisplayIcon=.ico,
        // InstallLocation blank, UninstallString pointing at E:\ZCode. The scan
        // must derive E:\ZCode and return E:\ZCode\ZCode.exe (or whichever
        // name-matching exe lives there).
        use crate::models::tool::InstallHints;
        let hints = InstallHints {
            windows_display_name_prefixes: v(&["ZCode"]),
            ..Default::default()
        };
        let got = super::scan_windows_registry(&hints);
        let path = got.expect("ZCode registry entry should resolve to an exe");
        assert!(
            super::is_windows_exe(&path),
            "resolved path must be an exe: {path}"
        );
        assert!(
            path.to_lowercase().ends_with(r"\zcode.exe"),
            "expected ...\\ZCode.exe, got {path}"
        );
    }

    // ── tool-paths.json self-heal: a file seeded before a tool shipped (e.g.
    //    Kimi Code in v5.4.3) must gain that tool's default-path entry on the
    //    next open, while existing user edits + "_" note keys survive. ──

    fn override_seed() -> Vec<(String, Vec<String>)> {
        vec![
            ("claudecode".to_string(), v(&["~/.claude/local/claude"])),
            ("kimicode".to_string(), v(&["~/.kimi-code/bin/kimi"])),
        ]
    }

    fn arr(items: &[&str]) -> serde_json::Value {
        serde_json::Value::Array(
            items
                .iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
        )
    }

    #[test]
    fn missing_file_is_seeded_with_every_tool() {
        // No existing file → empty object → every seed tool inserted.
        let map = merge_override_seed(
            Some(&serde_json::Value::Object(serde_json::Map::new())),
            &override_seed(),
        )
        .expect("should write a full seed");
        assert!(map.contains_key("claudecode"));
        assert!(map.contains_key("kimicode"));
    }

    #[test]
    fn existing_file_gains_only_missing_tools() {
        // File predates kimicode: claudecode present (with a user edit) + a
        // "_" note, no kimi. Kimi must be added; the rest preserved verbatim.
        let mut existing = serde_json::Map::new();
        existing.insert("claudecode".to_string(), arr(&["/my/custom/claude"]));
        existing.insert(
            "_note".to_string(),
            serde_json::Value::String("hand-edited".to_string()),
        );

        let map = merge_override_seed(Some(&serde_json::Value::Object(existing)), &override_seed())
            .expect("should add the missing kimi entry");

        assert!(map.contains_key("kimicode"));
        // User's custom claudecode path NOT overwritten with the default.
        assert_eq!(map.get("claudecode"), Some(&arr(&["/my/custom/claude"])));
        // "_" note preserved (scanner ignores it; we must not drop it).
        assert!(map.contains_key("_note"));
    }

    #[test]
    fn up_to_date_file_is_left_untouched() {
        let mut existing = serde_json::Map::new();
        existing.insert("claudecode".to_string(), arr(&[]));
        existing.insert("kimicode".to_string(), arr(&[]));
        assert!(
            merge_override_seed(Some(&serde_json::Value::Object(existing)), &override_seed())
                .is_none()
        );
    }

    #[test]
    fn unreadable_or_non_object_file_is_left_untouched() {
        // No parseable object → don't risk clobbering the user's file.
        assert!(merge_override_seed(None, &override_seed()).is_none());
        assert!(
            merge_override_seed(Some(&serde_json::Value::Array(vec![])), &override_seed())
                .is_none()
        );
        assert!(merge_override_seed(
            Some(&serde_json::Value::String("oops".into())),
            &override_seed()
        )
        .is_none());
    }

    #[cfg(windows)]
    #[test]
    fn is_windows_exe_recognizes_exe_and_rejects_non_executables() {
        // Exe — case-insensitive on the extension.
        assert!(is_windows_exe("C:\\Program Files\\ZCode\\ZCode.exe"));
        assert!(is_windows_exe("C:\\Program Files\\ZCode\\ZCode.EXE"));
        // Non-executables the registry can serve as DisplayIcon — the exact
        // regression we guard against (Start-Process would open these with
        // the default image viewer instead of launching the app).
        assert!(!is_windows_exe("C:\\Program Files\\ZCode\\zcode.ico"));
        assert!(!is_windows_exe("C:\\Program Files\\ZCode\\resources.dll"));
        // No extension at all.
        assert!(!is_windows_exe("C:\\Program Files\\ZCode\\zcode"));
    }
}
