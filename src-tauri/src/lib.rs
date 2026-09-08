//! EchoBird — Tauri app entry point.
//!
//! All business logic, command implementations, services, models, and
//! utils live in this crate's submodules. [`run`] builds the Tauri
//! context (via `tauri::generate_context!()` against `tauri.conf.json`
//! in this crate's CARGO_MANIFEST_DIR), reads the tray icon, registers
//! the bundled public install-asset table, and starts the Tauri app.

pub mod commands;
pub mod models;
pub mod services;
pub mod utils;

use commands::mod_stub;
use commands::model_commands;
use commands::process_commands;
use commands::settings_commands;
use commands::skill_commands;
use commands::smart_router_commands;
use commands::tool_commands;

use commands::agent_commands;
use commands::ai_career_commands;
use commands::bundled_commands;
use commands::parasite_commands;
use commands::pulse_commands;
use commands::secret_commands;
use commands::ssh_commands;

use std::sync::{Mutex, OnceLock};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

use services::bundled_assets::BundledAssets;

/// Compile-time bundle of every PUBLIC install JSON the smart-install flow
/// reads. Built here (the only place `docs/api/tools/install/` paths resolve
/// against CARGO_MANIFEST_DIR) and registered with `services::bundled_assets`
/// before the Tauri app starts. Internal-only assets (Mother Agent prompt,
/// Quick-Action scripts) are bundled by `services::bundled_assets` itself
/// from `src-tauri/assets/`.
static BUNDLED: BundledAssets = BundledAssets {
    install_index_json: include_str!("../../docs/api/tools/install/index.json"),
    install_refs: &[
        (
            "claudecode",
            include_str!("../../docs/api/tools/install/claudecode.json"),
        ),
        (
            "codex",
            include_str!("../../docs/api/tools/install/codex.json"),
        ),
        (
            "qwencode",
            include_str!("../../docs/api/tools/install/qwencode.json"),
        ),
        (
            "aider",
            include_str!("../../docs/api/tools/install/aider.json"),
        ),
        ("pi", include_str!("../../docs/api/tools/install/pi.json")),
        ("omp", include_str!("../../docs/api/tools/install/omp.json")),
        (
            "hermes",
            include_str!("../../docs/api/tools/install/hermes.json"),
        ),
        (
            "openclaw",
            include_str!("../../docs/api/tools/install/openclaw.json"),
        ),
        (
            "opencode",
            include_str!("../../docs/api/tools/install/opencode.json"),
        ),
        (
            "mimocode",
            include_str!("../../docs/api/tools/install/mimocode.json"),
        ),
        (
            "kilo",
            include_str!("../../docs/api/tools/install/kilo.json"),
        ),
        (
            "kimicode",
            include_str!("../../docs/api/tools/install/kimicode.json"),
        ),
        (
            "claudedesktop",
            include_str!("../../docs/api/tools/install/claudedesktop.json"),
        ),
        (
            "chatgptdesktop",
            include_str!("../../docs/api/tools/install/chatgptdesktop.json"),
        ),
        (
            "geminidesktop",
            include_str!("../../docs/api/tools/install/geminidesktop.json"),
        ),
        (
            "opencodedesktop",
            include_str!("../../docs/api/tools/install/opencodedesktop.json"),
        ),
        (
            "coffeecli",
            include_str!("../../docs/api/tools/install/coffeecli.json"),
        ),
        (
            "claudescience",
            include_str!("../../docs/api/tools/install/claudescience.json"),
        ),
        (
            "openscience",
            include_str!("../../docs/api/tools/install/openscience.json"),
        ),
        (
            "vscode",
            include_str!("../../docs/api/tools/install/vscode.json"),
        ),
        (
            "cursor",
            include_str!("../../docs/api/tools/install/cursor.json"),
        ),
        (
            "clashverge",
            include_str!("../../docs/api/tools/install/clashverge.json"),
        ),
        (
            "trae",
            include_str!("../../docs/api/tools/install/trae.json"),
        ),
        (
            "traecn",
            include_str!("../../docs/api/tools/install/traecn.json"),
        ),
        (
            "grok",
            include_str!("../../docs/api/tools/install/grok.json"),
        ),
        (
            "vibe-trading",
            include_str!("../../docs/api/tools/install/vibe-trading.json"),
        ),
        (
            "workbuddy",
            include_str!("../../docs/api/tools/install/workbuddy.json"),
        ),
        (
            "zcode",
            include_str!("../../docs/api/tools/install/zcode.json"),
        ),
        ("dsh", include_str!("../../docs/api/tools/install/dsh.json")),
    ],
};

/// Managed state for tray locale
pub struct TrayState {
    pub locale: Mutex<String>,
}

/// PNG bytes for the tray icon, set once by [`run`]. We need to access
/// them whenever the tray is rebuilt (e.g. on locale change), but the
/// `include_bytes!` macro that produces them only resolves against the
/// public crate's CARGO_MANIFEST_DIR — so the public shell hands the
/// slice over at startup and we cache it here.
static TRAY_ICON_BYTES: OnceLock<&'static [u8]> = OnceLock::new();

/// Decode the registered tray icon PNG into a Tauri image.
fn load_tray_icon() -> tauri::image::Image<'static> {
    let icon_bytes = TRAY_ICON_BYTES
        .get()
        .copied()
        .expect("tray icon bytes not registered — run() must be called first");
    let img = image::load_from_memory(icon_bytes).expect("Failed to decode tray-icon.png");
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    tauri::image::Image::new_owned(rgba.into_raw(), width, height)
}

/// Get localized tray string
fn tray_t(locale: &str, key: &str) -> String {
    match (locale, key) {
        // English
        ("en", "show") => "Show EchoBird".into(),
        ("en", "quit") => "Quit".into(),
        // Simplified Chinese
        ("zh-Hans", "show") => "显示 EchoBird".into(),
        ("zh-Hans", "quit") => "退出".into(),
        // Fallback to English
        (_, key) => tray_t("en", key),
    }
}

/// Localized labels for the macOS application menu. Only the user-facing custom
/// items need this — predefined items (Quit, Copy, Paste, …) are localized by
/// the OS automatically. Mirrors [`tray_t`]: English + Simplified Chinese, with
/// every other locale falling back to English.
#[allow(dead_code)] // only reachable from install_macos_menu (macOS-only call site)
fn menu_t(locale: &str, key: &str) -> String {
    match (locale, key) {
        ("zh-Hans", "settings") => "设置…".into(),
        ("zh-Hans", "feedback") => "问题反馈".into(),
        ("zh-Hans", "edit") => "编辑".into(),
        ("zh-Hans", "view") => "视图".into(),
        ("zh-Hans", "window") => "窗口".into(),
        ("zh-Hans", "help") => "帮助".into(),
        ("en", "settings") => "Settings…".into(),
        ("en", "feedback") => "Feedback".into(),
        ("en", "edit") => "Edit".into(),
        ("en", "view") => "View".into(),
        ("en", "window") => "Window".into(),
        ("en", "help") => "Help".into(),
        // Fallback to English
        (_, key) => menu_t("en", key),
    }
}

/// Build and install EchoBird's macOS application menu (the screen-top menu bar).
///
/// Tauri auto-creates a default menu on macOS, but it has no Settings entry, so
/// we replace it with an equivalent complete menu that adds **Settings… (⌘,)**
/// and **Feedback**. Every standard shortcut stays bound — ⌘W close, ⌘M
/// minimize, ⌘Q quit, ⌘H hide, ⌘X/⌘C/⌘V/⌘A edit, ⌃⌘F fullscreen — and the
/// window items route through the same `CloseRequested`/lifecycle paths as the
/// in-window controls (so ⌘W honors the close-to-tray setting just like the X
/// button). Called on macOS only: Windows/Linux render menus inside the window,
/// which would clash with the custom frameless title bar.
#[allow(dead_code)] // call site is gated to macOS; kept un-cfg'd so it type-checks everywhere
fn install_macos_menu(app: &tauri::App, locale: &str) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    let settings_item = MenuItemBuilder::with_id("menu-open-settings", menu_t(locale, "settings"))
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let feedback_item =
        MenuItemBuilder::with_id("menu-open-feedback", menu_t(locale, "feedback")).build(app)?;

    // App menu — macOS uses the app name as the title automatically.
    let app_menu = SubmenuBuilder::new(app, "EchoBird")
        .item(&PredefinedMenuItem::about(app, None, None)?)
        .separator()
        .item(&settings_item)
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, menu_t(locale, "edit"))
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, menu_t(locale, "view"))
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    let window_menu = SubmenuBuilder::new(app, menu_t(locale, "window"))
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    let help_menu = SubmenuBuilder::new(app, menu_t(locale, "help"))
        .item(&feedback_item)
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&window_menu)
        .item(&help_menu)
        .build()?;

    app.set_menu(menu)?;
    Ok(())
}

/// Handle clicks on the macOS application-menu items by forwarding them to the
/// frontend, which opens the same Settings dialog / Feedback page as the
/// in-window buttons. Kept as a free function (not an inline closure) so it
/// type-checks on every platform even though it is only registered on macOS.
#[allow(dead_code)] // registered only under #[cfg(target_os = "macos")]
fn handle_app_menu_event(app_handle: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    use tauri::Emitter;
    match event.id().as_ref() {
        "menu-open-settings" => {
            let _ = app_handle.emit("menu-open-settings", ());
        }
        "menu-open-feedback" => {
            let _ = app_handle.emit("menu-open-feedback", ());
        }
        _ => {}
    }
}

/// Rebuild tray menu dynamically (call when locale changes).
pub fn rebuild_tray_menu(app: &tauri::AppHandle) {
    let state = app.state::<TrayState>();
    let locale = state.locale.lock().unwrap().clone();

    // Get tray icon by ID
    let Some(tray) = app.tray_by_id("main-tray") else {
        log::warn!("[Tray] Cannot find tray icon 'main-tray'");
        return;
    };

    // Build menu items
    let app_name = "EchoBird";
    let brand_item = MenuItemBuilder::with_id("brand", app_name)
        .enabled(false)
        .build(app)
        .unwrap();
    let show_item = MenuItemBuilder::with_id("show", tray_t(&locale, "show"))
        .build(app)
        .unwrap();
    let quit_item = MenuItemBuilder::with_id("quit", tray_t(&locale, "quit"))
        .build(app)
        .unwrap();

    // Build menu
    let menu = MenuBuilder::new(app)
        .item(&brand_item)
        .separator()
        .item(&show_item)
        .separator()
        .item(&quit_item)
        .build()
        .unwrap();

    let _ = tray.set_menu(Some(menu));

    // Update icon
    let tray_icon = load_tray_icon();
    let _ = tray.set_icon(Some(tray_icon));

    // Update tooltip
    let _ = tray.set_tooltip(Some(app_name));

    log::info!("[Tray] Menu rebuilt: locale={}", locale);
}

/// Hide the main window to the tray. On macOS this also switches the
/// app to Accessory activation policy so the dock icon disappears
/// (paired with `show_main_window` which restores Regular policy).
pub(crate) fn hide_main_window(app_handle: &tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
}

/// Restore the main window from the tray. On macOS this switches the
/// app back to Regular activation policy first so the dock icon
/// reappears before the window becomes visible.
fn show_main_window(app_handle: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

// ─────────────────────────────────────────────────────────────────────
// Window state persistence (manual, ~/.echobird/window-state.json)
// ─────────────────────────────────────────────────────────────────────
//
// `tauri-plugin-window-state` would do this automatically but it
// intercepts `CloseRequested` events to flush state on exit — that
// tramples our `close_to_tray` logic in the WindowEvent::CloseRequested
// handler below, which is the reason that plugin is permanently
// disabled in `Builder::default()`.
//
// So we save state ourselves: a small JSON file written on every
// `Resized` / `Moved` event the main window emits, read back during
// `setup()` and applied before the window is shown.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WindowStateRecord {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    maximized: bool,
}

// Minimum window dimensions enforced on both restore and capture.
//
// Older versions occasionally persisted tiny sizes (mid-collapse capture,
// close-to-tray race, schema drift) which on the next launch rendered the
// main window as just a thin strip of title bar — a "broken" first
// impression for upgraders. Two layers of defense:
//
//   • apply_window_state clamps up to the minimum, so a poisoned file from
//     an earlier version self-heals on this launch.
//   • capture_window_state refuses to write back anything below the
//     minimum, so this version can never re-poison the file going forward.
//
// The values match the smallest sensible layout for the main view (App
// Manager / Mother Agent panes still readable, sidebar not crushed).
const MIN_WINDOW_WIDTH: u32 = 800;
const MIN_WINDOW_HEIGHT: u32 = 600;

fn window_work_area_overlap(
    state: &WindowStateRecord,
    area: &tauri::PhysicalRect<i32, u32>,
) -> u64 {
    let left = i64::from(state.x).max(i64::from(area.position.x));
    let top = i64::from(state.y).max(i64::from(area.position.y));
    let right = (i64::from(state.x) + i64::from(state.width))
        .min(i64::from(area.position.x) + i64::from(area.size.width));
    let bottom = (i64::from(state.y) + i64::from(state.height))
        .min(i64::from(area.position.y) + i64::from(area.size.height));

    right.saturating_sub(left).max(0) as u64 * bottom.saturating_sub(top).max(0) as u64
}

fn fit_window_state_to_work_area(
    state: &WindowStateRecord,
    area: &tauri::PhysicalRect<i32, u32>,
    preserve_position: bool,
) -> WindowStateRecord {
    let width = state
        .width
        .max(MIN_WINDOW_WIDTH)
        .min(area.size.width.max(1));
    let height = state
        .height
        .max(MIN_WINDOW_HEIGHT)
        .min(area.size.height.max(1));
    let area_x = i64::from(area.position.x);
    let area_y = i64::from(area.position.y);
    let max_x = area_x + i64::from(area.size.width) - i64::from(width);
    let max_y = area_y + i64::from(area.size.height) - i64::from(height);

    let (x, y) = if preserve_position {
        (
            i64::from(state.x).clamp(area_x, max_x),
            i64::from(state.y).clamp(area_y, max_y),
        )
    } else {
        (
            area_x + i64::from(area.size.width.saturating_sub(width)) / 2,
            area_y + i64::from(area.size.height.saturating_sub(height)) / 2,
        )
    };

    WindowStateRecord {
        width,
        height,
        x: x as i32,
        y: y as i32,
        maximized: state.maximized,
    }
}

fn window_state_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".echobird").join("window-state.json"))
}

fn load_window_state() -> Option<WindowStateRecord> {
    let path = window_state_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_window_state(record: &WindowStateRecord) {
    let Some(path) = window_state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(record) {
        let _ = std::fs::write(&path, content);
    }
}

fn apply_window_state(window: &tauri::WebviewWindow, state: &WindowStateRecord) {
    use tauri::{PhysicalPosition, PhysicalSize};

    let monitors = window.available_monitors().unwrap_or_default();
    let saved_monitor = monitors
        .iter()
        .map(|monitor| monitor.work_area())
        .map(|area| (window_work_area_overlap(state, area), area))
        .filter(|(overlap, _)| *overlap > 0)
        .max_by_key(|(overlap, _)| *overlap)
        .map(|(_, area)| area);
    let fallback_monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let restored = saved_monitor
        .map(|area| fit_window_state_to_work_area(state, area, true))
        .or_else(|| {
            fallback_monitor
                .as_ref()
                .map(|monitor| fit_window_state_to_work_area(state, monitor.work_area(), false))
        })
        .unwrap_or_else(|| WindowStateRecord {
            width: state.width.max(MIN_WINDOW_WIDTH),
            height: state.height.max(MIN_WINDOW_HEIGHT),
            ..state.clone()
        });

    // Size first so the final position can be clamped against the actual
    // restored dimensions when the display layout changed between launches.
    let _ = window.set_size(PhysicalSize::new(restored.width, restored.height));
    let _ = window.set_position(PhysicalPosition::new(restored.x, restored.y));
    if restored.maximized {
        let _ = window.maximize();
    }
}

fn capture_window_state(window: &tauri::WebviewWindow) -> Option<WindowStateRecord> {
    let size = window.inner_size().ok()?;
    let pos = window.outer_position().ok()?;
    // Reject obviously-corrupt sizes — minimized / collapsed /
    // pre-destruction captures shouldn't poison the next launch.
    if size.width < MIN_WINDOW_WIDTH || size.height < MIN_WINDOW_HEIGHT {
        return None;
    }
    Some(WindowStateRecord {
        width: size.width,
        height: size.height,
        x: pos.x,
        y: pos.y,
        maximized: window.is_maximized().unwrap_or(false),
    })
}

/// Force-kill a single PID synchronously. Returns true if the kill
/// command exited successfully (process was alive and killed) or false
/// if the process was already gone. Never spawns async — we always wait.
fn force_kill_pid(pid: u32, label: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let status = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let killed = matches!(status, Ok(s) if s.success());
        if killed {
            log::info!("[Cleanup] Killed {} pid={}", label, pid);
        } else {
            log::info!("[Cleanup] {} pid={} already gone", label, pid);
        }
        killed
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let status = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let killed = matches!(status, Ok(s) if s.success());
        if killed {
            log::info!("[Cleanup] Killed {} pid={}", label, pid);
        } else {
            log::info!("[Cleanup] {} pid={} already gone", label, pid);
        }
        killed
    }
}

/// Kill the orphaned llama-server we spawned in a prior session
/// (recorded in ~/.echobird/llama-server.pid by services::local_llm).
/// PID-only — never taskkill /IM llama-server.exe — user-launched
/// instances must survive when EchoBird closes.
fn kill_stale_llama_server() {
    let Some(pid) = services::local_llm::pid_file::read_pid_file() else {
        return;
    };
    log::info!("[Cleanup] Found stale llama-server PID file: pid={}", pid);
    force_kill_pid(pid, "llama-server");
    services::local_llm::pid_file::delete_pid_file();
}

/// Boot the Tauri application.
///
/// Builds the Tauri context via `tauri::generate_context!()` (resolving
/// `tauri.conf.json` against this crate's CARGO_MANIFEST_DIR), reads the
/// tray icon PNG, registers the bundled public install-asset table, then
/// starts the Tauri app. The tray icon bytes are cached in a [`OnceLock`]
/// so [`rebuild_tray_menu`] can repaint the tray after locale changes.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Hydrate the process PATH from the Windows registry (HKLM + HKCU
    // `Path`) before anything resolves executables — the Windows analog of
    // the Unix login-shell PATH sourcing in `utils::platform::shell_command_path`.
    // Append-only + existence-gated; no-op when the inherited PATH is already
    // complete. Must run single-threaded, before the Tauri runtime spawns.
    // See `services/windows_path.rs`.
    #[cfg(windows)]
    crate::services::windows_path::hydrate();

    let context = tauri::generate_context!();
    let tray_icon_bytes: &'static [u8] = include_bytes!("../icons/tray-icon.png");
    services::bundled_assets::register(&BUNDLED);

    if TRAY_ICON_BYTES.set(tray_icon_bytes).is_err() {
        panic!("run() called twice — tray icon bytes already registered");
    }

    tauri::Builder::default()
        // Single-instance guard. Must be the FIRST plugin registered (Tauri
        // requirement). When EchoBird is launched again while already running
        // — e.g. the user double-clicks the desktop shortcut while the window
        // is minimized to tray — the OS would otherwise spawn a second process
        // (Windows has no app-level dedup; the codex proxy's fixed port 53682
        // just logs EADDRINUSE and the duplicate window still opens). Instead,
        // the second launch hands off to this primary instance and we restore
        // and focus the existing window rather than opening a new one.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            log::info!("[SingleInstance] second launch detected — focusing existing window");
            show_main_window(app);
        }))
        // Note: window-state plugin is temporarily disabled because it intercepts
        // CloseRequested events, preventing our "minimize to tray" feature from working.
        // We'll need to manually save/restore window state if needed.
        // .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(TrayState {
            locale: Mutex::new("en".into()),
        })
        .manage(ssh_commands::create_ssh_pool())
        .manage(services::agent_loop::create_session_map())
        .manage(services::parasite::create_parasite_sessions())
        .setup(move |app| {
            // Clean up orphaned llama-server from a previous EchoBird
            // session. The codex launcher doesn't need this — the proxy
            // shares a fixed port (53682) and any stale launcher gets
            // shared by new launchers via the EADDRINUSE branch.
            kill_stale_llama_server();
            log::info!("[Setup] Cleaned up any leftover llama-server processes");

            // Rust codex_proxy. Binds 127.0.0.1:53682 as a background
            // task and serves POST /v1/responses by translating Codex's
            // Responses-API request to upstream Chat Completions, then
            // translating the streaming response back. Replaced the
            // Node-based launcher (tools/codex/lib/*.cjs) that earlier
            // versions shipped — end users no longer need Node installed.
            //
            // If port 53682 is already held by another EchoBird instance
            // the bind fails and we log + continue, so EchoBird's other
            // features still start.
            services::codex_proxy::spawn_proxy_task();

            // Local OpenAI-compatible smart router. It owns a separate fixed
            // loopback port so Codex's Responses bridge can safely use it as
            // an upstream without forwarding back into itself.
            services::smart_router::spawn_proxy_task();

            // Initialize resource_dir for correct tools/ path resolution on all platforms
            // (especially Linux where exe is at /usr/bin but tools are at /usr/lib/com.echobird.ai/)
            if let Ok(res_dir) = app.path().resource_dir() {
                services::tool_manager::init_resource_dir(res_dir);
            } else {
                log::warn!("[Setup] Could not resolve resource_dir");
            }

            // Enable file logging in all builds for diagnostics.
            //
            // Rotation tuned for the Feedback page's "copy last 30 lines"
            // button: the default rotation strategy can flip the file
            // mid-session and orphan the head of the user's window in a
            // sibling file. We bump max_file_size to 4 MB so a normal
            // session stays in one file, and switch to KeepAll so any
            // rotated files stay readable by read_log_tail's
            // multi-file walker.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .max_file_size(4 * 1024 * 1024)
                    .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                    .targets([
                        // Log to file
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: Some("echobird".to_string()),
                        }),
                        // Also log to stdout in dev mode
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    ])
                    .build(),
            )?;

            // Register shell plugin (open external URLs, folders)
            app.handle().plugin(tauri_plugin_shell::init())?;

            // Register clipboard plugin — read-text powers the paste button on
            // the API-key field, reading the OS clipboard from Rust so it
            // bypasses the WebView's clipboard-read permission prompt.
            app.handle()
                .plugin(tauri_plugin_clipboard_manager::init())?;

            // Register dialog plugin (native file pickers, used by the
            // "我的AI项目" Add dialog for icon / launcher / models.json paths)
            app.handle().plugin(tauri_plugin_dialog::init())?;

            // Register autostart plugin (launch at login). Initialized with a
            // `--minimized` arg so a boot autostart launches the app hidden to
            // the tray - app_ready and the 1s fallback show below both check
            // for that arg and skip showing the window when present.
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--minimized"]),
            ))?;

            // ─── System Tray ───
            let tray_icon = load_tray_icon();

            // Load user's locale from settings
            let user_locale = settings_commands::get_settings()
                .locale
                .unwrap_or_else(|| "en".to_string());

            // Build initial tray menu with user's locale
            let brand_item = MenuItemBuilder::with_id("brand", "EchoBird")
                .enabled(false)
                .build(app)?;
            let show_item =
                MenuItemBuilder::with_id("show", tray_t(&user_locale, "show")).build(app)?;
            let quit_item =
                MenuItemBuilder::with_id("quit", tray_t(&user_locale, "quit")).build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&brand_item)
                .separator()
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // Update TrayState with user's locale
            let state = app.state::<TrayState>();
            *state.locale.lock().unwrap() = user_locale;

            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("EchoBird")
                .on_menu_event(move |app_handle, event| match event.id().as_ref() {
                    "show" => {
                        show_main_window(app_handle);
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Left click: toggle window visibility (only on button release)
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app_handle = tray.app_handle();
                        let visible = app_handle
                            .get_webview_window("main")
                            .and_then(|w| w.is_visible().ok())
                            .unwrap_or(false);
                        if visible {
                            hide_main_window(app_handle);
                        } else {
                            show_main_window(app_handle);
                        }
                    }
                    // Right click: show menu (handled automatically by Tauri)
                })
                .build(app)?;

            // ─── macOS application menu ───
            // Replace Tauri's auto-generated default menu with our own complete
            // menu so ⌘, opens Settings (the default has no Settings item) while
            // every other standard shortcut stays bound. macOS only — on
            // Windows/Linux a menu renders inside the window and would clash with
            // the custom frameless title bar, so they are left untouched.
            #[cfg(target_os = "macos")]
            {
                let menu_locale = settings_commands::get_settings()
                    .locale
                    .unwrap_or_else(|| "en".to_string());
                if let Err(e) = install_macos_menu(app, &menu_locale) {
                    log::warn!("[macOS] Failed to install application menu: {e}");
                }

                // Route app-menu clicks to the frontend, which opens the same
                // Settings dialog / Feedback page as the in-window buttons. Tray
                // menu ids ("show"/"quit") are handled by the tray's own handler
                // and fall through here.
                app.on_menu_event(handle_app_menu_event);
            }

            // Windows 11: disable shadow and force square corners on borderless window.
            // Without this, DWM adds a drop-shadow and rounds corners by default,
            // creating visible gaps between the system border and the app content.
            #[cfg(target_os = "windows")]
            {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_shadow(false);

                    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::Graphics::Dwm::{
                        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
                    };

                    if let Ok(handle) = win.window_handle() {
                        if let RawWindowHandle::Win32(win32_handle) = handle.as_ref() {
                            let hwnd = HWND(win32_handle.hwnd.get() as _);
                            let pref: i32 = DWMWCP_DONOTROUND.0;
                            unsafe {
                                let _ = DwmSetWindowAttribute(
                                    hwnd,
                                    DWMWA_WINDOW_CORNER_PREFERENCE,
                                    &pref as *const _ as *const _,
                                    std::mem::size_of_val(&pref) as u32,
                                );
                            }
                        }
                    }
                }
            }

            // macOS: ensure cursor events are enabled to prevent hit-test failures.
            // Some users reported buttons becoming unresponsive (only scrolling worked)
            // when decorations=false + transparent=true + window-state restoration
            // caused the window to lose proper event routing.
            #[cfg(target_os = "macos")]
            {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_ignore_cursor_events(false);
                    log::info!("[macOS] Explicitly enabled cursor events for hit-test");
                }
            }

            // Note: Window close interception is now handled in the frontend (App.tsx)
            // using getCurrentWindow().onCloseRequested() API, which is the recommended
            // approach in Tauri 2.0 for cross-platform compatibility.

            // Restore previous window size + position (saved at ~/.echobird/window-state.json).
            // Manual because the tauri-plugin-window-state plugin intercepts
            // CloseRequested events, fighting our close-to-tray flow.
            if let Some(state) = load_window_state() {
                if let Some(win) = app.get_webview_window("main") {
                    apply_window_state(&win, &state);
                    log::info!(
                        "[WindowState] Restored {}x{} at ({},{}) maximized={}",
                        state.width,
                        state.height,
                        state.x,
                        state.y,
                        state.maximized
                    );
                }
            }

            // Save window state on every resize/move. Skips writes when the
            // window is hidden (close-to-tray) — those events fire spurious
            // dimensions that would overwrite the real state with garbage.
            if let Some(win) = app.get_webview_window("main") {
                let win_handle = win.clone();
                win.on_window_event(move |event| {
                    if matches!(
                        event,
                        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)
                    ) {
                        if !win_handle.is_visible().unwrap_or(true) {
                            return;
                        }
                        if let Some(record) = capture_window_state(&win_handle) {
                            save_window_state(&record);
                        }
                    }
                });
            }

            // Safety fallback: show main window after 1s even if appReady() never fires.
            // Uses std::thread to avoid tokio runtime dependency in sync setup().
            #[cfg(not(target_os = "android"))]
            {
                // Boot-autostart launches pass `--minimized` and must stay
                // hidden in the tray - skip the safety fallback so it never
                // pops the window open 1s after a boot launch.
                let start_minimized = std::env::args().any(|a| a == "--minimized");
                if !start_minimized {
                    let fallback_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1000));
                        if let Some(win) = fallback_handle.get_webview_window("main") {
                            if !win.is_visible().unwrap_or(true) {
                                log::warn!(
                                    "[Safety] appReady() not called after 1s — showing main window"
                                );
                                let _ = win.center();
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    });
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mod_stub::app_ready,
            mod_stub::read_log_tail,
            tool_commands::scan_tools,
            tool_commands::apply_model_to_tool,
            tool_commands::restore_tool_to_official,
            tool_commands::launch_game,
            tool_commands::apply_user_project_model,
            tool_commands::launch_user_project,
            tool_commands::seed_builtin_to_user_dir,
            tool_commands::open_folder,
            tool_commands::open_tool_paths_config,
            model_commands::get_models,
            model_commands::add_model,
            model_commands::delete_model,
            model_commands::update_model,
            model_commands::reorder_models,
            model_commands::test_model,
            model_commands::ping_model,
            model_commands::is_key_destroyed,
            skill_commands::get_skills,
            skill_commands::add_skill,
            skill_commands::delete_skill,
            skill_commands::update_skill,
            model_commands::query_model_usage,
            model_commands::save_volc_aksk,
            model_commands::has_volc_aksk,
            model_commands::clear_volc_aksk,
            model_commands::get_volc_aksk,
            smart_router_commands::get_smart_router_config,
            smart_router_commands::get_smart_router_activity,
            smart_router_commands::get_smart_router_candidates,
            smart_router_commands::remove_smart_router_candidate,
            smart_router_commands::set_smart_router_candidates,
            process_commands::start_tool,
            process_commands::start_llm_server,
            process_commands::stop_llm_server,
            process_commands::get_llm_server_info,
            process_commands::get_llm_server_logs,
            process_commands::get_llm_default_command,
            process_commands::get_llm_custom_command,
            process_commands::set_llm_custom_command,
            process_commands::clear_llm_custom_command,
            process_commands::download_and_install_update,
            process_commands::get_models_dirs,
            process_commands::get_download_dir,
            process_commands::scan_gguf_files,
            process_commands::scan_hf_models,
            process_commands::add_models_dir,
            process_commands::remove_models_dir,
            process_commands::detect_gpu,
            process_commands::get_gpu_info,
            process_commands::set_download_dir,
            process_commands::get_store_models,
            process_commands::get_model_directory,
            process_commands::get_free_model_directory,
            process_commands::download_model,
            process_commands::pause_download,
            process_commands::cancel_download,
            process_commands::get_system_info,
            process_commands::get_local_engine_status,
            process_commands::install_local_engine,
            process_commands::list_engine_release_options,
            settings_commands::get_settings,
            settings_commands::save_settings,
            settings_commands::get_my_projects,
            settings_commands::save_my_projects,
            ssh_commands::ssh_test_connection,
            ssh_commands::load_ssh_servers,
            ssh_commands::save_ssh_server,
            ssh_commands::remove_ssh_server,
            secret_commands::decrypt_secret,
            secret_commands::encrypt_secret,
            agent_commands::agent_send_message,
            agent_commands::agent_abort,
            agent_commands::agent_reset,
            parasite_commands::parasite_list_installed,
            parasite_commands::parasite_send_message,
            parasite_commands::parasite_abort,
            parasite_commands::parasite_reset,
            bundled_commands::get_mother_hints,
            bundled_commands::get_install_index,
            pulse_commands::pulse_save,
            pulse_commands::pulse_load_all,
            pulse_commands::pulse_list_dates,
            ai_career_commands::ai_career_family_history,
            ai_career_commands::ai_career_heatmap,
            ai_career_commands::ai_career_token_bytes,
            settings_commands::set_avatar,
            settings_commands::get_avatar,
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::WindowEvent {
                    label,
                    event: tauri::WindowEvent::CloseRequested { api, .. },
                    ..
                } if label == "main" => {
                    // Check user settings for close behavior
                    let settings = settings_commands::get_settings();
                    let close_to_tray = settings.close_to_tray.unwrap_or(false);

                    if close_to_tray {
                        // Prevent the window from closing and hide it instead.
                        // On macOS this also drops the dock icon (Accessory
                        // policy) so the app lives only in the menu bar until
                        // the user re-opens it from the tray.
                        api.prevent_close();
                        hide_main_window(app_handle);
                    }
                    // Otherwise, let it close normally
                }
                tauri::RunEvent::Exit => {
                    // Clean up llama-server (our spawned local LLM). The
                    // codex launcher we don't touch — if it's still running
                    // when EchoBird exits, that's fine; the user keeps
                    // working in Codex, and any new launcher we spawn next
                    // session will share the same FIXED proxy port.
                    kill_stale_llama_server();
                    log::info!("[App] Exit: cleaned up llama-server");
                }
                _ => {}
            }
        });
}

#[cfg(test)]
mod window_state_tests {
    use super::*;

    fn state(width: u32, height: u32, x: i32, y: i32) -> WindowStateRecord {
        WindowStateRecord {
            width,
            height,
            x,
            y,
            maximized: false,
        }
    }

    fn area(width: u32, height: u32) -> tauri::PhysicalRect<i32, u32> {
        tauri::PhysicalRect {
            position: (0, 0).into(),
            size: (width, height).into(),
        }
    }

    #[test]
    fn keeps_a_visible_saved_window_unchanged() {
        let saved = state(1400, 900, 200, 100);
        let area = area(1920, 1040);

        let restored = fit_window_state_to_work_area(&saved, &area, true);

        assert_eq!(restored.width, 1400);
        assert_eq!(restored.height, 900);
        assert_eq!((restored.x, restored.y), (200, 100));
    }

    #[test]
    fn shrinks_a_saved_window_to_a_smaller_display_work_area() {
        let saved = state(2400, 1400, 0, 0);
        let area = area(1366, 728);

        let restored = fit_window_state_to_work_area(&saved, &area, true);

        assert_eq!(restored.width, 1366);
        assert_eq!(restored.height, 728);
        assert_eq!((restored.x, restored.y), (0, 0));
    }

    #[test]
    fn clamps_a_partly_offscreen_window_back_into_view() {
        let saved = state(1200, 800, 1000, 500);
        let area = area(1920, 1040);

        let restored = fit_window_state_to_work_area(&saved, &area, true);

        assert_eq!((restored.x, restored.y), (720, 240));
    }

    #[test]
    fn centers_a_window_when_its_saved_display_is_gone() {
        let saved = state(1200, 800, 2560, 200);
        let area = area(1920, 1040);

        assert_eq!(window_work_area_overlap(&saved, &area), 0);
        let restored = fit_window_state_to_work_area(&saved, &area, false);

        assert_eq!((restored.x, restored.y), (360, 120));
    }
}
