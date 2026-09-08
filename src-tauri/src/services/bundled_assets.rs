// Bundled install/script assets — embedded at compile time via include_str!.
//
// Two source trees feed into this module:
//
//   * Public install JSONs live in the public repository's
//     `docs/api/tools/install/`, which is also served at
//     echobird.ai/api/tools/install/.... The public crate's thin shell
//     constructs a [`BundledAssets`] with those `include_str!`-baked
//     strings and calls [`register`] before [`crate::run`], so this
//     crate sees them as a single `&'static` reference and never
//     touches the filesystem at runtime.
//
//   * The Mother Agent prompt/hints and the Quick-Action task scripts
//     (network-info, security-audit) are internal app behavior, NOT a
//     public API. They live INSIDE this crate under `assets/` and are
//     pulled in directly via `include_str!` below.

use std::sync::OnceLock;

/// Compile-time bundle of every PUBLIC text asset the agent / smart-install
/// flow reads. Constructed by the public crate (the only place where
/// `docs/api/...` paths resolve) and handed to [`register`] before
/// [`crate::run`] starts the Tauri app.
///
/// Internal-only assets (Mother Agent prompt / hints, Quick-Action task
/// scripts) are NOT part of this struct — they live inside this crate
/// and are exposed via dedicated `pub const`s + the existing accessor
/// functions below.
pub struct BundledAssets {
    pub install_index_json: &'static str,
    /// Tool-id → install reference JSON. Lookup is linear; the list is
    /// short (~21 entries) so no map needed.
    pub install_refs: &'static [(&'static str, &'static str)],
}

/// Mother Agent system prompt. Lives in this private crate so the
/// content is not visible in the public repository or on echobird.ai.
const MOTHER_SYSTEM_PROMPT: &str = include_str!("../../assets/mother/system_prompt.md");

/// Mother Agent welcome-screen hints. Same privacy rationale as
/// [`MOTHER_SYSTEM_PROMPT`].
const MOTHER_HINTS_JSON: &str = include_str!("../../assets/mother/hints.json");

/// Quick-Action: "Show Internal/Public IP" task script. Drives the
/// agent through network info gathering + tunnel setup. Internal-only
/// — read in isolation by a user, the auto-tunnel-without-asking
/// wording could be misread as a backdoor, so it does not live on the
/// public site.
const QUICK_ACTION_NETWORK_INFO: &str = include_str!("../../assets/quick-actions/network-info.md");

/// Quick-Action: "Detect Suspicious Activity" task script. Drives the
/// agent through a server-side security audit (failed-login analysis,
/// crypto-miner process scan, etc.). Same privacy rationale as
/// [`QUICK_ACTION_NETWORK_INFO`].
const QUICK_ACTION_SECURITY_AUDIT: &str =
    include_str!("../../assets/quick-actions/security-audit.md");

/// Quick-Action: "Detect CUDA Module Status" task script. Probes the
/// user's Windows for NVIDIA driver, CUDA runtime/Toolkit, and the
/// stripped-edition red flags (msiserver disabled, MSVC runtime
/// missing, tiny11/Atlas/ReviOS markers) that explain most CUDA pain
/// reports. Read-only — never modifies state. Internal-only by the
/// same rationale as [`QUICK_ACTION_NETWORK_INFO`].
const QUICK_ACTION_DETECT_CUDA: &str = include_str!("../../assets/quick-actions/detect-cuda.md");

/// Quick-Action: "Install CUDA Modules" task script. Pre-flights via
/// the detect script, refuses to install on modified Windows, handles
/// admin / VC++ Redist / msiserver prerequisites, downloads the
/// Toolkit installer and runs it silent. Internal-only by the same
/// rationale as [`QUICK_ACTION_NETWORK_INFO`].
const QUICK_ACTION_INSTALL_CUDA: &str = include_str!("../../assets/quick-actions/install-cuda.md");

/// Quick-Action: "Help me install Git" task script. winget first on
/// Windows with the npmmirror git-for-windows binary mirror as the
/// mainland-network fallback; brew / xcode-select on macOS; distro
/// package managers on Linux. Git is a prerequisite the website
/// promises we can install.
/// Internal-only by the same rationale as [`QUICK_ACTION_NETWORK_INFO`].
const QUICK_ACTION_INSTALL_GIT: &str = include_str!("../../assets/quick-actions/install-git.md");

static ASSETS: OnceLock<&'static BundledAssets> = OnceLock::new();

/// Register the bundled assets. Must be called exactly once, before
/// [`crate::run`].
pub fn register(assets: &'static BundledAssets) {
    if ASSETS.set(assets).is_err() {
        panic!("bundled_assets::register called twice");
    }
}

fn assets() -> &'static BundledAssets {
    ASSETS
        .get()
        .copied()
        .expect("bundled_assets::register must be called before any accessor")
}

pub fn mother_system_prompt() -> &'static str {
    MOTHER_SYSTEM_PROMPT
}

pub fn mother_hints_json() -> &'static str {
    MOTHER_HINTS_JSON
}

pub fn install_index_json() -> &'static str {
    assets().install_index_json
}

pub fn get_install_ref(tool_id: &str) -> Option<&'static str> {
    assets()
        .install_refs
        .iter()
        .find(|(id, _)| *id == tool_id)
        .map(|(_, json)| *json)
}

pub fn get_tool_script(name: &str) -> Option<&'static str> {
    match name {
        "network-info" => Some(QUICK_ACTION_NETWORK_INFO),
        "security-audit" => Some(QUICK_ACTION_SECURITY_AUDIT),
        "detect-cuda" => Some(QUICK_ACTION_DETECT_CUDA),
        "install-cuda" => Some(QUICK_ACTION_INSTALL_CUDA),
        "install-git" => Some(QUICK_ACTION_INSTALL_GIT),
        _ => None,
    }
}

/// IDs of every tool with a bundled install reference. Mirrors the keys of
/// `get_install_ref` so the system prompt and AppManager stay in sync.
pub const INSTALLABLE_TOOL_IDS: &[&str] = &[
    "claudecode",
    "codex",
    "qwencode",
    "aider",
    "pi",
    "omp",
    "hermes",
    "openclaw",
    "opencode",
    "mimocode",
    "kilo",
    "kimicode",
    "claudedesktop",
    "chatgptdesktop",
    "geminidesktop",
    "opencodedesktop",
    "coffeecli",
    "claudescience",
    "openscience",
    "vscode",
    "cursor",
    "clashverge",
    "trae",
    "traecn",
    "grok",
    "vibe-trading",
    "workbuddy",
    "zcode",
    "dsh",
];

/// Build the full embedded-references block to append to the system prompt.
/// The agent reads from this instead of `web_fetch`-ing echobird.ai.
pub fn build_embedded_refs_section() -> String {
    let mut out = String::with_capacity(48 * 1024);
    out.push_str("\n\n---\n\n## OFFLINE-FIRST: Embedded Install References\n\n");
    out.push_str(
        "The references below are bundled with the EchoBird app. **PREFER \
         them over `web_fetch`** — many users choose smart-install precisely \
         because their network is unreliable. Use `web_fetch` for tools not \
         in this list, when a reference explicitly requires current download \
         links or repository setup instructions, or when an install failure indicates \
         an outdated endpoint, package, prerequisite, or installer option. \
         Verify replacements against the tool's official docs/repository.\n\n",
    );

    out.push_str("### Tool Install JSONs\n\n");
    for tool_id in INSTALLABLE_TOOL_IDS {
        if let Some(json) = get_install_ref(tool_id) {
            out.push_str(&format!(
                "#### `{}` install reference\n```json\n{}\n```\n\n",
                tool_id,
                json.trim()
            ));
        }
    }

    out.push_str("### Quick-Action Task Scripts\n\n");
    for name in &[
        "network-info",
        "security-audit",
        "detect-cuda",
        "install-cuda",
        "install-git",
    ] {
        if let Some(md) = get_tool_script(name) {
            out.push_str(&format!(
                "#### `{}.md` (use this when the matching Quick Action runs)\n{}\n\n",
                name,
                md.trim()
            ));
        }
    }

    out
}
