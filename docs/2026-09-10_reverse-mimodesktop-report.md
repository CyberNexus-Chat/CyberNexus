# Xiaomi MiMo Desktop 测试版接入记录

核对版本：`26.909.91205`；日期：2026-09-10。

Xiaomi MiMo Desktop 使用 MiMo Code 引擎和服务商配置，但将桌面默认模型单独保存在 Electron 偏好文件中。EchoBird 新增独立的 `mimodesktop` 桌面条目、官方图标和安装参考，并写入桌面默认选择。此实现面向新任务；“应用并启动”复用项目已有的桌面重启流程，“仅修改”后需手动完全退出并重新打开。已有任务可能保留自己的模型。

## 范围

用户授权接入其本机安装的 Xiaomi MiMo Desktop。只读核对安装包 JavaScript、配置结构和图标；联网读取官方文档及检查下载链接；实现和测试 EchoBird。未修改已安装应用、真实配置或密钥，未请求中转模型，也未覆盖安装现有程序。

本地分析记录：[scope.md](../.agent/mimodesktop-case/scope.md)。该目录是本机临时证据，不随仓库发布；下方保留了版本、文件来源和核对方法。

## 下载与安装

来源：[官网](https://mimo.xiaomimimo.com/desktop/invite/)、[官方更新说明](https://mimo.mi.com/docs/zh-CN/updates/feature/desktop)。以下 CDN 链接由用户提供，并通过 HTTP HEAD 检查。

| 平台 | 官方文件 | 本次检查 |
| --- | --- | --- |
| Windows x64 | [EXE](https://mimocode-cdn.xiaomimimo.com/mimocode/mimodesktop/XiaomiMiMo-latest-x64-setup.exe) | HTTP 200；251232872 字节 |
| macOS ARM64 | [DMG](https://mimocode-cdn.xiaomimimo.com/mimocode/mimodesktop/XiaomiMiMo-latest-arm64.dmg) | HTTP 200；363843592 字节 |
| Linux x64 | [AppImage](https://mimocode-cdn.xiaomimimo.com/mimocode/mimodesktop/XiaomiMiMo-latest-x64.AppImage) | HTTP 404；安装时须重新检查，失效时报告不可用 |

Windows 安装检测匹配注册表 DisplayName 前缀 `Xiaomi MiMo`，兼容名称后的版本号，并从 DisplayIcon / 卸载器同目录定位程序。因此支持自定义盘符，无需写死本机路径。安装参考由远端 `docs/api/tools/install/` 提供，同时通过 `include_str!` 编译进安装助手，离线索引同步增加 `mimodesktop`。

Mac 下载仅适用于 ARM64。安装参考提供平台安装步骤，但本次未执行安装包；Mac/Linux 实机安装与启动尚未验证。测试资格及登录仍由官方管理。

## 配置机制与实现

| 用途 | 位置 / 字段 |
| --- | --- |
| 服务商定义 | `~/.config/mimocode/mimocode.jsonc`，兼容已有 `mimocode.json` / `config.json` |
| Windows 桌面偏好 | `%APPDATA%/Xiaomi MiMo/preferences.json` |
| macOS 桌面偏好 | `~/Library/Application Support/Xiaomi MiMo/preferences.json` |
| Linux 桌面偏好 | `$XDG_CONFIG_HOME/Xiaomi MiMo/preferences.json`，未设置时为 `~/.config/Xiaomi MiMo/preferences.json` |
| EchoBird 服务商 | `provider["echobird-desktop"]`，OpenAI-compatible 协议 |
| 桌面当前默认模型 | 偏好文件的 `model = "echobird-desktop/<model-id>"` |

服务商目录遵循安装包中 `Tr()` 的规则：`MIMOCODE_HOME/config` 优先，其次 `XDG_CONFIG_HOME/mimocode`，然后 `~/.config/mimocode`。已有配置文件优先级为 JSONC、JSON、config.json。

应用模型时保留其他服务商和桌面偏好，不更改共享配置的顶层 `model` / `small_model`，避免覆盖 MiMo Code CLI 的默认选择。读取当前模型时使用桌面偏好中的服务商与模型 ID，并按 `config.json → mimocode.json → mimocode.jsonc` 合并服务商字段，兼容模型定义与接口覆盖分散在多个文件的情况。恢复操作仅移除 `echobird-desktop`，且只清除属于它的桌面选择。

解析接受注释和尾逗号，拒绝缺逗号、非法对象等损坏配置。两份文件均验证成功后才开始写入。沿用项目 JSON 写入方式，写回时会规范化格式并移除注释。写入失败会返回错误；配置写入与偏好写入不是跨文件事务。

## 证据与结论

安装包：`E:/Xiaomi MiMo/resources/app.asar`，SHA-256：
`5a4a179371b91ae271adf8bb078967336e4248192f19c98b436f0286a4f4a702`。

| Evidence | 来源与复核方式 | 观察 |
| --- | --- | --- |
| E-001 | ASAR `package.json` / `out/main/launch.mjs`；读取 JSON 和入口 import | 产品名 Xiaomi MiMo，版本 26.909.91205；Electron 入口加载 `out/main/index.mjs` |
| E-002 | `out/main/index.mjs` 中 `Tr`、`FR`、`en`、`XZ`；搜索函数名及 `mimo:getPrefs` | 服务商配置使用 MiMo Code 路径；Electron Store 名称为 preferences；偏好接口返回 `model` |
| E-003 | `out/renderer/assets/index-DKekZm7Y.js` 中 `mme`、`cme`、`modelByConvo` | 启动偏好加载器将 `prefs.model` 赋给模型状态；任务另有模型状态 |
| E-004 | ASAR `assets/icon-win.png`；与新增图标做 SHA-256 对比 | `public/icons/tools/mimodesktop.png` 为原始官方图标的逐字节副本，未重绘 |
| E-005 | Windows 注册表扫描测试；`cargo test real_registry_finds_mimodesktop -- --ignored --nocapture` | 用新增条目中的安装提示定位本机程序 |
| E-006 | `curl.exe -I <上述官方下载地址>` | Windows / Mac 为 200，Linux 为 404；哈希不适用（只检查响应头） |

图标 SHA-256：`93f6d314f5bea8ebfb915bb860a9f6b2007da96bc3d08a4acf187d1427d262bd`。图标及 Xiaomi MiMo 名称归原权利人所有，仅用于识别所集成的应用。

F-001（`n/a_re`，validated，高置信度，E-002/E-003）：桌面选模不能只写 CLI 默认字段；应写 preferences.json 的 model。位置为 `Tr/FR/XZ/mme`。

F-002（`n/a_re`，validated，高置信度，E-001/E-004/E-005）：这是独立桌面产品，可用产品名注册表提示检测，图标可使用安装包原始资源。

P-001（`path_type=callflow`）：EchoBird 应用模型 → 写入共享配置的独立服务商 → 写入桌面 preferences.model → 完全退出并重开桌面 → `mimo:getPrefs` → `mme` 恢复默认选择 → 新任务使用选择的模型。已有任务与实时引擎刷新不在本次自动切换范围内。

`tool_manager::is_managed_desktop_tool` 依据 Desktop 分类和模型配置能力识别本条目；`process_manager::start_tool` 会调用现有终止并重启流程。Windows 进程名由条目路径推导为 `Xiaomi MiMo.exe`。该重启会中断运行中的任务，界面提示“应用并启动”将重启程序；本次核对未对用户正在运行的实例执行重启。

## 验证与复核

Rust 测试覆盖连续切换、CLI 与其他服务商保留、桌面偏好保留、手动添加的服务商读取、含斜杠的模型 ID、JSONC 优先级、注释及尾逗号、损坏配置不覆盖、恢复时仅清理自有字段。

前端 typecheck、format:check、lint 通过，27 项测试通过。Rust fmt、全目标全特性 clippy 通过，523 项测试通过、2 项依赖本机安装的测试默认忽略；此前单独运行 Xiaomi MiMo 注册表检测测试通过，返回 `E:/Xiaomi MiMo/Xiaomi MiMo.exe`。安装索引、编译内置引用和 JSON 格式一致性检查通过。

提交前复核修复了两处边界问题：新增 Rust 回归测试先复现多文件配置读取遗漏接口覆盖，再验证合并后的接口、密钥和模型名称；本地浏览器使用真实 `AppManagerMain` 和模拟安装状态，复现某分类最后一个应用安装后列表空白，修复后确认自动选中“全部”并显示其余未安装应用。浏览器测试未调用真实安装或模型接口。

本地还用 Node VM 执行了安装包原始 `mme/cme` 偏好加载函数，注入模拟偏好和状态，验证两个自定义模型 ID 与清空选择，共 3 项通过。该检查验证加载函数，并非完整桌面会话或中转请求测试。原始资源提取和加载器检查脚本位于 `.agent/mimodesktop-case/`，不会发布安装包源码。

仓库复核命令：

```powershell
npm run typecheck
npm run format:check
npm run lint
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --jobs 1 --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --jobs 1
cargo test --manifest-path src-tauri/Cargo.toml --locked --jobs 1 real_registry_finds_mimodesktop -- --ignored --nocapture
```

时间线：2026-09-10 先核对本机配置与官方发布资料；随后读取安装包中的路径、偏好加载及图标；再实现独立桌面接入、运行隔离配置测试和仓库检查。后续版本应优先复核偏好字段、配置目录、引擎刷新机制和 Linux 下载可用性。
