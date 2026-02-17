import { app, BrowserWindow, ipcMain, shell, Menu, Tray, nativeImage, session } from 'electron';
import path from 'path';
import fs from 'fs';
import { toolManager } from './toolManager.js';
import * as skillManager from './skillManager.js';
import * as processManager from './processManager.js';
import { registerLocalModelHandlers, isLocalServerRunning, setServerStatusChangeCallback } from './ipc/localModelHandlers.js';
import { addAppLog, getAppLogs, clearAppLogs } from './appLogger.js';

// esbuild injects __dirname in CJS mode

let mainWindow: BrowserWindow | null = null;
let tray: Tray | null = null;
let isQuitting = false;

// 主进程托盘菜单翻译
let trayLocale = 'en';
const trayStrings: Record<string, Record<string, string>> = {
    en: { show: 'Show WhichClaw', server: 'LOCAL SERVER', on: 'ON', off: 'OFF', quit: 'Quit', tooltip: 'Local Server' },
    'zh-Hans': { show: '显示 WhichClaw', server: '本地服务器', on: '开启', off: '关闭', quit: '退出', tooltip: '本地服务器' },
    'zh-Hant': { show: '顯示 WhichClaw', server: '本機伺服器', on: '開啟', off: '關閉', quit: '結束', tooltip: '本機伺服器' },
    ja: { show: 'WhichClaw を表示', server: 'ローカルサーバー', on: 'オン', off: 'オフ', quit: '終了', tooltip: 'ローカルサーバー' },
    ko: { show: 'WhichClaw 표시', server: '로컬 서버', on: '켜기', off: '끄기', quit: '종료', tooltip: '로컬 서버' },
};
function trayT(key: string): string {
    return trayStrings[trayLocale]?.[key] || trayStrings.en[key] || key;
}
let splashWindow: BrowserWindow | null = null;

function createSplashWindow() {
    splashWindow = new BrowserWindow({
        width: 300,
        height: 300,
        frame: false,
        transparent: true,
        resizable: false,
        alwaysOnTop: true,
        skipTaskbar: true,
        backgroundColor: '#00000000',
        webPreferences: {
            contextIsolation: true,
            nodeIntegration: false,
        },
    });

    if (!app.isPackaged) {
        // 开发模式：从 public 目录加载
        splashWindow.loadFile(path.join(__dirname, '../public/splash.html'));
    } else {
        // 生产模式：从 dist 目录加载（vite 会复制 public 里的文件到 dist）
        splashWindow.loadFile(path.join(__dirname, '../dist/splash.html'));
    }

    splashWindow.center();
}

function createWindow() {
    const preloadPath = path.join(__dirname, 'preload.cjs');
    console.log('[Main] __dirname:', __dirname);
    console.log('[Main] Preload path:', preloadPath);
    console.log('[Main] Preload file exists:', fs.existsSync(preloadPath));

    mainWindow = new BrowserWindow({
        width: 1400,
        height: 900,
        minWidth: 1200,
        minHeight: 700,
        frame: false,        // 无边框窗口 - 无系统标题栏
        show: false,         // 先隐藏，等前端渲染完成后再显示
        backgroundColor: '#0F1117',  // 应用主色，避免加载闪白
        // Windows requires .ico icon, dev mode uses default
        // Prod build automatically uses build/icon.ico
        webPreferences: {
            preload: preloadPath,
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: false,  // Disable sandbox to ensure preload works
        },
    });

    // 移除默认菜单栏
    Menu.setApplicationMenu(null);

    // Pass window instance to processManager for IPC
    processManager.setMainWindow(mainWindow);

    // Listen for webContents errors
    mainWindow.webContents.on('did-fail-load', (_event, errorCode, errorDescription) => {
        console.error('[Main] Page load failed:', errorCode, errorDescription);
    });

    mainWindow.webContents.on('preload-error', (_event, preloadPath, error) => {
        console.error('[Main] Preload error:', preloadPath, error);
    });

    mainWindow.webContents.on('console-message', (_event, _level, message) => {
        if (message.includes('[Preload]')) {
            console.log('[Main->Console]', message);
        }
    });

    // Dev: load Vite server, Prod: load build files
    if (!app.isPackaged) {
        // 开发模式：连接 Vite 开发服务器
        mainWindow.loadURL('http://localhost:5173').catch(() => {
            console.log('Dev server not running, loading build files...');
            mainWindow?.loadFile(path.join(__dirname, '../dist/index.html'));
        });
    } else {
        // 生产模式：直接加载打包后的 HTML
        mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
    }

    // Fix WebSocket origin for production builds
    // Dev mode uses http://localhost:5173 (accepted by tools like OpenClaw)
    // Prod mode uses file:// (rejected by many WebSocket servers)
    mainWindow.webContents.session.webRequest.onBeforeSendHeaders(
        { urls: ['ws://*/*', 'wss://*/*'] },
        (details, callback) => {
            if (details.requestHeaders['Origin'] === 'file://') {
                details.requestHeaders['Origin'] = `http://${new URL(details.url).hostname}`;
            }
            callback({ requestHeaders: details.requestHeaders });
        }
    );

    // 关闭窗口时隐藏到托盘而非退出
    mainWindow.on('close', (e) => {
        if (!isQuitting) {
            e.preventDefault();
            mainWindow?.hide();
        }
    });

    mainWindow.on('closed', () => {
        mainWindow = null;
    });
}

// 从像素数据生成托盘图标（彻底避免 SVG 渲染缝隙）
function createTrayIcon(color: 'green' | 'yellow' = 'green'): Electron.NativeImage {
    // 7×7 pixel pattern (WhichClaw logo), 1=filled, 0=transparent
    const pixels = [
        [0, 1, 1, 1, 1, 1, 0],
        [1, 0, 1, 1, 0, 1, 1],
        [1, 1, 1, 1, 1, 1, 1],
        [0, 1, 1, 1, 1, 1, 0],
        [1, 1, 1, 0, 1, 1, 1],
        [1, 0, 1, 0, 1, 0, 1],
        [1, 0, 1, 0, 1, 0, 1],
    ];

    // 颜色映射：绿色=#00FF9D，黄色=#FFD700
    const colors = {
        green: { r: 0x00, g: 0xFF, b: 0x9D },
        yellow: { r: 0xFF, g: 0xD7, b: 0x00 },
    };
    const c = colors[color];

    const srcSize = 7;
    const scale = 4;           // exact integer scale: 7 * 4 = 28
    const innerSize = srcSize * scale;  // 28
    const outSize = 32;        // standard tray size
    const pad = Math.floor((outSize - innerSize) / 2); // 2px padding

    // Create RGBA Buffer
    const buf = Buffer.alloc(outSize * outSize * 4);
    for (let py = 0; py < outSize; py++) {
        for (let px = 0; px < outSize; px++) {
            const ix = px - pad;
            const iy = py - pad;
            const offset = (py * outSize + px) * 4;
            if (ix >= 0 && ix < innerSize && iy >= 0 && iy < innerSize) {
                const sx = Math.floor(ix / scale);
                const sy = Math.floor(iy / scale);
                if (pixels[sy][sx]) {
                    buf[offset] = c.r;
                    buf[offset + 1] = c.g;
                    buf[offset + 2] = c.b;
                    buf[offset + 3] = 0xFF; // A (fully opaque)
                    continue;
                }
            }
            buf[offset + 3] = 0x00; // fully transparent
        }
    }

    return nativeImage.createFromBuffer(buf, { width: outSize, height: outSize });
}

// 创建系统托盘
function createTray() {
    let trayIcon = createTrayIcon();

    // macOS: 将图标标记为 Template Image（自动适配深/浅色菜单栏）
    if (process.platform === 'darwin') {
        trayIcon.setTemplateImage(true);
    }

    tray = new Tray(trayIcon);
    tray.setToolTip('WhichClaw');

    // 构建并更新右键菜单
    updateTrayMenu();

    // 注册服务器状态变化回调：服务器启停时自动刷新菜单
    setServerStatusChangeCallback(() => updateTrayMenu());

    // 单击托盘图标：显示/聚焦窗口
    tray.on('click', () => {
        if (mainWindow) {
            if (mainWindow.isVisible()) {
                mainWindow.focus();
            } else {
                mainWindow.show();
                mainWindow.focus();
            }
        }
    });

    console.log('[Main] System tray created');
}

// 构建托盘右键菜单（可重复调用以刷新状态）
function updateTrayMenu() {
    if (!tray) return;

    const version = app.getVersion();
    const isServerOnline = isLocalServerRunning();

    // 根据服务器状态切换图标颜色：运行中=黄色，离线=绿色
    const icon = createTrayIcon(isServerOnline ? 'yellow' : 'green');
    if (process.platform === 'darwin') {
        icon.setTemplateImage(true);
    }
    tray.setImage(icon);
    tray.setToolTip(`WhichClaw - ${trayT('tooltip')} ${isServerOnline ? trayT('on') : trayT('off')}`);

    const contextMenu = Menu.buildFromTemplate([
        {
            label: `WhichClaw v${version}`,
            enabled: false,  // 灰色版本号
        },
        { type: 'separator' },
        {
            label: trayT('show'),
            click: () => {
                if (mainWindow) {
                    mainWindow.show();
                    mainWindow.focus();
                }
            }
        },
        { type: 'separator' },
        {
            label: `${trayT('server')} [${isServerOnline ? trayT('on') : trayT('off')}]`,
            submenu: [
                {
                    label: trayT('on'),
                    type: 'radio',
                    checked: isServerOnline,
                    click: () => {
                        if (mainWindow) {
                            mainWindow.show();
                            mainWindow.focus();
                            mainWindow.webContents.send('tray:server-toggle', 'online');
                        }
                        // 立即重建菜单，还原为真实状态（防止 radio 自动切换）
                        setTimeout(() => updateTrayMenu(), 200);
                    }
                },
                {
                    label: trayT('off'),
                    type: 'radio',
                    checked: !isServerOnline,
                    click: () => {
                        if (mainWindow) {
                            mainWindow.show();
                            mainWindow.focus();
                            mainWindow.webContents.send('tray:server-toggle', 'offline');
                        }
                        setTimeout(() => updateTrayMenu(), 200);
                    }
                },
            ]
        },
        { type: 'separator' },
        {
            label: trayT('quit'),
            click: () => {
                isQuitting = true;
                app.quit();
            }
        }
    ]);

    tray.setContextMenu(contextMenu);
}

// Configure Electron cache path to avoid permission issues
// 开发模式：使用项目目录下的 .electron-cache
// 生产模式：使用系统默认 userData 路径（打包后 __dirname 在 asar 内部不可写）
if (!app.isPackaged) {
    const userDataPath = path.join(__dirname, '../.electron-cache');
    app.setPath('userData', userDataPath);
    app.setPath('cache', path.join(userDataPath, 'cache'));
    app.setPath('sessionData', path.join(userDataPath, 'session'));
}

// Linux AppImage 需要禁用 sandbox（SUID sandbox 权限问题）
if (process.platform === 'linux') {
    app.commandLine.appendSwitch('no-sandbox');
}

// Disable GPU cache to avoid cache errors
app.commandLine.appendSwitch('disable-gpu-shader-disk-cache');
app.commandLine.appendSwitch('disable-gpu-program-cache');

// Ensure single instance
const gotTheLock = app.requestSingleInstanceLock();

if (!gotTheLock) {
    app.quit();
} else {
    app.on('second-instance', () => {
        // When second instance runs, focus existing window
        if (mainWindow) {
            if (mainWindow.isMinimized()) mainWindow.restore();
            mainWindow.focus();
        }
    });

    app.whenReady().then(async () => {
        // Initialize Tool Manager
        await toolManager.initialize();

        // 窗口控制 IPC（无边框窗口自定义标题栏用）
        ipcMain.on('window-minimize', () => mainWindow?.minimize());
        ipcMain.on('window-maximize', () => {
            if (mainWindow?.isMaximized()) {
                mainWindow.unmaximize();
            } else {
                mainWindow?.maximize();
            }
        });
        ipcMain.on('window-close', () => mainWindow?.close());
        ipcMain.handle('window-is-maximized', () => mainWindow?.isMaximized() ?? false);

        // 前端渲染完成信号：关闭 splash 显示主窗口
        ipcMain.on('app-ready', () => {
            if (splashWindow) {
                splashWindow.close();
                splashWindow = null;
            }
            mainWindow?.show();
        });

        // Register Local Model IPC
        registerLocalModelHandlers();

        // Start SS Proxy Server
        const ssProxy = await import('./ssProxyServer.js');
        try {
            const port = await ssProxy.startProxyServer();
            console.log(`[Main] SS Proxy server started on port ${port}`);
        } catch (err) {
            console.error('[Main] Failed to start SS Proxy server:', err);
        }

        createSplashWindow();
        createWindow();
        createTray();

        // activate event only on macOS
        app.on('activate', () => {
            if (process.platform === 'darwin' && BrowserWindow.getAllWindows().length === 0) {
                createWindow();
            }
        });
    });
}

app.on('window-all-closed', () => {
    // 有托盘时不退出，所有平台都保持后台运行
    // 只有通过托盘菜单 Quit 才真正退出
});

// 应用退出前的清理
app.on('before-quit', () => {
    isQuitting = true;
    console.log('[Main] App quitting, stopping all tool processes...');
    processManager.stopAllTools();
});

// IPC handlers - Tool scanning
ipcMain.handle('scan-tools', async () => {
    console.log('[IPC] scan-tools called');
    try {
        const result = await toolManager.scanInstalledTools();

        // 为已安装的工具提取图标
        const { toolLoader } = await import('./tools/loader');
        const fs = await import('fs');
        const nodePath = await import('path');
        for (const tool of result) {
            if (tool.installed) {
                try {
                    let exePath = tool.detectedPath || '';

                    // 如果 detectedPath 不是 exe 文件（如配置目录检测的情况），尝试从候选路径找 exe
                    if (!exePath.match(/\.(exe|app)$/i)) {
                        const toolConfig = toolLoader.getTool(tool.id);
                        if (toolConfig) {
                            const candidatePaths = (toolConfig as any).getCandidatePaths?.() || [];
                            for (const p of candidatePaths) {
                                if (fs.existsSync(p) && p.match(/\.(exe|app)$/i)) {
                                    exePath = p;
                                    break;
                                }
                            }
                        }
                    }

                    if (exePath && fs.existsSync(exePath)) {
                        let iconExtracted = false;
                        const exeDir = nodePath.dirname(exePath);
                        const exeName = nodePath.basename(exePath, nodePath.extname(exePath));

                        // 策略1：从 VisualElementsManifest.xml 读取高清 PNG 图标（适用于 Electron/VSCode 类应用）
                        const manifestPath = nodePath.join(exeDir, `${exeName}.VisualElementsManifest.xml`);
                        if (fs.existsSync(manifestPath)) {
                            try {
                                const xml = fs.readFileSync(manifestPath, 'utf-8');
                                // 优先取 150x150，其次 70x70
                                const match150 = xml.match(/Square150x150Logo="([^"]+)"/);
                                const match70 = xml.match(/Square70x70Logo="([^"]+)"/);
                                const logoRelPath = match150?.[1] || match70?.[1];
                                if (logoRelPath) {
                                    const logoFullPath = nodePath.join(exeDir, logoRelPath);
                                    if (fs.existsSync(logoFullPath)) {
                                        const iconBuffer = fs.readFileSync(logoFullPath);
                                        tool.iconBase64 = `data:image/png;base64,${iconBuffer.toString('base64')}`;
                                        iconExtracted = true;
                                        console.log(`[IPC] Extracted high-res icon for ${tool.id} from VisualElementsManifest`);
                                    }
                                }
                            } catch (xmlErr) {
                                console.warn(`[IPC] Failed to parse manifest for ${tool.id}:`, xmlErr);
                            }
                        }

                        // 策略2：fallback 到 app.getFileIcon()（低分辨率但通用）
                        if (!iconExtracted && exePath.match(/\.(exe|app)$/i)) {
                            const icon = await app.getFileIcon(exePath, { size: 'large' });
                            tool.iconBase64 = icon.toDataURL();
                        }
                    }
                } catch (iconErr) {
                    console.warn(`[IPC] Failed to extract icon for ${tool.id}:`, iconErr);
                }
            }
        }

        console.log('[IPC] scan-tools result:', result.map(t => ({ id: t.id, installed: t.installed, hasIcon: !!t.iconBase64 })));
        return result;
    } catch (error) {
        console.error('[IPC] scan-tools error:', error);
        throw error;
    }
});

ipcMain.handle('update-tool-config', async () => {
    return await toolManager.updateOnlineConfig();
});

ipcMain.handle('add-custom-tool', async () => {
    await toolManager.addCustomTool();
    return await toolManager.scanInstalledTools();
});

ipcMain.handle('remove-custom-tool', async () => {
    await toolManager.removeCustomTool();
    return await toolManager.scanInstalledTools();
});

// IPC handlers - Process management
ipcMain.handle('start-tool', async (_event, toolId: string) => {
    console.log('[IPC] start-tool called for:', toolId);
    return await processManager.startTool(toolId);
});

ipcMain.handle('stop-tool', async (_event, toolId: string) => {
    console.log('[IPC] stop-tool called for:', toolId);
    return await processManager.stopTool(toolId);
});

ipcMain.handle('get-running-tools', () => {
    return processManager.getRunningTools();
});

ipcMain.handle('is-tool-running', (_event, toolId: string) => {
    return processManager.isToolRunning(toolId);
});

// 无边框窗口控制 IPC（只注册一次，通过 event.sender 自动定位到发送方窗口）
ipcMain.on('game-window-minimize', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    win?.minimize();
});
ipcMain.on('game-window-close', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    win?.close();
});

// 启动内置游戏/工具（弹出独立窗口）
ipcMain.handle('launch-game', async (_event, toolId: string, launchFile: string, modelConfig?: any) => {
    console.log(`[IPC] launch-game called: ${toolId}, file: ${launchFile}, model:`, modelConfig);
    try {
        // 在工具目录中查找启动文件
        let toolsDir = __dirname;
        if (path.basename(toolsDir) !== 'tools') {
            const potentialToolsDir = path.join(toolsDir, 'tools');
            if (fs.existsSync(potentialToolsDir)) {
                toolsDir = potentialToolsDir;
            }
        }
        const gamePath = path.join(toolsDir, toolId, launchFile);
        console.log(`[IPC] Game file path: ${gamePath}`);

        if (!fs.existsSync(gamePath)) {
            return { success: false, message: `Game file not found: ${gamePath}` };
        }

        // 创建独立游戏窗口（无边框）
        const gameWindow = new BrowserWindow({
            width: 1060,
            height: 700,
            frame: false,
            maximizable: false,
            title: `WhichClaw - ${toolId}`,
            icon: path.join(__dirname, '../public/ico.svg'),
            autoHideMenuBar: true,
            backgroundColor: '#0a0e17',
            webPreferences: {
                nodeIntegration: false,
                contextIsolation: true,
                webSecurity: false, // 允许游戏页面调用外部 API（无 CORS 限制）
                preload: path.join(__dirname, 'tools', toolId, 'preload-game.js'),
            },
        });

        // 解密 apiKey（前端传来的是加密后的 enc:v1:...，工具页面需要明文 key）
        if (modelConfig?.apiKey) {
            const decryptedKey = modelManager.decryptKeyForUse(modelConfig.apiKey);
            // 如果密钥已自毁（解密失败返回空），拒绝启动
            if (modelConfig.apiKey.startsWith('enc:') && !decryptedKey) {
                addAppLog('SECURITY', `Blocked: API Key for "${modelConfig.name || toolId}" has been destroyed, cannot launch ${toolId}`);
                return { success: false, message: `API Key has been destroyed (environment changed). Please re-enter the key.` };
            }
            modelConfig = {
                ...modelConfig,
                apiKey: decryptedKey,
            };
        }

        // 通过 URL hash 传递模型配置（游戏页面可通过 location.hash 读取）
        const modelHash = modelConfig ? '#model=' + encodeURIComponent(JSON.stringify(modelConfig)) : '';
        gameWindow.loadURL(`file://${gamePath}${modelHash}`);

        return { success: true };
    } catch (e: any) {
        console.error('[IPC] launch-game error:', e);
        return { success: false, message: e.message };
    }
});

// IPC Communication Example
ipcMain.handle('get-app-version', () => {
    return app.getVersion();
});

// 前端切换语言时同步到主进程，重建托盘菜单
ipcMain.handle('set-locale', (_event, locale: string) => {
    trayLocale = locale;
    updateTrayMenu();
});

// 前端请求完全退出应用
ipcMain.handle('quit-app', () => {
    isQuitting = true;
    app.quit();
});

// IPC handlers - Skill management
ipcMain.handle('get-skills-list', async () => {
    return await skillManager.getSkillsList();
});

ipcMain.handle('get-skill-categories', () => {
    return skillManager.getSkillCategories();
});

ipcMain.handle('install-skill', async (_event, skillId: string) => {
    return await skillManager.installSkill(skillId);
});

ipcMain.handle('uninstall-skill', async (_event, skillId: string) => {
    return await skillManager.uninstallSkill(skillId);
});

ipcMain.handle('search-skills', async (_event, query: string) => {
    return await skillManager.searchSkills(query);
});

ipcMain.handle('get-skill-details', async (_event, skillId: string) => {
    return await skillManager.getSkillDetails(skillId);
});

// Get installed skills list for specific tool
ipcMain.handle('get-tool-installed-skills', (_event, skillsPath: string) => {
    return skillManager.getToolInstalledSkills(skillsPath);
});

// Open folder
ipcMain.handle('open-folder', async (_event, folderPath: string) => {
    try {
        // 先检查路径是否存在，避免 Windows 弹出原生错误弹窗
        if (!fs.existsSync(folderPath)) {
            return { success: false, error: `Path not found: ${folderPath}` };
        }
        const errorMessage = await shell.openPath(folderPath);
        if (errorMessage) {
            return { success: false, error: errorMessage };
        }
        return { success: true };
    } catch (error) {
        console.error('[Main] Failed to open folder:', error);
        return { success: false, error: String(error) };
    }
});

// Open external URL (System browser)
ipcMain.handle('open-external', async (_event, url: string) => {
    try {
        await shell.openExternal(url);
        return { success: true };
    } catch (error) {
        console.error('[Main] Failed to open external URL:', error);
        return { success: false, error: String(error) };
    }
});

// Set tool model config (Write to tool's settings.json)
ipcMain.handle('set-tool-model', async (_event, toolId: string, modelConfig: {
    name: string;
    baseUrl: string;
    apiKey: string;
    model: string;
}) => {
    try {
        // Tool config path mapping
        const HOME = process.env.HOME || process.env.USERPROFILE || '';
        const toolConfigPaths: Record<string, string> = {
            'claudecode': path.join(HOME, '.claude', 'settings.json'),
            'openclaw': path.join(HOME, '.openclaw', 'settings.json'),
            'opencode': path.join(HOME, '.opencode', 'settings.json'),
            'codex': path.join(HOME, '.codex', 'settings.json'),
        };

        const settingsPath = toolConfigPaths[toolId];
        if (!settingsPath) {
            return { success: false, error: `Unknown tool: ${toolId}` };
        }

        // Read existing config
        let settings: Record<string, unknown> = {};
        if (fs.existsSync(settingsPath)) {
            try {
                const content = fs.readFileSync(settingsPath, 'utf-8');
                settings = JSON.parse(content);
            } catch {
                // File exists but parse failed, use empty object
            }
        }

        // Ensure env object exists
        if (!settings.env || typeof settings.env !== 'object') {
            settings.env = {};
        }
        const env = settings.env as Record<string, string>;

        // Write model config to env
        env['ANTHROPIC_BASE_URL'] = modelConfig.baseUrl;
        env['ANTHROPIC_AUTH_TOKEN'] = modelConfig.apiKey;
        env['ANTHROPIC_MODEL'] = modelConfig.model;
        env['ANTHROPIC_DEFAULT_OPUS_MODEL'] = modelConfig.model;
        env['ANTHROPIC_DEFAULT_SONNET_MODEL'] = modelConfig.model;
        env['ANTHROPIC_DEFAULT_HAIKU_MODEL'] = modelConfig.model;

        // Ensure directory exists
        const dir = path.dirname(settingsPath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }

        // Write config file
        fs.writeFileSync(settingsPath, JSON.stringify(settings, null, 2), 'utf-8');
        console.log(`[Main] Wrote model config to ${settingsPath}:`, modelConfig.name);

        return { success: true, modelName: modelConfig.name };
    } catch (error) {
        console.error('[Main] Failed to set tool model:', error);
        return { success: false, error: String(error) };
    }
});

// Get Claude Code config (MCP Servers, Agents, etc.)
ipcMain.handle('get-claude-config', async (_event, configPath: string) => {
    try {
        const settingsPath = path.join(configPath, 'settings.json');
        if (!fs.existsSync(settingsPath)) {
            return { mcpServers: [], agents: [], env: {} };
        }
        const content = fs.readFileSync(settingsPath, 'utf-8');
        const settings = JSON.parse(content);

        // Parse MCP Servers
        const mcpServers: { name: string; command?: string; url?: string }[] = [];
        if (settings.mcpServers) {
            for (const [name, config] of Object.entries(settings.mcpServers as Record<string, any>)) {
                mcpServers.push({
                    name,
                    command: config.command || config.args?.join(' '),
                    url: config.url
                });
            }
        }

        // Parse Custom Agents
        const agents: { name: string; description?: string }[] = [];
        if (settings.agents) {
            for (const [name, config] of Object.entries(settings.agents as Record<string, any>)) {
                agents.push({
                    name,
                    description: config.description
                });
            }
        }

        return {
            mcpServers,
            agents,
            env: settings.env || {}
        };
    } catch (error) {
        console.error('[Main] Failed to read Claude config:', error);
        return { mcpServers: [], agents: [], env: {} };
    }
});

// ========== Channel Management ==========
import * as channelManager from './channelManager.js';

// Get channel config
ipcMain.handle('get-channels', () => {
    return channelManager.getChannels();
});

// Save channel config
ipcMain.handle('save-channels', (_event, channels: { id: number; name: string; address: string; protocol: string }[]) => {
    channelManager.saveChannels(channels);
    return { success: true };
});

// ========== Model Management ==========
import * as modelManager from './modelManager.js';

// Get all models
ipcMain.handle('get-models', () => {
    return modelManager.getModels();
});

// Get models with decrypted API keys (for sharing via Channels)
ipcMain.handle('get-models-decrypted', () => {
    const models = modelManager.getModels();
    return models.map(m => ({
        ...m,
        apiKey: modelManager.decryptKeyForUse(m.apiKey),
    }));
});

// Add model
ipcMain.handle('add-model', (_event, config: { name: string; baseUrl: string; anthropicUrl?: string; apiKey: string; model: string; proxyUrl?: string }) => {
    const result = modelManager.addModel(config);
    addAppLog('MODEL', `Added model: ${config.name}`);
    return result;
});

// Delete model
ipcMain.handle('delete-model', (_event, id: string) => {
    // 获取模型名用于日志
    const models = modelManager.getModels();
    const model = models.find(m => m.internalId === id);
    const result = modelManager.deleteModel(id);
    addAppLog('MODEL', `Deleted model: ${model?.name || id}`);
    return result;
});

// Update model
ipcMain.handle('update-model', (_event, id: string, updates: { name?: string; baseUrl?: string; apiKey?: string; model?: string; proxyUrl?: string }) => {
    const result = modelManager.updateModel(id, updates);
    addAppLog('MODEL', `Updated model: ${updates.name || id}`);
    return result;
});

// Test model
ipcMain.handle('test-model', async (_event, modelId: string, prompt: string, protocol: 'openai' | 'anthropic' = 'openai') => {
    const result = await modelManager.testModel(modelId, prompt, protocol);
    if (!result.success) {
        addAppLog('ERROR', `Model test failed (${protocol}): ${result.error}`);
    }
    return result;
});

// 切换 API Key 加密状态
ipcMain.handle('toggle-key-encryption', (_event, internalId: string) => {
    return modelManager.toggleKeyEncryption(internalId);
});

// 检测加密密钥是否已自毁
ipcMain.handle('is-key-destroyed', (_event, internalId: string) => {
    const destroyed = modelManager.isKeyDestroyed(internalId);
    if (destroyed) {
        // 获取模型名称用于日志
        const models = modelManager.getModels();
        const model = models.find((m: any) => m.internalId === internalId);
        const modelName = model?.name || internalId;
        addAppLog('SECURITY', `API Key self-destructed: "${modelName}" (environment fingerprint changed)`);
        console.log(`[Security] API Key destroyed for model: ${modelName}`);
    }
    return destroyed;
});

// Ping model server (Simple reachability test)
ipcMain.handle('ping-model', async (_event, modelId: string) => {
    return modelManager.pingModel(modelId);
});

// Get default tool config (from default-tools.json)
ipcMain.handle('get-default-tools-config', () => {
    return toolManager.getDefaultToolsConfig();
});

// ========== Tool Config Management ==========
import * as toolConfigManager from './toolConfigManager.js';

// Apply model to tool config
ipcMain.handle('apply-model-to-tool', async (_event, toolId: string, modelInfo: { id: string; name: string; baseUrl: string; apiKey: string; model: string; proxyUrl?: string; protocol?: string }) => {
    try {
        console.log('[Main] Applying model to tool:', toolId, modelInfo.name);

        // 1. Get full model config (including SS node info)
        const models = modelManager.getModels();
        // Frontend passes internalId
        const fullModelConfig = models.find(m => m.internalId === modelInfo.id);

        let finalProxyUrl = modelInfo.proxyUrl;

        // 2. If SS node configured (TUNNEL mode), enable transparent proxy
        if (fullModelConfig && fullModelConfig.ssNode) {
            console.log(`[Main] Configuring transparent proxy for ${fullModelConfig.name} (${fullModelConfig.baseUrl})`);

            // Start proxy server (if not started)
            // startProxyServer check internally
            const proxyPort = await ssProxyServer.startProxyServer(fullModelConfig.ssNode);

            // Extract domain and add route rules
            try {
                // Prioritize OpenAI URL, then Anthropic URL
                const targetUrlStr = fullModelConfig.baseUrl || fullModelConfig.anthropicUrl;
                if (targetUrlStr) {
                    const url = new URL(targetUrlStr);
                    const hostname = url.hostname;

                    // Add Host Rule: hostname -> SS Node
                    ssProxyServer.addHostRule(hostname, fullModelConfig.ssNode);
                    console.log(`[Main] Added host rule for ${hostname} -> Proxy Port ${proxyPort}`);
                }
            } catch (e) {
                console.error('[Main] Failed to parse URL for host rule:', e);
            }

            // Set tool HTTP proxy address (point to local proxy)
            finalProxyUrl = `http://127.0.0.1:${proxyPort}`;
        }

        // 3. 解密 apiKey（加密密钥需要解密后传给第三方工具）
        const decryptedKey = modelManager.decryptKeyForUse(modelInfo.apiKey);

        // 如果密钥已自毁（解密失败返回空），拒绝应用
        if (modelInfo.apiKey && modelInfo.apiKey.startsWith('enc:') && !decryptedKey) {
            addAppLog('SECURITY', `Blocked: API Key for "${modelInfo.name}" has been destroyed, cannot apply to ${toolId}`);
            return { success: false, message: `API Key for "${modelInfo.name}" has been destroyed (environment changed). Please re-enter the key.` };
        }

        // 4. Apply config to tool
        const result = await toolConfigManager.applyModelToTool(toolId, {
            ...modelInfo,
            apiKey: decryptedKey,
            proxyUrl: finalProxyUrl
        });

        addAppLog('TOOL', `Applied "${modelInfo.name}" to ${toolId}`);
        return result;

    } catch (error: any) {
        console.error('[Main] Error applying model to tool:', error);
        addAppLog('ERROR', `Failed to apply model to ${toolId}: ${error.message}`);
        return { success: false, message: error.message };
    }
});

// Get tool current model config
ipcMain.handle('get-tool-model-info', (_event, toolId: string) => {
    return toolConfigManager.getToolModelInfo(toolId);
});

// ========== SS Proxy Server ==========
import * as ssProxyServer from './ssProxyServer.js';

// Start SS Proxy Server
ipcMain.handle('start-ss-proxy', async () => {
    try {
        const port = await ssProxyServer.startProxyServer();
        return { success: true, port };
    } catch (err: any) {
        return { success: false, error: err.message };
    }
});

// Stop SS Proxy Server
ipcMain.handle('stop-ss-proxy', () => {
    ssProxyServer.stopProxyServer();
    return { success: true };
});

// Add proxy route (return relay URL)
ipcMain.handle('add-ss-proxy-route', (_event, modelId: string, targetUrl: string, ssNode: {
    name: string;
    server: string;
    port: number;
    cipher: string;
    password: string;
}) => {
    const proxyUrl = ssProxyServer.addProxyRoute(modelId, targetUrl, ssNode);
    return { success: true, proxyUrl };
});

// Remove proxy route
ipcMain.handle('remove-ss-proxy-route', (_event, modelId: string) => {
    const removed = ssProxyServer.removeProxyRoute(modelId);
    return { success: removed };
});

// Parse SS URL
ipcMain.handle('parse-ss-url', (_event, ssUrl: string) => {
    const node = ssProxyServer.parseSSUrl(ssUrl);
    return node;
});

// Get proxy port
ipcMain.handle('get-ss-proxy-port', () => {
    return ssProxyServer.getProxyPort();
});

// ========== App Logger ==========
// Check for updates: fetch latest version from remote server
ipcMain.handle('check-for-updates', async () => {
    try {
        const https = await import('https');
        return new Promise((resolve) => {
            const req = https.get('https://whichclaw.com/api/version/index.json', { timeout: 8000 }, (res) => {
                let data = '';
                res.on('data', (chunk: string) => { data += chunk; });
                res.on('end', () => {
                    try {
                        const json = JSON.parse(data);
                        resolve({ success: true, version: json.version });
                    } catch {
                        resolve({ success: false, error: 'Invalid JSON' });
                    }
                });
            });
            req.on('error', (err: Error) => {
                resolve({ success: false, error: err.message });
            });
            req.on('timeout', () => {
                req.destroy();
                resolve({ success: false, error: 'Timeout' });
            });
        });
    } catch (err: any) {
        return { success: false, error: err.message };
    }
});

// Frontend can push logs via IPC
ipcMain.handle('add-app-log', (_event, category: string, message: string) => {
    addAppLog(category as any, message);
    return { success: true };
});

ipcMain.handle('get-app-logs', () => {
    return getAppLogs();
});

ipcMain.handle('clear-app-logs', () => {
    clearAppLogs();
    return { success: true };
});
