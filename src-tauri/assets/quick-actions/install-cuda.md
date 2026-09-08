# CUDA Module Installation — Agent Instructions

You are installing the **CUDA modules** (= CUDA Toolkit / `cudart64_*.dll`
user-mode libraries) on the user's Windows machine. Execute autonomously,
but ask ONCE before triggering the multi-GB Toolkit download. Fail loud
on stripped / modified Windows where install genuinely cannot proceed —
do NOT silently retry or fudge state.

**Scope**: this script ONLY installs CUDA modules + their prerequisites
(VC++ Redist, Windows Installer service). Anything about the engine —
EchoBird's CUDA-build of the inference engine, the SETUP ENGINE button
— is **out of scope**. Do NOT mention it. The user handles the engine
themselves through EchoBird's normal one-click UI; that flow is
independent of this script.

**No agent-side admin gate.** The CUDA Toolkit `.exe` manifest is marked
`requireAdministrator`, so Windows itself raises a UAC prompt when the
installer is launched. The user clicks Yes → installer runs elevated.
Do NOT check admin status in our shell up-front, and do NOT instruct
the user to relaunch EchoBird as administrator — that's a 3-step
workaround for a 1-click UAC prompt. Just launch the installer; if the
user declines UAC, the exit code surfaces the failure normally.

## Terminology

- **NVIDIA driver** — kernel-mode driver from NVIDIA. Provides
  `nvidia-smi`. Not installed by this script (kernel installs are
  risky — directed to NVIDIA's official page in Step 1).
- **CUDA modules** — the user-mode CUDA Toolkit. Authoritative
  presence check: `cudart64_*.dll` reachable. **This is what this
  script installs.**

All commands are PowerShell (Windows PowerShell 5.1, NOT pwsh 7).

Reply in the user's UI language. The trigger phrase tells you which one:
- "Help me install CUDA modules" → English
- "帮我安装CUDA模块" → Simplified Chinese
- "幫我安裝 CUDA 模組" → Traditional Chinese
- "CUDA モジュールのインストールを支援" → Japanese

## Step 0: Mandatory Pre-flight

**Re-run the full detect-cuda.md probe first.** Do NOT skip — users
often click "Install" without ever clicking "Detect", so the install
path must include its own diagnosis.

Branch on the verdict:

- **No NVIDIA GPU** → tell user CUDA does not apply and stop. Do NOT
  proceed.
- **Modified Windows detected** (tiny11/Atlas/ReviOS/AME/Ghost/stripped)
  → see Step 3: refuse and stop.
- **Driver missing** → handle in Step 1 first, then stop and ask the
  user to come back after driver install + reboot.
- **Driver OK, modules missing** → main path (Steps 2-6).
- **Modules already installed** → tell the user nothing needs
  installing; CUDA modules are already in place. Stop.

## Step 1: NVIDIA Driver (if detect-cuda flagged missing)

If `nvidia-smi` failed in Step 0:

- Direct the user to `https://www.nvidia.com/Download/index.aspx`,
  tell them to pick their GPU, download the latest **Game Ready** or
  **Studio Driver**, install, reboot, and re-run "Detect CUDA module
  status".
- Stop here. Do NOT attempt driver auto-install — kernel driver
  failures can brick display output.

## Step 2: User picks the Toolkit version

**Do NOT pick the version for the user.** Fetch NVIDIA's archive and
present the top 8 stable versions; the user chooses. Check the selected
Toolkit's driver and GPU requirements before downloading it.

### 2a. Fetch the top 8 versions from NVIDIA archive

```powershell
$archive = (Invoke-WebRequest 'https://developer.nvidia.com/cuda-toolkit-archive' -UseBasicParsing).Content
$releasePages = @{}
$rx = [regex]::Matches($archive, '<a\s+href="(?<path>/cuda-(?:downloads|\d+-\d+-\d+-download-archive))"[^>]*>\s*CUDA Toolkit\s+(?<version>\d+\.\d+\.\d+)(?<suffix>[^<]*)</a>')
foreach ($m in $rx) {
    if ($m.Groups['suffix'].Value -match '(?i)preview|beta|\brc\b') { continue }
    $v = $m.Groups['version'].Value
    $releasePages[$v] = 'https://developer.nvidia.com' + $m.Groups['path'].Value
}
$allVersions = @($releasePages.Keys | Sort-Object { [version]$_ } -Descending)
$versions = @($allVersions | Select-Object -First 8)
# Keep the full stable list for finding an older GPU-compatible recommendation.
# The latest release links to /cuda-downloads, not a versioned archive URL.
```

If the fetch fails or returns no stable versions: tell the user
to fix VPN/proxy and try again, or visit
`https://developer.nvidia.com/cuda-toolkit-archive` manually. Stop.

### 2b. Read driver ceiling (informational)

```powershell
$ceiling = $null
try {
    $smi = & nvidia-smi 2>$null
    $m = [regex]::Match(($smi -join "`n"), 'CUDA Version:\s*(\d+\.\d+)')
    if ($m.Success) { $ceiling = $m.Groups[1].Value }
} catch { }
# $ceiling is the driver's advertised CUDA API version, not a hard
# Toolkit-installation limit. Minor-version compatibility may apply.
```

### 2c. Detect GPU + recommend a CUDA version (★)

This is the **primary recommendation** the user sees. The agent reads
the GPU model + driver ceiling, classifies the architecture, and
marks the optimal CUDA version with ★. Without this, a 5090 user
gets stuck on CUDA 12.6 if they don't know to pick higher; without
this, a GTX 1060 user might pick CUDA 13.x and watch it fail.

```powershell
$gpus = Get-CimInstance Win32_VideoController | Where-Object { $_.Name -like '*NVIDIA*' }
$gpuName = ($gpus | Select-Object -First 1).Name
# $gpuName is e.g. "NVIDIA GeForce RTX 5090" or "NVIDIA GeForce GTX 1060 6GB"
```

Classify architecture from `$gpuName` and apply the recommendation rule:

| Marketing name pattern | Architecture | Compute cap | ★ Recommended CUDA |
|---|---|---|---|
| `GTX 9..` / `GTX TITAN X` | Maxwell | sm_5x | latest **12.x** (13.x dropped Maxwell) |
| `GTX 10..` / `TITAN Xp` | Pascal | sm_6x | latest **12.x** in driver ceiling |
| `RTX 20..` / `TITAN RTX` / `GTX 16..` | Turing | sm_75 | latest **12.x** in driver ceiling |
| `RTX 30..` / `A100` | Ampere | sm_8x | latest **12.x** in driver ceiling |
| `RTX 40..` / `L40` / `H100` | Ada / Hopper | sm_8.9/9.0 | latest **12.x** in driver ceiling |
| `RTX 50..` | Blackwell | sm_120 | compatible **12.8+** or **13.x**, after checking the Toolkit release notes |
| `B200` / `GB200` | Blackwell | sm_100 | compatible **12.8+** or **13.x**, after checking the Toolkit release notes |

Recommendation logic:
1. If GPU is Blackwell **and** driver ceiling ≥ 13.0 → ★ = the highest
   stable 13.x in `$allVersions` within the driver's advertised version.
2. Otherwise → ★ = the highest version in `$allVersions` whose
   major.minor ≤ `$ceiling` AND whose major ≤ 12 (cap at 12.x for
   pre-Blackwell to avoid forcing driver upgrade for marginal gain).
   Blackwell additionally requires a Toolkit supporting the specific GPU;
   do not recommend a version below 12.8 based only on the driver number.
3. Confirm the GPU in NVIDIA's compute-capability table and the selected
   release's supported targets. If no candidate is confirmed, show no ★
   and explain which compatibility requirement still needs checking.
4. If `$recommended` is outside the top 8, append it to `$versions` so
   users with older GPUs can actually choose the recommended release.

This is a conservative recommendation, not proof that higher minor
versions cannot run. Check NVIDIA's driver compatibility guide for the
actual minimum driver and PTX limitations. CUDA 13.1+ Windows Toolkits
do not bundle a display driver; upgrading the Toolkit does not upgrade it.

Store the recommended version string in `$recommended` (e.g. `"12.6.3"`).

### 2d. Present the menu and wait for user input

Render this exact shape (translate surrounding prose to the user's UI
language; keep version numbers and letter keys verbatim). Annotate
each version with markers:

- `★ 适合你的显卡` (or English equivalent) if version == `$recommended`
- `⚠ 高于驱动报告的 CUDA <ceiling>，需核对兼容性` if version's major.minor > `$ceiling`

**No engine-aware markers.** The user installs CUDA modules here;
matching the engine afterwards is a separate concern (the engine
picker on the 'Local LLM' page handles that — but do NOT reference
it in your reply).

```
请选择要安装的 CUDA Toolkit 版本(NVIDIA 官方最新 8 个):

  [A] 13.2.1   ⚠ 需核对驱动兼容性
  [B] 13.2.0   ⚠ 需核对驱动兼容性
  [C] 13.1.2   ⚠ 需核对驱动兼容性
  [D] 13.1.1   ⚠ 需核对驱动兼容性
  [E] 13.0.3   ⚠ 需核对驱动兼容性
  [F] 13.0.2   ⚠ 需核对驱动兼容性
  [G] 12.9.2   ⚠ 需核对驱动兼容性
  [H] 12.9.1   ⚠ 需核对驱动兼容性
  [I] 12.6.3   ★ 适合你的 GTX 1060(Pascal,驱动 560.94)

  [N] 我自己去 NVIDIA 网站下,不用你装

提示:
- ★ 是基于你的显卡 + 驱动给的最优推荐
- ⚠ 是高于驱动报告版本的 Toolkit，需要核对 NVIDIA 的驱动兼容表。
  Toolkit 安装成功不代表运行兼容；较新版本也不会自动升级驱动。

输入字母:
```

(The menu may have 9 or fewer entries; pick letters A-I as needed.)

Wait for explicit response (single letter, case-insensitive). Validate
strictly — anything outside the rendered range or N → re-prompt once,
then abort.

If the user takes more than ~10 seconds to respond (or asks "which
should I pick?"), nudge them toward ★. Do NOT bring up engines.

### 2e. Honour the user's pick

- **N** → print
  `https://developer.nvidia.com/cuda-toolkit-archive` and stop. Don't
  install anything.
- **A-I (or whatever letters were rendered)** → set
  `$cudaVersion` to the picked entry's version. Continue.

### 2f. Single confirmation gate

> "About to install CUDA Toolkit <picked version> (~3 GB download,
> 5-10 minutes total). Windows will pop a UAC prompt during install —
> click Yes when it appears. Do not close the window during install.
> Continue? (Y/N)"

Wait for explicit yes / confirm / continue / y. Anything else = abort.

## Step 3: Modified Windows — Refuse, Don't Try

If detect-cuda flagged any of: `tiny11`, `Atlas`, `ReviOS`, `AME`,
`Ghost`, `精简`, `优化`, `纯净`, `Lite`, OR if these probes are broken
on a Caption that already looks suspicious:

- Windows Installer service: `Disabled`
- Trusted Installer service: `Disabled`
- Multiple core MSVC runtime DLLs missing from System32
- `ucrtbase.dll` missing

→ **Refuse to install.** Tell the user (translate to their UI language):

> "Your Windows is a stripped / modified build (<exact name detected>,
> e.g. tiny11). Core components — Windows Installer service, Visual
> C++ runtime, system DLLs — have been removed or disabled.
>
> CUDA Toolkit installs on this kind of Windows fail roughly 80% of
> the time, often leaving the system in a half-installed state worse
> than where you started. This is not something EchoBird can repair.
>
> Two paths forward:
>  1. Reinstall an official Windows 11 image (from Microsoft's site,
>     not another modified ISO).
>  2. Use a different, standard Windows machine.
>
> Once you're on a standard Windows, click this button again and
> I'll install CUDA for you immediately."

Then stop. Do NOT try to fix the stripped Windows.

## Step 4: Standard Install Path

Un-modified Windows, driver OK, user confirmed in Step 2.

### 4a. Repair Visual C++ Redistributable (defensive)

CUDA Toolkit needs MSVC runtime. If detect-cuda found ANY of
`vcruntime140.dll`, `msvcp140.dll`, `vcruntime140_1.dll` missing, OR
the registry key `HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64`
absent:

```powershell
# winget is preinstalled on Windows 11 22H2+. For 10 / older 11:
# fall back to direct download.
winget install --id Microsoft.VCRedist.2015+.x64 --silent --accept-package-agreements --accept-source-agreements
```

If winget is missing:
- Download `https://aka.ms/vs/17/release/vc_redist.x64.exe` to `$env:TEMP`
- Run with `/install /quiet /norestart`
- The redist installer is also `requireAdministrator` — Windows will
  UAC-prompt the user.
- Non-zero exit code = log, surface to user, stop.

### 4b. Ensure Windows Installer service can run (best-effort)

```powershell
$svc = Get-Service msiserver
try {
    if ($svc.StartType -eq 'Disabled') {
        Set-Service msiserver -StartupType Manual -ErrorAction Stop
        Start-Service msiserver -ErrorAction Stop
    } elseif ($svc.Status -ne 'Running') {
        Start-Service msiserver -ErrorAction Stop
    }
} catch {
    # We aren't elevated yet, or msiserver is locked. Log and continue —
    # the CUDA installer's own UAC-elevated process can usually start
    # msiserver itself. If the install later fails on the msiserver
    # symptom, Step 4e will surface it.
    Write-Host "Note: could not modify msiserver from current session ($_). Continuing — installer may handle it after UAC."
}
```

### 4c. Resolve the canonical .exe URL for the user's pick (do NOT guess)

`$cudaVersion` is the version the user picked in Step 2. Older filenames
include a driver version (`cuda_12.6.3_561.17_windows.exe`); CUDA 13.1+
does not bundle the Windows driver and uses filenames such as
`cuda_13.3.1_windows.exe`. Resolve the URL from the selected official page.

```powershell
# Use the actual link captured in Step 2, including /cuda-downloads for latest.
$downloadPage = $releasePages[$cudaVersion]

# Fetch the page. Apply target_os/target_arch/target_version query if
# the page needs them to render the Windows section:
$pageHtml = (Invoke-WebRequest "$downloadPage`?target_os=Windows&target_arch=x86_64&target_version=11&target_type=exe_local" -UseBasicParsing).Content
$pageHtml = [System.Net.WebUtility]::HtmlDecode($pageHtml)

# Match this exact Toolkit version, with an optional bundled-driver suffix.
$escapedVersion = [regex]::Escape($cudaVersion)
$canonicalUrl = ([regex]::Match($pageHtml,
    "https://developer\.download\.nvidia\.com/compute/cuda/$escapedVersion/local_installers/cuda_$escapedVersion(?:_[\d\.]+)?_windows\.exe")).Value
```

**Do NOT** probe multiple `local_installers/` paths by guessing —
that wastes turns hitting 404s (the agent has done exactly this in
past runs). One regex match on the archive page, done.

If the page is unreachable or the regex doesn't match:
- Tell the user `developer.nvidia.com` is unreachable from this
  network, or the page format changed. Ask them to fix VPN/proxy,
  or download CUDA <picked version> manually from
  the selected `$downloadPage`
  and run the installer themselves. Stop.

### 4d. Mirror speed test + download

Take the canonical URL from 4c and extract its **path tail**:
`<X.Y.Z>/local_installers/<official-installer-filename>`.
Append this tail to each mirror prefix in the list below, then
race a 2 MB Range download against each to find the fastest mirror
for THIS user's network. Whoever wins gets the full download.

```powershell
$canonicalUrl = '<URL resolved in 4c>'

# Extract the path tail by stripping the official prefix.
$officialPrefix = 'https://developer.download.nvidia.com/compute/cuda/'
$pathTail = $canonicalUrl -replace [regex]::Escape($officialPrefix), ''

# Mirror prefixes to race. The agent does NOT need to know which are
# "Chinese" vs "global" — the speed test sorts it out automatically.
# A 404 / timeout on any mirror just drops it from the race.
$mirrors = @(
    'https://mirrors.tuna.tsinghua.edu.cn/nvidia-cuda/',
    'https://mirrors.ustc.edu.cn/cuda-toolkit/',
    'https://mirrors.bfsu.edu.cn/nvidia-cuda/',
    'https://mirrors.aliyun.com/nvidia-cuda/',
    'https://mirrors.cloud.tencent.com/nvidia-cuda/',
    'https://mirrors.sjtug.sjtu.edu.cn/nvidia-cuda/',
    'https://mirror.nju.edu.cn/nvidia-cuda/',
    'https://developer.download.nvidia.com/compute/cuda/'
)

# Race: 2 MB Range GET, 8s timeout per mirror. Skip on any failure.
$probeFile = Join-Path $env:TEMP 'cuda_probe.bin'
$results = @()
foreach ($prefix in $mirrors) {
    $candidate = $prefix + $pathTail
    try {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        Invoke-WebRequest -Uri $candidate `
                          -Headers @{ 'Range' = 'bytes=0-2097151' } `
                          -OutFile $probeFile `
                          -TimeoutSec 8 -UseBasicParsing -ErrorAction Stop | Out-Null
        $sw.Stop()
        $mbps = [math]::Round(2.0 / $sw.Elapsed.TotalSeconds, 2)
        $results += [pscustomobject]@{ Prefix = $prefix; Mbps = $mbps; Url = $candidate }
    } catch {
        # 404 / timeout / TLS error → mirror dropped from race
    } finally {
        Remove-Item $probeFile -Force -ErrorAction SilentlyContinue
    }
}

$winner = $results | Sort-Object Mbps -Descending | Select-Object -First 1
if (-not $winner) {
    # Every mirror including official failed — surface to user, stop.
    return
}

# Surface the race result so the user knows what's happening.
# Format: "Fastest mirror: <host> at ~X.X MB/s — starting download"

$dest = Join-Path $env:TEMP 'cuda_installer.exe'
Invoke-WebRequest -Uri $winner.Url -OutFile $dest -UseBasicParsing
```

Show progress periodically — 3 GB, 5-30 minutes depending on the
mirror. Report size + elapsed time every ~25%.

If ALL mirrors fail (winner is null): tell the user their network
can't reach NVIDIA's archive or any known mirror. Suggest manual
download from `https://developer.nvidia.com/cuda-toolkit-archive`
or a configured VPN, then stop.

### 4e. Install (silent, runtime + nvcc only)

```powershell
# -s = silent. Listing specific components keeps the install slim:
# we don't need samples, Nsight, Visual Studio integration.
# Windows raises the UAC prompt automatically because the installer's
# manifest is requireAdministrator.
# Component names embed the user-picked major.minor from Step 2.
$mm = ($cudaVersion -split '\.')[0..1] -join '.'
& "$env:TEMP\cuda_installer.exe" -s "nvcc_$mm" "cudart_$mm"
```

Wait for exit. Then:

```powershell
Get-ChildItem $env:TEMP\CUDA_Setup*.log | Sort-Object LastWriteTime -Descending | Select-Object -First 1 | Get-Content -Tail 80
```

If non-zero exit, surface the last 80 log lines + a brief interpretation.
Common failure modes:
- "User declined elevation" → user clicked No on UAC. Tell them to retry
  and accept the prompt.
- "Installer service could not be accessed" → msiserver issue (Step 4b
  more aggressively: reboot needed).
- "Out of disk space" → user needs ~5 GB free on C:.
- "Visual Studio Integration" failures are OK to ignore — we only need
  cudart + nvcc.

## Step 5: Verify Modules Are Reachable

```powershell
# Resolve InstallDir from the registry for the version the user picked.
$mm = ($cudaVersion -split '\.')[0..1] -join '.'
$cudaPath = (Get-ItemProperty "HKLM:\SOFTWARE\NVIDIA Corporation\GPU Computing Toolkit\CUDA\v$mm" -ErrorAction SilentlyContinue).InstallDir
if (-not $cudaPath) { throw "CUDA v$mm installation directory was not registered" }
$cudaPath = [Environment]::ExpandEnvironmentVariables($cudaPath)

# Verify the selected Toolkit, not a DLL left behind by another version.
if (-not (Test-Path "$cudaPath\bin\cudart64_*.dll")) { throw "CUDA runtime DLL was not installed in $cudaPath" }

# Child shells inherit this process's old PATH. Use the installed binary.
if (-not (Test-Path "$cudaPath\bin\nvcc.exe" -PathType Leaf)) { throw "CUDA compiler was not installed in $cudaPath" }
& "$cudaPath\bin\nvcc.exe" --version
if ($LASTEXITCODE -ne 0) { throw "Installed nvcc verification failed" }
```

If all checks pass → success. If `cudart64_*.dll` is still nowhere to
be found → install failed silently; re-run detect-cuda and report
what's missing.

## Step 6: Wrap-up

Tell the user (translate to their UI language):

> "✓ CUDA modules installed."

**Stop after this single line.** Do NOT add any of the following — every
one of them has been flagged by users as out-of-place noise:

- "You may need to restart EchoBird for the env vars to take effect"
  → EchoBird is a UI, not a CUDA consumer. If the engine subprocess
  inherits stale env, that's EchoBird's internal concern, not the
  user's.
- "Reopen your terminal / PowerShell window" → users have no terminal
  in their mental model; this script ran inside the agent's shell,
  invisible to them.
- "If you see CUDA errors later, try restarting" → preemptive
  troubleshooting belongs in support docs, not in a success message.
- "Click SETUP ENGINE next" / any reference to the engine flow → out
  of scope (see Hard Rules).

One line. Stop. The user closes the chat, clicks whatever they want
in the main UI.

## Hard Rules

- **User picks the version, agent only recommends with ★** — Step 2
  lists the NVIDIA archive's top 8 stable versions. The agent classifies the
  user's GPU + driver and marks the GPU-optimal version with ★
  using Step 2c, appending an older compatible recommendation when needed.
  **But the user picks** — never auto-pick, never skip the menu, never
  short-circuit on the assumption that ★ is what they want. If their
  pick fails (driver too old, etc.), fail loud and explain what went
  wrong.
- **One confirmation gate** before downloading (Step 2f). After
  that, no more "Are you sure?" prompts — just install and report.
  Windows handles UAC on its own.
- **Fail loud**: on any non-zero exit code, surface the actual error
  + log tail. Never retry silently. Never claim success when Step 5's
  verification fails.
- **No agent-side admin gate** — the installer's manifest triggers
  UAC. Don't precheck `IsInRole('Administrator')`; don't tell the
  user to relaunch EchoBird as admin.
- **One-line wrap-up, no extras** — the success message is exactly
  `✓ CUDA modules installed.` (translated). No "restart EchoBird",
  no "reopen terminal", no "if you see errors later". The user's
  mental model has neither restarts nor terminals for this product;
  hedging suggestions undermine the success.
- **Never conflate the NVIDIA driver with CUDA modules.** Separate
  pieces.
- **Stay in scope**: CUDA modules + their install prerequisites. Do
  NOT mention EchoBird's engine, the SETUP ENGINE button, what to do
  after install, or how GPU acceleration works in EchoBird. That is
  out of scope.
- **Reply in the user's UI language** — detect from the trigger
  phrase (帮我 → Chinese, 幫我 → Traditional, 支援 → Japanese,
  Help me → English).
