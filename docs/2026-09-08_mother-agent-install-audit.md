# Mother Agent 安装方式核对（2026-09-08）

核对了现有 28 个软件的官网、官方仓库或发布元数据，以及 Git、CUDA 两个快捷安装脚本。更新了 20 份软件安装参考、CUDA 脚本、相关提示词及 3 份检测配置；其余现有安装方式保留。

这是安装说明与下载入口的核验，不是 30 个软件在所有操作系统上的实际安装验收。本次没有在工作机执行这些第三方安装器。

## 优先修复的问题

- OpenCode：Windows `install.ps1` 返回 HTTP 404；另一处流程还把 Bash 脚本传给 PowerShell `iex`。两处均改为官方支持的 npm / Scoop / Chocolatey / 发布二进制路径。
- OpenClaw：旧 Node 22.14 门槛已不适用；补充新版 npm 生命周期脚本选项，并通过 `--no-onboard` / `-NoOnboard` 避免安装过程卡在交互式初始化。
- Qwen、Pi、Kimi：分别核对 npm 运行时要求，避免统一套用旧 Node 版本。Qwen 的独立安装和 npm 回退分开处理，Windows 新安装目录纳入检测。
- Hermes Desktop：普通脚本只保证 CLI；改为桌面安装器或带桌面参数的脚本，并分别验证 CLI 和 GUI。
- CUDA：解析官网最新稳定版链接、排除 Developer Preview、保留旧 GPU 可选版本；兼容不含驱动版本的新 Windows 安装包文件名。

## 全量核对表

“保留”表示本次找到的官方来源仍支持项目现有主要安装方式，不表示验证了每个历史镜像、账号登录或模型配置字段。

| ID / 软件 | 结论与处理 | 官方证据 |
| --- | --- | --- |
| `aider` | 更新：优先官方隔离安装器 / uv；区分 bootstrap Python 与直接 pip/pipx 的版本范围。 | [安装文档](https://aider.chat/docs/install.html) |
| `chatgptdesktop` | 更新：Linux preview 的发行版、架构、deb/rpm 安装与启动说明；保留 Windows Store ID。添加 Linux 桌面入口检测。 | [Linux](https://learn.chatgpt.com/docs/linux/linux-app)、[Windows](https://learn.chatgpt.com/docs/windows/windows-app)、[桌面应用](https://learn.chatgpt.com/docs/app) |
| `clashverge` | 更新：WinGet / Homebrew 仍从 GitHub 下载，不能承诺绕过网络限制。当前 latest.json 与 v2.5.2 资源命名仍有效。 | [安装文档](https://www.clashverge.dev/install.html)、[WinGet manifest](https://github.com/microsoft/winget-pkgs/blob/master/manifests/c/ClashVergeRev/ClashVergeRev/2.5.2/ClashVergeRev.ClashVergeRev.installer.yaml)、[Homebrew cask](https://formulae.brew.sh/api/cask/clash-verge-rev.json) |
| `claudecode` | 更新：npm 仍受支持，安装时要求 Node 22+；Homebrew cask 仅 macOS；区分原生自动更新与包管理器更新。删除安装时强写旧 `allowedTools` / onboarding 状态的步骤，改为版本与 doctor 检查。 | [安装与更新](https://code.claude.com/docs/en/setup)、[settings schema](https://code.claude.com/docs/en/settings) |
| `claudedesktop` | 更新：官方 Linux beta 支持 Ubuntu 22.04+ / Debian 12+、x86_64/arm64；使用签名 apt 仓库。添加 Linux 桌面入口检测。 | [官方下载](https://claude.com/download)、[Linux 安装文档](https://code.claude.com/docs/en/desktop-linux) |
| `claudescience` | 下载方式保留；修正 Linux 命令，先创建 `~/.local/bin` 再下载。 | [官方下载入口](https://claude.com/download)、[产品页](https://claude.com/product/claude-science) |
| `codex` | 更新：优先不依赖 Node 的独立安装器；npm 按发布包 engines 核对，Homebrew cask 仅 macOS。区分 Windows 原生与 WSL 安装，删除“CLI 不能安装到远程服务器”的错误描述。 | [Codex CLI](https://learn.chatgpt.com/docs/codex/cli)、[官方包元数据](https://registry.npmjs.org/@openai/codex/latest) |
| `coffeecli` | 保留：官方 README 仍列仓库 `install/install.ps1` 与 `install/install.sh`。 | [官方仓库](https://github.com/edison7009/Coffee-CLI#install) |
| `cursor` | 更新：Linux 官方同时提供 deb、rpm、AppImage，并需选择 x64/ARM64。 | [官方下载](https://cursor.com/download) |
| `dsh` | 更新：npx 示例补 `web`；本地启动会自动开浏览器，SSH 启动只打印地址；去除过时的固定预览版本描述，明确 Node 22.19+ 的 22.x 或 24+，排除 Node 23。 | [官方仓库](https://github.com/deepseek-ai/deepseek-harness)、[运行时范围](https://github.com/deepseek-ai/deepseek-harness/blob/master/package.json) |
| `geminidesktop` | 更新：补充 Apple Silicon、macOS 15.0+ 前置条件；Windows/Linux 原生应用仍不支持。 | [官方 Mac 页面 FAQ](https://gemini.google/mac/) |
| `grok` | 保留：官方 shell / PowerShell 安装器可用。 | [Grok Build 安装](https://docs.x.ai/build/overview)、[PowerShell 安装器](https://x.ai/cli/install.ps1) |
| `hermes` | 更新：桌面安装器优先；Windows scriptblock 传 `-IncludeDesktop`，Unix 传 `--include-desktop`；已有 CLI 使用 `hermes desktop`。修正依赖版本、managed uv 和安装后验证说明。 | [安装文档](https://hermes-agent.nousresearch.com/docs/getting-started/installation/)、[Desktop](https://hermes-agent.nousresearch.com/docs/user-guide/desktop)、[PowerShell](https://hermes-agent.nousresearch.com/install.ps1)、[shell](https://hermes-agent.nousresearch.com/install.sh) |
| `kilo` | 保留：`@kilocode/cli` 与官方 shell 安装器仍有效。 | [CLI 文档](https://kilo.ai/docs/code-with-ai/platforms/cli)、[安装器](https://kilo.ai/cli/install) |
| `kimicode` | 更新：原生安装器仍有效；补充 npm 需要 Node 22.19.0+。保留 Windows Git Bash 前置条件。 | [Getting started](https://moonshotai.github.io/kimi-code/en/guides/getting-started)、[仓库](https://github.com/MoonshotAI/kimi-code) |
| `mimocode` | 更新：新增官方 Windows PowerShell 安装器，npm 继续可用。 | [仓库 Quick Start](https://github.com/XiaomiMiMo/MiMo-Code#quick-start)、[PowerShell 安装器](https://mimo.xiaomi.com/install.ps1) |
| `openclaw` | 更新：官方无 onboarding 安装参数、新 Node 要求、npm 11.16+/12 的 `--allow-scripts=openclaw`；首次 onboarding 交给用户终端，升级保留已有配置。 | [安装文档](https://docs.openclaw.ai/install)、[发布包 engines](https://registry.npmjs.org/openclaw/latest) |
| `opencode` | 更新：去除失效 PowerShell 路径及 Bash → `iex` 错误；Windows 使用 npm / Scoop / Chocolatey / Releases，Bun 路径限制到 macOS/Linux。 | [官方安装文档](https://opencode.ai/docs/)、[官方仓库](https://github.com/anomalyco/opencode) |
| `opencodedesktop` | 更新：下载页仍有 macOS、Windows、Linux deb/rpm；移除下载页未列出的 AppImage 承诺。 | [官方下载](https://opencode.ai/download) |
| `openscience` | 保留：`@synsci/openscience`、`npx synsci`、shell 安装器及现有二进制资源仍有效。上游另有 GUI，本次继续使用已有 CLI/浏览器工作台集成。 | [官方仓库](https://github.com/synthetic-sciences/OpenScience)、[发布资源](https://github.com/synthetic-sciences/OpenScience/releases/latest)、[安装器](https://openscience.sh/install) |
| `pi` | 更新：Node 门槛改为 22.19.0+；shell 安装器需要 Node/npm；镜像命令同步保留官方 `--ignore-scripts`。 | [CLI README](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/README.md)、[安装器](https://pi.dev/install.sh)、[包 engines](https://registry.npmjs.org/@earendil-works/pi-coding-agent/latest) |
| `qwencode` | 更新：独立安装器、新 Node 22+ npm 要求、Windows `%LOCALAPPDATA%/qwen-code/bin/qwen.cmd` 检测。Unix 脚本仅 npm 分支检查 Node，独立安装不应被统一 Node 检查阻断。 | [仓库](https://github.com/QwenLM/qwen-code)、[Unix 安装器](https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.sh)、[Windows 安装器](https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.ps1) |
| `trae` | 更新：国际版下载中心已有 IDE Linux deb/rpm，移除拒绝 Linux 的步骤。已有 Linux desktop hints 可继续用于检测。 | [官方下载中心](https://www.trae.ai/download) |
| `traecn` | 更新：国内版下载中心已有 IDE Linux deb/rpm，需与同页 TraeWork 等产品区分。已有 Linux desktop hints 可继续用于检测。 | [官方下载中心](https://www.trae.cn/download) |
| `vibe-trading` | 保留：PyPI `vibe-trading-ai`、uv tool 和源码安装仍有效。 | [官方仓库安装说明](https://github.com/HKUDS/Vibe-Trading#-quick-start) |
| `vscode` | 保留：官方下载与发行版包安装方式仍有效。 | [官方下载](https://code.visualstudio.com/Download)、[安装文档](https://code.visualstudio.com/docs/setup/setup-overview) |
| `workbuddy` | 保留：官网 Windows/macOS 安装与 `Tencent.WorkBuddy` WinGet 包仍存在。 | [官方介绍](https://www.codebuddy.cn/docs/workbuddy/Overview)、[WinGet 包目录](https://github.com/microsoft/winget-pkgs/tree/master/manifests/t/Tencent/WorkBuddy) |
| `zcode` | 保留：官网继续提供 Windows/macOS/Linux 安装包，并列出不同架构。 | [官方下载](https://zcode.z.ai/cn#all-downloads) |
| Git 快捷安装 | 保留：`Git.Git`、macOS Homebrew / CLT、Linux 发行版包仍是官方安装途径。本次未验证第三方二进制镜像的全部版本。 | [Windows](https://git-scm.com/install/windows)、[Git 安装](https://git-scm.com/book/en/v2/Getting-Started-Installing-Git) |
| CUDA 快捷安装 | 更新：最新稳定版实际链接、预览版过滤、新旧文件名、驱动独立安装、Blackwell 架构区分、旧 GPU 可选版本及使用安装目录验证。 | [版本索引](https://developer.nvidia.com/cuda-toolkit-archive)、[Windows 安装](https://docs.nvidia.com/cuda/cuda-installation-guide-microsoft-windows/index.html)、[13.1 起取消驱动捆绑](https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html)、[GPU 表](https://developer.nvidia.com/cuda/gpus)、[驱动兼容性](https://docs.nvidia.com/deploy/cuda-compatibility/minor-version-compatibility.html) |

## 来源差异与验证边界

OpenClaw 在线文档和 npm `latest` 的门槛存在差异：本次文档要求 Node 24.16+ 或 26.1+，推荐 26；读取到的 `openclaw@2026.9.2` engines 为 `>=22.22.3 <23 || >=24.15.0 <25 || >=25.9.0`。安装参考优先官方 bootstrapper，手动 npm 安装则检查实际目标包 engines，不把这次读到的发布版本永久固定下来。

部分下载页面由 JavaScript 渲染。TRAE 国内版在网页提取工具中报错，但直接读取官方 HTML 成功，能看到 IDE 的 `.deb / .rpm` 与 x64 下载项；国际版 HTML 也有对应项。WorkBuddy 使用官方文档与 Microsoft 的 WinGet 包目录交叉核对。没有把网页提取失败当作软件停用，也没有执行下载按钮背后的安装包。

没有全面审计软件模型配置格式、登录协议、卸载步骤、全部镜像、Linux GUI 首次启动或不同 CPU 的二进制运行兼容性。表中“新增 Linux 支持”按厂商 preview/beta 范围记录，仍需要对应系统上的实际安装验收。

## 项目中生效的位置

- [安装参考目录](api/tools/install/)：本次改动的 20 份 JSON。
- [编译入口](../src-tauri/src/lib.rs)：通过 `include_str!` 直接嵌入同一目录的 JSON。因此此处没有第二份独立的 bundled JSON 要手工同步；构建新版应用后才会携带新参考。
- [Mother Agent 提示词](../src-tauri/assets/mother/system_prompt.md)、[agent loop 提示词](../src-tauri/src/services/agent_loop.rs)、[嵌入参考提示](../src-tauri/src/services/bundled_assets.rs)：移除旧 Node 固定下载，保持离线参考优先；参考明确要求获取当前下载入口或仓库配置时，允许先读取官方来源；安装失败明确暴露参考过时时，也允许核对后重试。
- [Qwen 检测](../tools/qwencode/paths.json)、[Claude Desktop 检测](../tools/claudedesktop/paths.json)、[ChatGPT 检测](../tools/chatgptdesktop/paths.json)：补充新路径或 Linux 桌面项名称。
- [CUDA 安装脚本](../src-tauri/assets/quick-actions/install-cuda.md)：修正版本和安装包解析，以及安装后验证。

## 本地验证

提交前审查补充修正了三处问题：通用联网规则与 Linux 官网仓库配置步骤的冲突、CUDA 缺少 nvcc 时的明确失败检查，以及 DSH Node 范围原有描述对 Node 23 的歧义。

- 前端：typecheck、format:check、lint、Vitest 全部通过；23 项测试通过。
- Rust：fmt --check、clippy（all-targets / all-features / -D warnings）、cargo test 全部通过；502 项通过，1 项已有忽略。
- 安装数据：28 个 ID 与 JSON 文件、编译嵌入清单、运行时清单一致；JSON 可解析。
- CUDA：在 PowerShell 5.1 中运行从 Markdown 提取的解析代码，以本次官方 HTML 为输入。验证最新稳定版 13.3.1 指向 `/cuda-downloads`、排除 13.4.0 Developer Preview、保留 12.6.3 等旧版本、菜单前 8 项，以及新旧安装包文件名和不同版本误匹配。
- CUDA 失败场景：用本地临时目录模拟 cudart 存在、nvcc 缺失且上一条原生命令退出码为 0 的情况；复现旧代码继续走到验证末尾，确认补充检查会明确报错。10 个 PowerShell 代码块均通过 5.1 语法解析。
- `git diff --check` 通过。

后续复查应重新读取表中的官方来源、npm engines 和发布资源，而不是仅比较软件版本号。只有实际执行安装器并验证目标平台的程序启动，才能升级为该平台的安装验收记录。
