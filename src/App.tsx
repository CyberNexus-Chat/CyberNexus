import { useState, useEffect, useRef, useCallback } from 'react';
import { X, Server as ServerIcon } from 'lucide-react';
import { MiniSelect } from './components/MiniSelect';
import { ModelCard, ModelCardSkeleton, ToolCard, Sidebar, PageType, getModelIcon, LocalModelPlayer, ModelStore, ToastProvider, ConfirmDialogProvider } from './components';
import { DownloadProvider } from './components/DownloadContext';
import { DownloadBar } from './components/DownloadBar';
import { TitleBar } from './components/TitleBar';
import { SettingsDialog } from './components/SettingsDialog';
import { Channels } from './components/Channels';
import { CircuitFlow } from './components/CircuitFlow';
import { GatewayProvider } from './contexts/GatewayContext';
import { useI18n } from './hooks/useI18n';

declare const __APP_VERSION__: string;




// Local tool data type
interface LocalTool {
    id: string;
    name: string;
    version: string;
    installed: boolean;
    path?: string;
    icon?: string;
    skillsCount?: number;
    detectedPath?: string;  // CLI tool installation path (for display)
    configPath?: string;    // New: User config file path (for display)
    skillsPath?: string;    // skills/commands config directory (for reading skills)
    activeModel?: string;   // Currently used model
    category?: 'CLI' | 'AgentOS' | 'IDE' | 'AutoTrading' | 'Game' | 'Utility' | 'Custom';  // Tool category
    names?: Record<string, string>;  // i18n tool names (from paths.json)
    website?: string;       // Tool official website / docs link
    apiProtocol?: string[]; // Supported API protocols ['openai', 'anthropic']
    iconBase64?: string;    // 从 exe 提取的图标（base64 格式）
    launchFile?: string;    // 内置可启动文件（如游戏 HTML）
}

// Default tool list
const defaultTools: LocalTool[] = [
    { id: 'claudecode', name: 'ClaudeCode', version: '', installed: false, category: 'CLI' },
    { id: 'openclaw', name: 'OpenClaw', version: '', installed: false, category: 'AgentOS' },
    { id: 'opencode', name: 'OpenCode', version: '', installed: false, category: 'CLI' },
    { id: 'codex', name: 'Codex', version: '', installed: false, category: 'CLI' },
    { id: 'codebuddy', name: 'CodeBuddy', version: '', installed: false, category: 'IDE', website: 'https://www.codebuddy.ai/docs/ide/Introduction' },
    { id: 'codebuddycn', name: 'CodeBuddy CN', version: '', installed: false, category: 'IDE', website: 'https://www.codebuddy.cn/docs/ide/Introduction' },
    { id: 'tradingagents', name: 'TradingAgents', version: '', installed: false, category: 'AutoTrading', website: 'https://github.com/TauricResearch/TradingAgents' },
    { id: 'fingpt', name: 'FinGPT', version: '', installed: false, category: 'AutoTrading', website: 'https://github.com/AI4Finance-Foundation/FinGPT' },
    { id: 'ai-trader', name: 'AI-Trader', version: '', installed: false, category: 'AutoTrading', website: 'https://github.com/HKUDS/AI-Trader' },
];

function App() {
    const [activePage, setActivePage] = useState<PageType>('models');
    const [showSettings, setShowSettings] = useState(false);
    const [showLogsPage, setShowLogsPage] = useState(true);
    const { t, locale: currentLocale, setLocale: setCurrentLocale } = useI18n();
    const [localModelPath, setLocalModelPath] = useState<string>(''); // Selected local model path in ModelStore (right panel)
    const [selectedModel, setSelectedModel] = useState<string | null>('gpt4o');

    // Tool scan state
    const [detectedTools, setDetectedTools] = useState<LocalTool[]>(defaultTools);
    const [isScanning, setIsScanning] = useState(false);
    const [pingingModelIds, setPingingModelIds] = useState<Set<string>>(new Set()); // Model IDs currently being pinged
    const [lastScanTime, setLastScanTime] = useState<Date | null>(null);
    const [selectedTool, setSelectedTool] = useState<string | null>(null);
    const [agentConfigTab, setAgentConfigTab] = useState<'models' | 'skills'>('models');
    const [selectedSkill, setSelectedSkill] = useState<string | null>(null);



    // Skill categories
    const skillCategories = ['All', 'Development', 'Marketing', 'Design', 'Research', 'AI/ML', 'Finance'];
    const [activeSkillCategory, setActiveSkillCategory] = useState('All');

    // Tool categories (Agent Worker)
    const toolCategories = ['ALL', 'AgentOS', 'IDE', 'CLI', 'AutoTrading', 'Game', 'Utility'] as const;
    const [activeToolCategory, setActiveToolCategory] = useState<string>('ALL');
    const [launchAfterApply, setLaunchAfterApply] = useState(true); // 修改模型并启动（默认勾选）
    const [isLaunching, setIsLaunching] = useState(false); // LAUNCH APP 按钮冷却
    const [agreedConfigPolicy, setAgreedConfigPolicy] = useState(true); // 同意配置策略（默认勾选）

    // Skill data (fetched from backend API)
    interface SkillInfo {
        id: string;
        name: string;
        author: string;
        category: string;
        installed: boolean;
        brief?: string;
    }
    const [skillsData, setSkillsData] = useState<SkillInfo[]>([]);
    const [isLoadingSkills, setIsLoadingSkills] = useState(true);
    const [skillSearchQuery, setSkillSearchQuery] = useState('');
    const [skillDescription, setSkillDescription] = useState<string>('');
    const [isLoadingDescription, setIsLoadingDescription] = useState(false);

    // Installed skills for tool (read locally)
    interface InstalledSkillInfo {
        id: string;
        name: string;
        path: string;
        hasReadme: boolean;
        description?: string;
    }
    const [toolInstalledSkills, setToolInstalledSkills] = useState<InstalledSkillInfo[]>([]);
    const [isLoadingToolSkills, setIsLoadingToolSkills] = useState(false);

    // User custom models
    const [userModels, setUserModels] = useState<ModelConfig[]>([]);
    const [showAddModelModal, setShowAddModelModal] = useState(false);
    const [modelModalAnimatingOut, setModelModalAnimatingOut] = useState(false);

    // 统一的模型弹窗关闭函数（带动画）
    const closeModelModal = useCallback(() => {
        setModelModalAnimatingOut(true);
        setTimeout(() => {
            setModelModalAnimatingOut(false);
            setShowAddModelModal(false);
            setEditingModelId(null);
        }, 200);
    }, []);
    const [newModelForm, setNewModelForm] = useState({
        name: '',
        baseUrl: '',        // OpenAI compatible address
        anthropicUrl: '',   // Anthropic address
        apiKey: '',
        modelId: '',  // API Model ID (e.g. gpt-4o, claude-3-opus etc.)
        // SS proxy node config (optional)
        useProxy: false,
        ssServer: '',
        ssPort: '',
        ssCipher: 'aes-128-gcm',
        ssPassword: ''
    });

    // Get selected model data
    const selectedModelData = userModels.find(m => m.internalId === selectedModel);

    // Model test state
    const [testInput, setTestInput] = useState('');
    const [testOutput, setTestOutput] = useState<string[]>([]);  // Terminal output lines
    const [isTesting, setIsTesting] = useState(false);
    const [arrowIndex, setArrowIndex] = useState(0);  // Marquee arrow index
    const [modelLatencies, setModelLatencies] = useState<Record<string, number>>({});  // Record latency for each model
    const [modelTerminals, setModelTerminals] = useState<Record<string, { input: string; output: string[] }>>({});  // Independent terminal history for each model
    const [editingModelId, setEditingModelId] = useState<string | null>(null);  // Currently editing model ID
    const [showApiKey, setShowApiKey] = useState(false);  // Control API Key visibility
    const [keyDestroyed, setKeyDestroyed] = useState(false);  // 加密密钥是否已自毁
    const [applyError, setApplyError] = useState<string | null>(null);  // apply 失败时的错误信息弹窗
    const [testProtocol, setTestProtocol] = useState<'openai' | 'anthropic'>('openai');  // Test protocol selection

    // ===== Logs & Debug 状态 =====
    interface AppLogEntry { timestamp: string; category: string; message: string; }
    const [appLogs, setAppLogs] = useState<AppLogEntry[]>([]);
    const [debugModel, setDebugModel] = useState<string | null>(null); // 用于调试分析的模型 ID
    const [debugInput, setDebugInput] = useState('');
    const [debugOutput, setDebugOutput] = useState<string[]>([]);
    const [isDebugging, setIsDebugging] = useState(false);
    const logsEndRef = useRef<HTMLDivElement>(null);
    const debugEndRef = useRef<HTMLDivElement>(null);
    const debugInputRef = useRef<HTMLInputElement>(null);
    const [debugInputFocused, setDebugInputFocused] = useState(false);
    const [debugCursorPos, setDebugCursorPos] = useState(0);

    // 加载日志并订阅实时推送
    useEffect(() => {
        if (activePage !== 'logs') return;
        // 拉取已有日志
        window.electron?.getAppLogs?.().then(logs => setAppLogs(logs || []));
        // 订阅实时日志
        window.electron?.onAppLog?.((entry) => {
            setAppLogs(prev => [...prev, entry]);
        });
    }, [activePage]);

    // 日志自动滚动到底部
    useEffect(() => {
        logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [appLogs]);

    // 调试输出自动滚动
    useEffect(() => {
        debugEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [debugOutput]);

    // 获取调试选中的模型数据
    const debugModelData = userModels.find(m => m.internalId === debugModel);

    // Send logs to AI for analysis
    const handleSendLogsToAI = useCallback(async () => {
        if (!debugModel || isDebugging) return;
        // Collect recent error logs
        const errorLogs = appLogs.filter(l => l.category === 'ERROR').slice(-10);
        const recentLogs = appLogs.slice(-20);
        const logsToSend = errorLogs.length > 0 ? errorLogs : recentLogs;
        const logsText = logsToSend.map(l => `[${l.timestamp}] [${l.category}] ${l.message}`).join('\n');
        const userMsg = debugInput.trim();
        const prompt = userMsg
            ? `Analyze the following system logs, identify potential issues and provide suggestions:\n\n${logsText}\n\nUser note: ${userMsg}`
            : `Analyze the following system logs, identify potential issues and provide suggestions:\n\n${logsText}`;

        setDebugInput('');
        setIsDebugging(true);
        setDebugOutput(prev => [...prev, `> Sending ${logsToSend.length} logs to AI...`]);

        try {
            const result = await window.electron?.testModel(debugModel, prompt, 'openai');
            if (result?.success) {
                setDebugOutput(prev => [...prev, `Analysis complete (${result.latency}ms):`, result.response || 'No response']);
            } else {
                setDebugOutput(prev => [...prev, `Analysis failed: ${result?.error || 'Unknown error'}`]);
            }
        } catch (e) {
            setDebugOutput(prev => [...prev, `Error: ${String(e)}`]);
        } finally {
            setIsDebugging(false);
        }
    }, [debugModel, debugInput, appLogs, isDebugging]);

    // Debug terminal direct message
    const handleDebugSend = useCallback(async () => {
        if (!debugModel || isDebugging || !debugInput.trim()) return;
        const prompt = debugInput.trim();
        setDebugInput('');
        setIsDebugging(true);
        setDebugOutput(prev => [...prev, `> ${prompt}`]);

        try {
            // 30 秒超时保护：防止网络卡住导致输入永久禁用
            const timeoutPromise = new Promise<any>((_, reject) =>
                setTimeout(() => reject(new Error('Request timeout (30s)')), 30000)
            );
            const result = await Promise.race([
                window.electron?.testModel(debugModel, prompt, 'openai'),
                timeoutPromise
            ]);
            if (result?.success) {
                setDebugOutput(prev => [...prev, `(${result.latency}ms):`, result.response || 'No response']);
            } else {
                setDebugOutput(prev => [...prev, `Error: ${result?.error || 'Unknown error'}`]);
            }
        } catch (e) {
            setDebugOutput(prev => [...prev, `Error: ${String(e)}`]);
        } finally {
            setIsDebugging(false);
        }
    }, [debugModel, debugInput, isDebugging]);

    // Auto-fill Model ID and API Key for local models
    useEffect(() => {
        const isLocal = (url: string) => url.includes('localhost') || url.includes('127.0.0.1');
        const hasLocalUrl = isLocal(newModelForm.baseUrl) || isLocal(newModelForm.anthropicUrl);

        if (hasLocalUrl) {
            setNewModelForm(prev => {
                const updates: any = {};
                if (!prev.modelId) updates.modelId = 'local-model';
                if (!prev.apiKey) updates.apiKey = 'not-needed';

                return Object.keys(updates).length > 0 ? { ...prev, ...updates } : prev;
            });
        }
    }, [newModelForm.baseUrl, newModelForm.anthropicUrl]);

    const testInputRef = useRef<HTMLInputElement>(null);  // Test input ref
    const [inputFocused, setInputFocused] = useState(false);
    const [cursorPos, setCursorPos] = useState(0); // Track input cursor position  // Input focus state
    const [modelProtocolSelection, setModelProtocolSelection] = useState<Record<string, 'openai' | 'anthropic'>>({});  // Agent Worker model protocol selection

    // Marquee animation
    useEffect(() => {
        if (!isTesting) return;
        const timer = setInterval(() => {
            setArrowIndex(prev => (prev + 1) % 4);
        }, 200);
        return () => clearInterval(timer);
    }, [isTesting]);

    // ping --all 独立函数（供按钮和托盘菜单共用）
    const pingAllModels = async () => {
        if (isTesting) return;
        setIsTesting(true);
        const allModels = userModels;
        setPingingModelIds(new Set(allModels.map(m => m.internalId)));
        for (const model of allModels) {
            try {
                const result = await window.electron?.pingModel?.(model.internalId);
                setPingingModelIds(prev => {
                    const next = new Set(prev);
                    next.delete(model.internalId);
                    return next;
                });
                if (result?.success) {
                    setModelLatencies(prev => ({ ...prev, [model.internalId]: result.latency }));
                }
            } catch {
                setPingingModelIds(prev => {
                    const next = new Set(prev);
                    next.delete(model.internalId);
                    return next;
                });
            }
        }
        setPingingModelIds(new Set());
        setIsTesting(false);
    };

    // 监听托盘菜单 server toggle 事件
    useEffect(() => {
        window.electron?.onTrayServerToggle?.((action) => {
            // 切换到 Local Server 页面，用户在前端手动操作启停
            setActivePage('player');
        });
    }, []);

    // Listen for model selection change - auto restore terminal history and focus
    useEffect(() => {
        if (selectedModel && modelTerminals[selectedModel]) {
            const saved = modelTerminals[selectedModel];
            setTestInput(saved?.input || '');
            setTestOutput(saved?.output || []);
        } else {
            setTestInput('');
            setTestOutput([]);
        }
        // Focus input
        testInputRef.current?.focus();
    }, [selectedModel]);

    // Listen for protocol change - focus input
    useEffect(() => {
        testInputRef.current?.focus();
    }, [testProtocol]);

    // Lazy load skills list - load only when opening Skills Browser
    const [skillsLoaded, setSkillsLoaded] = useState(false);
    const loadSkills = async (force = false) => {
        if (skillsLoaded && !force) return; // Already loaded and not forcing refresh

        setIsLoadingSkills(true);
        if (window.electron?.getSkillsList) {
            try {
                const skills = await window.electron.getSkillsList();
                setSkillsData(skills);
                setSkillsLoaded(true);
            } catch (error) {
                console.error('Load skills failed:', error);
            }
        }
        setIsLoadingSkills(false);
    };

    // Load when user switches to Skills page
    useEffect(() => {
        if (activePage === 'skills') {
            loadSkills();
        }
    }, [activePage]);

    // Load user custom models
    const [isLoadingModels, setIsLoadingModels] = useState(true);
    useEffect(() => {
        const loadModels = async () => {
            setIsLoadingModels(true);
            if (window.electron?.getModels) {
                try {
                    const models = await window.electron.getModels();
                    setUserModels(models);
                } catch (error) {
                    console.error('Load models failed:', error);
                }
            }
            setIsLoadingModels(false);
        };
        loadModels();
    }, []);

    // Load installed skills for selected tool - only when switching to SKILLS Tab
    const loadToolSkills = async () => {
        if (!selectedTool || agentConfigTab !== 'skills') {
            return;
        }

        const selectedToolData = detectedTools.find(t => t.id === selectedTool);

        if (!selectedToolData?.skillsPath || !window.electron?.getToolInstalledSkills) {
            setToolInstalledSkills([]);
            return;
        }

        setIsLoadingToolSkills(true);
        try {
            const skills = await window.electron.getToolInstalledSkills(selectedToolData.skillsPath);
            setToolInstalledSkills(skills);
        } catch (error) {
            console.error('Load tool skills failed:', error);
            setToolInstalledSkills([]);
        }
        setIsLoadingToolSkills(false);
    };

    // 用户切换工具 或 切换到 SKILLS Tab 时加载
    useEffect(() => {
        if (agentConfigTab === 'skills' && selectedTool) {
            loadToolSkills();
        } else {
            // Clear when switching tool or Tab
            setToolInstalledSkills([]);
        }
    }, [agentConfigTab, selectedTool]);

    // Load skill description when selected
    useEffect(() => {
        if (!selectedSkill || !window.electron?.getSkillDetails) {
            setSkillDescription('');
            return;
        }

        const loadDescription = async () => {
            setIsLoadingDescription(true);
            try {
                const details = await window.electron.getSkillDetails(selectedSkill);
                setSkillDescription(details.description || '');
            } catch (error) {
                console.error('Load skill details failed:', error);
                setSkillDescription('');
            }
            setIsLoadingDescription(false);
        };
        loadDescription();
    }, [selectedSkill]);

    // Filtered skills list
    // Filtered skills list (category + search)
    const filteredSkills = skillsData
        .filter(s => activeSkillCategory === 'All' || s.category === activeSkillCategory)
        .filter(s => !skillSearchQuery || s.name.toLowerCase().includes(skillSearchQuery.toLowerCase()) || s.author.toLowerCase().includes(skillSearchQuery.toLowerCase()));

    // Tool model config (single selection - one model per tool)
    const [toolModelConfig, setToolModelConfig] = useState<Record<string, string | null>>({
        'claudecode': null,
        'openclaw': null,
        'opencode': null,
        'codex': null,
    });

    // Set tool model (single selection) - UI state update
    const handleSelectModel = (toolId: string, modelId: string) => {
        // Update local state
        setToolModelConfig(prev => ({
            ...prev,
            [toolId]: modelId
        }));
    };

    // Apply model config to backend
    const applyModelConfig = async (toolId: string, internalId: string) => {
        // Find full model info
        const model = userModels.find(m => m.internalId === internalId);
        if (!model) {
            console.error('Model not found:', internalId);
            return false;
        }

        // Find tool apiProtocol to decide which URL to use
        const toolData = detectedTools.find(t => t.id === toolId);
        const toolProtocols = toolData?.apiProtocol || ['openai'];

        // Prioritize user selected protocol in UI, otherwise use tool default
        const userSelectedProtocol = modelProtocolSelection[model.modelId || ''] || modelProtocolSelection[internalId];
        const selectedProtocol = userSelectedProtocol || (toolProtocols[0] === 'anthropic' ? 'anthropic' : 'openai');

        // Use corresponding URL based on selected protocol
        const useAnthropicUrl = selectedProtocol === 'anthropic' && model.anthropicUrl;
        const apiUrl = useAnthropicUrl ? model.anthropicUrl! : model.baseUrl;

        console.log(`[App] Applying model to ${toolId}: protocol=${selectedProtocol}, url=${apiUrl}`);

        // Call backend IPC to write config
        try {
            const result = await window.electron?.applyModelToTool(toolId, {
                id: model.internalId,           // Internal communication ID (m-abc123)
                name: model.name,
                baseUrl: apiUrl,
                apiKey: model.apiKey,
                model: model.modelId || '',      // API Model ID (MiniMAX-2.1)
                proxyUrl: model.proxyUrl,
                protocol: selectedProtocol       // API 协议（openai / anthropic）
            });

            if (result?.success) {
                console.log(`[App] Model ${model.name} applied to ${toolId}`);
                // Rescan tools to update activeModel display (background run, non-blocking UI)
                // scanTools(); // Optimization: No full refresh, only update local state (user feedback: too slow)
                setDetectedTools(prev => prev.map(t =>
                    t.id === toolId ? { ...t, activeModel: model.modelId || model.internalId } : t
                ));
                return true;
            } else {
                console.error('[App] Failed to apply model:', result?.message);
                return result?.message || false;
            }
        } catch (error) {
            console.error('[App] Error applying model to tool:', error);
            return false;
        }
    };

    // Get selected skill data
    const selectedSkillData = skillsData.find(s => s.id === selectedSkill);
    // Get selected tool data
    const selectedToolData = detectedTools.find(t => t.id === selectedTool);

    // Model test function
    const handleTestModel = async () => {
        if (!testInput.trim() || !selectedModel || isTesting) return;

        const prompt = testInput.trim();
        setTestInput('');
        setIsTesting(true);
        // Blur input after sending to stop cursor blinking
        testInputRef.current?.blur();

        // Smart protocol selection: if model only has one protocol configured, auto-use it
        let effectiveProtocol = testProtocol;
        if (selectedModelData) {
            if (!selectedModelData.baseUrl && selectedModelData.anthropicUrl) {
                // Only Anthropic URL configured, force Anthropic protocol
                effectiveProtocol = 'anthropic';
            } else if (selectedModelData.baseUrl && !selectedModelData.anthropicUrl) {
                // Only OpenAI URL configured, force OpenAI protocol
                effectiveProtocol = 'openai';
            }
            // If both configured, use user-selected testProtocol
        }

        // Add user input to output
        setTestOutput(prev => [...prev, `> ${prompt}`, `Sending request via ${effectiveProtocol === 'openai' ? 'OpenAI' : 'Anthropic'}...`]);

        try {
            if (!window.electron?.testModel) {
                setTestOutput(prev => [...prev, 'Test API not available']);
                return;
            }

            const result = await window.electron.testModel(selectedModel, prompt, effectiveProtocol);

            if (result.success) {
                // Update latency record
                setModelLatencies(prev => ({ ...prev, [selectedModel]: result.latency }));
                setTestOutput(prev => [
                    ...prev,
                    `Response in ${result.latency}ms`,
                    result.response || 'No response'
                ]);
                // Reload model list to refresh test status (openaiTested/anthropicTested)
                if (window.electron?.getModels) {
                    const updatedModels = await window.electron.getModels();
                    setUserModels(updatedModels);
                }
            } else {
                setTestOutput(prev => [
                    ...prev,
                    result.error || 'Unknown error',
                    result.latency > 0 ? `(failed after ${result.latency}ms)` : ''
                ].filter(Boolean));
            }
        } catch (error) {
            setTestOutput(prev => [...prev, String(error)]);
        } finally {
            setIsTesting(false);
        }
    };

    // Scan lock to prevent duplicate scans
    const scanInProgress = useRef(false);

    // Detect local tools
    const scanTools = async () => {
        if (scanInProgress.current) {
            console.log('[App] Scan already in progress, skipping');
            return;
        }
        scanInProgress.current = true;

        console.log('[App] Scanning local tools...');
        setIsScanning(true);
        try {
            // Check if in Electron environment
            if (!window.electron?.scanTools) {
                console.log('❌ Not in Electron environment, using default tool list');
                setDetectedTools(defaultTools);
                setLastScanTime(new Date());
                return;
            }

            const tools = await window.electron.scanTools();
            const mappedTools: LocalTool[] = tools.map(tool => ({
                id: tool.id,
                name: tool.name,
                names: (tool as any).names,
                version: tool.version || '',
                installed: tool.installed,
                path: tool.detectedPath,
                skillsCount: tool.installedSkillsCount || 0,
                detectedPath: tool.detectedPath,  // CLI tool installation path
                configPath: tool.configPath,      // User config file path
                skillsPath: tool.skillsPath,      // skills/commands config directory
                activeModel: tool.activeModel,    // Currently used model
                category: tool.category as LocalTool['category'],  // Tool category
                website: tool.website,            // Tool official website / docs link
                apiProtocol: tool.apiProtocol,    // Supported API protocols
                iconBase64: tool.iconBase64,      // 从 exe 提取的图标
                launchFile: (tool as any).launchFile,      // 内置可启动文件
            }));
            // 合并扫描结果与默认工具列表：扫描到的用实际数据，未扫描到的保留默认（显示为未安装）
            const scannedIds = new Set(mappedTools.map(t => t.id));
            const unscannedDefaults = defaultTools.filter(t => !scannedIds.has(t.id));
            setDetectedTools([...mappedTools, ...unscannedDefaults]);
            setLastScanTime(new Date());
        } catch (error) {
            console.error('[App] Scan failed:', error);
            setDetectedTools(defaultTools);
        } finally {
            setIsScanning(false);
            scanInProgress.current = false;
        }
    };

    // Update online config
    const updateConfig = async () => {
        try {
            const updated = await window.electron.updateToolConfig();
            if (updated) {
                await scanTools(); // Re-scan
            }
        } catch (error) {
            console.error('Failed to update config:', error);
        }
    };

    // Scan tools on component mount
    useEffect(() => {
        scanTools().finally(() => {
            // 等 React 渲染完成后通知主进程：关闭 Splash 显示主窗口
            requestAnimationFrame(() => {
                (window as any).electron?.appReady?.();
            });
        });
    }, []);



    // Auto refresh data on page switch (refresh models for Model Nexus and App Manager)
    useEffect(() => {
        if ((activePage === 'models' || activePage === 'agents') && window.electron?.getModels) {
            window.electron.getModels().then(setUserModels);
        }
    }, [activePage]);

    return (
        <ToastProvider>
            <ConfirmDialogProvider>
                <DownloadProvider>
                    <GatewayProvider>
                        <div className="flex flex-col h-screen w-full bg-cyber-bg">
                            {/* 自定义标题栏 */}
                            <TitleBar onSettingsClick={() => setShowSettings(true)} />
                            <div className="flex flex-1 overflow-hidden text-cyber-accent font-mono p-4 gap-4 grid-bg relative">
                                <CircuitFlow />
                                {/* Content layer - above circuit canvas */}
                                <Sidebar activePage={activePage} onPageChange={setActivePage} showLogsPage={showLogsPage} />

                                {/* Main content wrapper */}
                                <div className="flex-1 flex flex-col overflow-hidden relative z-[1]">
                                    {/* Main + Right panel row */}
                                    <div className="flex-1 flex gap-3 overflow-hidden">
                                        <main className="flex-1 flex flex-col overflow-hidden">

                                            {/* Content area */}
                                            <section className="flex-1 flex flex-col overflow-hidden pr-2">
                                                <h2 className="text-xl mb-3 flex-shrink-0 relative flex items-center cjk-title">
                                                    <span className="truncate">
                                                        {activePage === 'models' && t('page.modelNexus')}
                                                        {activePage === 'skills' && <span className="text-cyber-warning">{t('page.skillBrowser')}</span>}
                                                        {activePage === 'agents' && t('page.appManager')}
                                                        {activePage === 'player' && t('page.localServer')}
                                                        {activePage === 'logs' && t('page.logsDebug')}
                                                        {activePage === 'chat' && t('page.channels')}
                                                    </span>
                                                    {activePage === 'models' && (
                                                        <div className="ml-auto flex-shrink-0 flex items-center gap-3">
                                                            <button
                                                                onClick={pingAllModels}
                                                                disabled={isTesting}
                                                                className={`text-xs font-mono px-2 py-1 border rounded transition-colors ${!isTesting
                                                                    ? 'border-cyber-accent/50 text-cyber-accent hover:bg-cyber-accent/10'
                                                                    : 'border-cyber-border text-cyber-text-muted cursor-not-allowed'
                                                                    }`}
                                                            >
                                                                $ ping --all
                                                            </button>
                                                        </div>
                                                    )}
                                                    {activePage === 'skills' && (
                                                        <div className="ml-auto flex-shrink-0 flex items-center gap-2">
                                                            <input
                                                                type="text"
                                                                placeholder={t('search.skills')}
                                                                value={skillSearchQuery}
                                                                onChange={(e) => setSkillSearchQuery(e.target.value)}
                                                                className="w-48 h-7 bg-black/20 border border-cyber-border px-3 text-xs text-cyber-text placeholder-cyber-text-muted focus:border-cyber-warning focus:outline-none transition-colors"
                                                            />
                                                            <button
                                                                onClick={() => loadSkills(true)}
                                                                disabled={isLoadingSkills}
                                                                className="h-7 min-w-[60px] flex items-center justify-center text-[10px] border border-cyber-warning text-cyber-warning px-3 hover:bg-cyber-warning/10 transition-colors disabled:opacity-50"
                                                            >
                                                                {isLoadingSkills ? t('btn.loading') : t('btn.refresh')}
                                                            </button>
                                                        </div>
                                                    )}
                                                </h2>

                                                {activePage === 'models' && (
                                                    <div className="flex-1 overflow-y-auto">
                                                        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">


                                                            {/* Show skeleton when loading */}
                                                            {isLoadingModels ? (
                                                                <>
                                                                    <ModelCardSkeleton />
                                                                    <ModelCardSkeleton />
                                                                    <ModelCardSkeleton />
                                                                    <ModelCardSkeleton />
                                                                </>
                                                            ) : (
                                                                <>
                                                                    {/* User custom models */}
                                                                    {userModels.map(model => {
                                                                        // Smart detect supported protocols
                                                                        const protocols: ('openai' | 'anthropic')[] = [];
                                                                        // If baseUrl exists, support OpenAI protocol
                                                                        if (model.baseUrl) {
                                                                            protocols.push('openai');
                                                                        }
                                                                        // If anthropicUrl exists, support Anthropic protocol
                                                                        if (model.anthropicUrl) {
                                                                            protocols.push('anthropic');
                                                                        }
                                                                        const isDemo = model.type === 'DEMO';
                                                                        return (
                                                                            <ModelCard
                                                                                key={model.internalId}
                                                                                id={model.internalId}
                                                                                name={model.name}
                                                                                type={model.type}
                                                                                baseUrl={model.baseUrl}
                                                                                anthropicUrl={model.anthropicUrl}
                                                                                modelId={model.modelId || ''}
                                                                                hasProxy={!!model.proxyUrl}
                                                                                protocols={protocols}
                                                                                latency={modelLatencies[model.internalId] ?? model.openaiLatency}
                                                                                openaiTested={model.openaiTested}
                                                                                anthropicTested={model.anthropicTested}
                                                                                isPinging={pingingModelIds.has(model.internalId)}
                                                                                selected={selectedModel === model.internalId}
                                                                                isActive={selectedModel === model.internalId}
                                                                                onClick={() => {
                                                                                    // Save current model terminal history
                                                                                    if (selectedModel) {
                                                                                        setModelTerminals(prev => ({
                                                                                            ...prev,
                                                                                            [selectedModel]: { input: testInput, output: testOutput }
                                                                                        }));
                                                                                    }
                                                                                    // Switch model (useEffect will auto restore history, set protocol and focus)
                                                                                    setSelectedModel(model.internalId);
                                                                                    // Auto set default protocol: OpenAI first, then Anthropic
                                                                                    if (model.baseUrl) {
                                                                                        setTestProtocol('openai');
                                                                                    } else if (model.anthropicUrl) {
                                                                                        setTestProtocol('anthropic');
                                                                                    }
                                                                                }}
                                                                                onProtocolClick={(protocol) => {
                                                                                    // Click protocol tag to switch protocol
                                                                                    setTestProtocol(protocol);
                                                                                    // Switch model (useEffect will auto restore history and focus)
                                                                                    if (selectedModel !== model.internalId) {
                                                                                        // Save current model history first
                                                                                        if (selectedModel) {
                                                                                            setModelTerminals(prev => ({
                                                                                                ...prev,
                                                                                                [selectedModel]: { input: testInput, output: testOutput }
                                                                                            }));
                                                                                        }
                                                                                        setSelectedModel(model.internalId);
                                                                                    }
                                                                                }}
                                                                                onEdit={isDemo ? undefined : () => {
                                                                                    // Pre-fill form and open modal
                                                                                    setEditingModelId(model.internalId);
                                                                                    // 检测加密密钥是否已自毁
                                                                                    if (model.apiKey?.startsWith('enc:v1:') && window.electron?.isKeyDestroyed) {
                                                                                        window.electron.isKeyDestroyed(model.internalId).then(destroyed => setKeyDestroyed(destroyed));
                                                                                    } else {
                                                                                        setKeyDestroyed(false);
                                                                                    }
                                                                                    setNewModelForm({
                                                                                        name: model.name,
                                                                                        baseUrl: model.baseUrl,
                                                                                        anthropicUrl: model.anthropicUrl || '',
                                                                                        apiKey: model.apiKey,
                                                                                        modelId: model.modelId || '',
                                                                                        useProxy: !!model.ssNode,
                                                                                        ssServer: model.ssNode?.server || '',
                                                                                        ssPort: model.ssNode?.port?.toString() || '',
                                                                                        ssCipher: model.ssNode?.cipher || 'aes-128-gcm',
                                                                                        ssPassword: model.ssNode?.password || ''
                                                                                    });
                                                                                    setShowAddModelModal(true);
                                                                                }}
                                                                                onDelete={isDemo ? undefined : async () => {
                                                                                    if (window.electron?.deleteModel) {
                                                                                        await window.electron.deleteModel(model.internalId);
                                                                                        setUserModels(prev => prev.filter(m => m.internalId !== model.internalId));
                                                                                    }
                                                                                }}
                                                                            />
                                                                        );
                                                                    })}



                                                                    {/* Add new model button (Gray Border) */}
                                                                    <div
                                                                        className="h-48 border border-dashed border-cyber-border flex flex-col items-center justify-center hover:border-cyber-accent cursor-pointer transition-all rounded-card text-cyber-text-secondary hover:text-cyber-accent"
                                                                        onClick={() => {
                                                                            // Reset form to empty
                                                                            setNewModelForm({
                                                                                name: '',
                                                                                baseUrl: '',
                                                                                anthropicUrl: '',
                                                                                apiKey: '',
                                                                                modelId: '',
                                                                                useProxy: false,
                                                                                ssServer: '',
                                                                                ssPort: '',
                                                                                ssCipher: 'aes-256-gcm',
                                                                                ssPassword: ''
                                                                            });
                                                                            setEditingModelId(null);
                                                                            setShowAddModelModal(true);
                                                                        }}
                                                                    >
                                                                        <span className="font-bold tracking-wider">{t('btn.addModel')}</span>
                                                                        <span className="text-[10px] opacity-60 mt-1">OpenAI / Anthropic API</span>
                                                                    </div>
                                                                </>
                                                            )}
                                                        </div>
                                                    </div>
                                                )}

                                                <div style={{ display: activePage === 'player' ? 'flex' : 'none', height: '100%' }} className="flex-col overflow-hidden">
                                                    <LocalModelPlayer onBack={() => setActivePage('models')} modelFilePath={localModelPath} />
                                                </div>

                                                {activePage === 'skills' && (
                                                    <div className="flex flex-col h-full">
                                                        {/* Category tabs - Bottom slider style */}
                                                        <div className="relative flex gap-1 flex-shrink-0 pb-4 border-b border-cyber-border mb-4">
                                                            {skillCategories.map((cat, index) => (
                                                                <button
                                                                    key={cat}
                                                                    id={`skill-tab-${index}`}
                                                                    onClick={() => setActiveSkillCategory(cat)}
                                                                    className={`relative px-4 py-2 text-xs transition-colors outline-none ${activeSkillCategory === cat
                                                                        ? 'text-cyber-warning font-bold'
                                                                        : 'text-cyber-text-secondary hover:text-cyber-warning'
                                                                        }`}
                                                                >
                                                                    {(() => {
                                                                        const catKeyMap: Record<string, any> = {
                                                                            'All': 'skills.catAll', 'Development': 'skills.catDevelopment',
                                                                            'Marketing': 'skills.catMarketing', 'Design': 'skills.catDesign',
                                                                            'Research': 'skills.catResearch', 'AI/ML': 'skills.catAIML',
                                                                            'Finance': 'skills.catFinance'
                                                                        };
                                                                        return t(catKeyMap[cat] || cat);
                                                                    })()}
                                                                </button>
                                                            ))}
                                                            {/* Independent slide indicator - Positioned relative to selected button */}
                                                            <span
                                                                className="absolute bottom-0 h-0.5 bg-cyber-warning rounded-full transition-all duration-300 ease-out pointer-events-none"
                                                                ref={el => {
                                                                    if (el) {
                                                                        const activeIndex = skillCategories.indexOf(activeSkillCategory);
                                                                        const activeBtn = document.getElementById(`skill-tab-${activeIndex}`);
                                                                        if (activeBtn) {
                                                                            el.style.left = `${activeBtn.offsetLeft}px`;
                                                                            el.style.width = `${activeBtn.offsetWidth}px`;
                                                                        }
                                                                    }
                                                                }}
                                                            />
                                                        </div>
                                                        {/* Skills list - Scrolling area */}
                                                        <div className="flex-1 overflow-y-auto">
                                                            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                                                                {isLoadingSkills ? (
                                                                    /* Skeleton - Loading */
                                                                    [...Array(8)].map((_, i) => (
                                                                        <div
                                                                            key={i}
                                                                            className="p-3 border border-cyber-warning/20 bg-black/20 animate-pulse"
                                                                        >
                                                                            <div className="h-4 bg-cyber-warning/20 w-3/4 mb-2"></div>
                                                                            <div className="h-3 bg-cyber-warning/10 w-1/2"></div>
                                                                        </div>
                                                                    ))
                                                                ) : filteredSkills.length === 0 ? (
                                                                    /* Empty state */
                                                                    <div className="col-span-2 text-center py-10 text-cyber-text-secondary">
                                                                        {t('skills.noSkillsInCategory')}
                                                                    </div>
                                                                ) : (
                                                                    /* Skills list */
                                                                    filteredSkills.map(skill => (
                                                                        <div
                                                                            key={skill.id}
                                                                            className={`p-3 border-l-2 border ${selectedSkill === skill.id ? 'border-l-cyber-warning border-cyber-warning bg-cyber-warning/10' : 'border-l-cyber-border border-cyber-border hover:border-l-cyber-warning/50'} cursor-pointer hover:border-cyber-warning/50 transition-all bg-black/20 flex items-center justify-between group`}
                                                                            onClick={() => setSelectedSkill(skill.id)}
                                                                        >
                                                                            <div className="min-w-0 flex-1">
                                                                                <div className="text-sm font-bold truncate text-cyber-warning group-hover:text-cyber-warning/80">{skill.name}</div>
                                                                                <div className="text-[10px] text-cyber-text-secondary truncate">{skill.brief || skill.author}</div>
                                                                            </div>
                                                                            {/* Skills Browser only displays skill info, no INSTALLED status needed
                                                        That is a Claude local skill, not a core CyberNexus feature */}
                                                                            {/* {skill.installed && (
                                                        <span className="text-[9px] px-1.5 py-0.5 bg-cyber-warning/20 text-cyber-warning rounded ml-2 flex-shrink-0">
                                                            INSTALLED
                                                        </span>
                                                    )} */}
                                                                        </div>
                                                                    ))
                                                                )}
                                                            </div>
                                                        </div>
                                                    </div>
                                                )}

                                                {activePage === 'agents' && (
                                                    <div className="flex-1 flex flex-col overflow-hidden">
                                                        {/* Toolbar - Fixed */}
                                                        {/* Category tabs + Action buttons */}
                                                        <div className="flex items-center justify-between flex-shrink-0 pb-4 mb-4">
                                                            <div className="flex gap-1">
                                                                {toolCategories.map(cat => (
                                                                    <button
                                                                        key={cat}
                                                                        onClick={() => setActiveToolCategory(cat)}
                                                                        className={`px-4 py-2 text-xs transition-colors outline-none ${activeToolCategory === cat
                                                                            ? 'text-cyber-accent font-bold border-b-2 border-cyber-accent'
                                                                            : 'text-cyber-text-secondary hover:text-cyber-accent'
                                                                            }`}
                                                                    >
                                                                        {(() => {
                                                                            const catMap: Record<string, any> = {
                                                                                'ALL': 'toolCat.all', 'AgentOS': 'toolCat.agentOS',
                                                                                'IDE': 'toolCat.ide', 'CLI': 'toolCat.cli',
                                                                                'AutoTrading': 'toolCat.autoTrading', 'Game': 'toolCat.game',
                                                                                'Utility': 'toolCat.utility'
                                                                            };
                                                                            return t(catMap[cat] || cat);
                                                                        })()}
                                                                    </button>
                                                                ))}
                                                            </div>
                                                            <div className="flex items-center gap-2">
                                                                <button
                                                                    onClick={scanTools}
                                                                    disabled={isScanning}
                                                                    className="text-[10px] border border-cyber-accent text-cyber-accent px-3 py-1 hover:bg-cyber-accent/10 transition-colors rounded disabled:opacity-50"
                                                                >
                                                                    {isScanning ? t('status.scanning') : t('btn.refresh')}
                                                                </button>
                                                            </div>
                                                        </div>
                                                        {/* Tool cards - Scrolling */}
                                                        <div className="flex-1 overflow-y-auto">
                                                            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                                                                {detectedTools
                                                                    .filter(tool => activeToolCategory === 'ALL' || tool.category === activeToolCategory)
                                                                    .sort((a, b) => {
                                                                        // 已安装优先
                                                                        if (a.installed !== b.installed) return a.installed ? -1 : 1;
                                                                        // 同安装状态内按分类排序
                                                                        const categoryOrder: Record<string, number> = { 'AgentOS': 0, 'IDE': 1, 'CLI': 2, 'AutoTrading': 3, 'Game': 4, 'Utility': 5 };
                                                                        return (categoryOrder[a.category || ''] ?? 99) - (categoryOrder[b.category || ''] ?? 99);
                                                                    })
                                                                    .map(tool => (
                                                                        <ToolCard
                                                                            key={tool.id}
                                                                            {...tool}
                                                                            selected={selectedTool === tool.id}
                                                                            onClick={() => setSelectedTool(tool.id)}
                                                                        />
                                                                    ))}
                                                            </div>
                                                        </div>
                                                    </div>
                                                )}

                                                {activePage === 'logs' && (
                                                    <div className="flex flex-col h-full">
                                                        {/* 终端标题栏 */}
                                                        <div className="flex items-center justify-between px-4 py-2 border-b border-cyber-border/30 flex-shrink-0">
                                                            <div className="flex items-center gap-2 text-sm font-mono text-cyber-text-secondary">
                                                                <span className="text-cyber-accent-secondary">{'>'}</span>
                                                                <span>{t('log.systemLog')}</span>
                                                                <span className="text-cyber-text-muted/60">|</span>
                                                                <span className="text-[10px] text-cyber-text-muted/70">{appLogs.length} {t('log.entries')}</span>
                                                            </div>
                                                            <button
                                                                onClick={async () => {
                                                                    await window.electron?.clearAppLogs?.();
                                                                    setAppLogs([]);
                                                                }}
                                                                className="text-[10px] font-mono text-cyber-text-muted/60 hover:text-red-400 transition-colors px-2 py-0.5"
                                                            >
                                                                {t('log.clear')}
                                                            </button>
                                                        </div>

                                                        {/* 日志内容区 */}
                                                        <div className="flex-1 overflow-y-auto px-4 py-3 bg-cyber-terminal font-mono text-sm space-y-0.5 custom-scrollbar">
                                                            {/* 欢迎界面 - 始终显示在日志流顶部 */}
                                                            <div className="mb-3 select-none">
                                                                <div className="flex items-center gap-4 py-2">
                                                                    {/* 左侧图标 */}
                                                                    <img src="./ico.svg" alt="CyberNexus" className="w-14 h-14 flex-shrink-0 drop-shadow-[0_0_6px_rgba(0,255,157,0.3)]" />
                                                                    {/* 分隔线 */}
                                                                    <div className="w-px h-12 bg-gradient-to-b from-transparent via-cyber-accent/30 to-transparent flex-shrink-0" />
                                                                    {/* 右侧信息 */}
                                                                    <div className="font-mono text-xs space-y-1">
                                                                        <div className="text-cyber-accent text-sm font-bold tracking-wide">CyberNexus <span className="text-cyber-text-muted/60 text-xs font-normal">v{__APP_VERSION__}</span></div>
                                                                        <div className="text-cyber-text-muted/65 space-y-0.5">
                                                                            <div>{userModels.length} models · {detectedTools.filter(t => t.installed).length}/{detectedTools.length} tools · {navigator.platform}</div>
                                                                        </div>
                                                                    </div>
                                                                </div>
                                                                <div className="text-cyber-accent/15 text-xs font-mono mt-1">{'─'.repeat(52)}</div>
                                                            </div>

                                                            {/* 无日志时显示等待提示 */}
                                                            {appLogs.length === 0 && (
                                                                <div className="text-[11px] text-cyber-text-muted/45 font-mono animate-pulse">{'>'} awaiting_events...</div>
                                                            )}

                                                            {/* 日志条目 */}
                                                            {appLogs.map((log, i) => (
                                                                <div key={i} className="leading-relaxed">
                                                                    <span className="text-cyber-text-muted/50 select-none mr-2 text-xs">{log.timestamp}</span>
                                                                    <span className={`text-xs font-bold mr-2 ${log.category === 'ERROR' ? 'text-red-400' :
                                                                        log.category === 'SECURITY' ? 'text-orange-400' :
                                                                            log.category === 'MODEL' ? 'text-cyber-accent' :
                                                                                log.category === 'DOWNLOAD' ? 'text-cyber-accent-secondary' :
                                                                                    log.category === 'TOOL' ? 'text-cyber-warning' :
                                                                                        'text-cyber-text-muted'
                                                                        }`}>[{log.category}]</span>
                                                                    <span className={`text-xs ${log.category === 'ERROR' || log.category === 'SECURITY' ? 'text-red-400/80' : 'text-cyber-text/80'
                                                                        }`}>{log.message}</span>
                                                                </div>
                                                            ))}
                                                            <div ref={logsEndRef} />
                                                        </div>
                                                    </div>
                                                )}

                                                {/* Main Console: Chat Page — 隐藏而非卸载，保留聊天状态 */}
                                                <div className="flex-1 overflow-hidden" style={{ display: activePage === 'chat' ? undefined : 'none' }}>
                                                    <Channels />
                                                </div>
                                            </section>
                                        </main>

                                        {/* Right panel (hidden on chat page) */}
                                        {activePage !== 'chat' && (
                                            <aside className="w-80 flex flex-col">
                                                {/* Model Library: DEBUG CONSOLE */}
                                                {activePage === 'models' && (
                                                    <>
                                                        <div className="px-4 pt-0.5 pb-3 text-sm flex items-center justify-between bg-transparent">
                                                            <span className="font-mono">{t('debug.console')}</span>
                                                            {selectedModelData && (
                                                                <span className="text-[10px] text-cyber-accent font-mono">
                                                                    {selectedModelData.name}
                                                                </span>
                                                            )}
                                                        </div>
                                                        <div className="flex-1 p-4 overflow-y-auto text-xs font-mono space-y-1 bg-cyber-terminal">
                                                            {selectedModelData ? (
                                                                <div className="space-y-1">
                                                                    <p className="text-cyber-accent">[SYS] Model connected</p>
                                                                    <p className="text-cyber-text-secondary">$ echo $MODEL_ID</p>
                                                                    <p className="text-cyber-accent/80 break-all">{selectedModelData.modelId || selectedModelData.internalId}</p>
                                                                    <p className="text-cyber-text-secondary">$ echo $ENDPOINT ({testProtocol.toUpperCase()})</p>
                                                                    <p className="text-cyber-accent/80 break-all">
                                                                        {testProtocol === 'openai'
                                                                            ? (selectedModelData.baseUrl || 'not set')
                                                                            : (selectedModelData.anthropicUrl || 'not set')}
                                                                    </p>
                                                                    <p className="text-cyber-text-secondary mt-2">$ test --prompt</p>
                                                                    {/* Test output history */}
                                                                    {testOutput.map((line, i) => (
                                                                        <p key={i} className={`break-words ${line.startsWith('Response in') ? 'text-green-400' :
                                                                            line.includes('HTTP 4') || line.includes('HTTP 5') || line.includes('error') || line.includes('Error') || line.includes('failed') ? 'text-red-400' :
                                                                                line.startsWith('Sending') ? 'text-cyber-accent' :
                                                                                    line.startsWith('>') ? 'text-white' :
                                                                                        'text-cyber-text-muted/80'
                                                                            }`}>{line}</p>
                                                                    ))}
                                                                    {isTesting ? (
                                                                        <p className="text-cyber-accent font-mono">[EXEC] <span className="inline-block w-8 text-left">{['>', '>>', '>>>', ''][arrowIndex]}</span> transmitting...</p>
                                                                    ) : (
                                                                        <p className="text-cyber-accent">_ ready</p>
                                                                    )}
                                                                </div>
                                                            ) : (
                                                                <p className="text-cyber-text-secondary text-center py-10">
                                                                    {t('model.selectToTest')}
                                                                </p>
                                                            )}
                                                        </div>
                                                        <div className="py-3">
                                                            <div
                                                                className="flex items-center gap-2 bg-cyber-terminal p-2 cursor-text"
                                                                onClick={() => testInputRef.current?.focus()}
                                                            >
                                                                {/* Clickable protocol selector ~\OpenAI > or ~\Anthropic > */}
                                                                <button
                                                                    onClick={(e) => {
                                                                        e.stopPropagation();  // Stop propagation to outer div
                                                                        // Allow switch only if corresponding URL is configured
                                                                        if (testProtocol === 'openai' && selectedModelData?.anthropicUrl) {
                                                                            setTestProtocol('anthropic');
                                                                        } else if (testProtocol === 'anthropic' && selectedModelData?.baseUrl) {
                                                                            setTestProtocol('openai');
                                                                        }
                                                                        // If target protocol URL not configured, do nothing
                                                                    }}
                                                                    className="text-xs font-mono select-none whitespace-nowrap text-cyber-accent cursor-pointer"
                                                                >
                                                                    {/* Display based on config: ~\OpenAI > or ~\Anthropic > or ~\ > (none configured) */}
                                                                    ~\{(selectedModelData?.baseUrl || selectedModelData?.anthropicUrl)
                                                                        ? (testProtocol === 'openai' ? 'OpenAI' : 'Anthropic')
                                                                        : ''} {'>'}
                                                                </button>
                                                                <div className="flex-1 relative flex items-center">
                                                                    <input
                                                                        ref={testInputRef}
                                                                        type="text"
                                                                        placeholder=""
                                                                        value={testInput}
                                                                        onChange={(e) => {
                                                                            setTestInput(e.target.value);
                                                                            setCursorPos(e.target.selectionStart || 0);
                                                                        }}
                                                                        onFocus={() => setInputFocused(true)}
                                                                        onBlur={() => setInputFocused(false)}
                                                                        onSelect={(e) => setCursorPos(e.currentTarget.selectionStart || 0)}
                                                                        onClick={(e) => setCursorPos(e.currentTarget.selectionStart || 0)}
                                                                        onKeyUp={(e) => {
                                                                            setCursorPos(e.currentTarget.selectionStart || 0);
                                                                            if (e.key === 'Enter' && !isTesting) {
                                                                                handleTestModel();
                                                                            }
                                                                        }}
                                                                        className="w-full bg-transparent text-xs font-mono focus:outline-none text-cyber-text"
                                                                        disabled={!selectedModelData || isTesting}
                                                                        style={{ caretColor: 'transparent' }}
                                                                    />
                                                                    {/* Custom underscore cursor - follows input position */}
                                                                    <div className="absolute inset-0 flex items-end pb-[2px] pointer-events-none text-xs font-mono text-cyber-text overflow-hidden whitespace-pre">
                                                                        <span className="invisible">{testInput.slice(0, cursorPos)}</span>
                                                                        {inputFocused && (
                                                                            <span
                                                                                className="inline-block w-[0.6em] h-[2px] bg-cyber-accent shadow-[0_0_8px_rgba(0,255,157,0.8)]"
                                                                                style={{ animation: 'blink 1s step-end infinite' }}
                                                                            ></span>
                                                                        )}
                                                                    </div>
                                                                </div>
                                                            </div>
                                                        </div>
                                                    </>
                                                )}

                                                {/* Skill Browser: SKILL DETAILS */}
                                                {activePage === 'skills' && (
                                                    <>
                                                        <div className="px-4 pt-0.5 pb-3 text-sm flex items-center justify-between bg-transparent">
                                                            <span className="text-cyber-warning">{t('skills.details')}</span>
                                                            {selectedSkillData && (
                                                                <span className="text-[10px] text-cyber-warning">
                                                                    {selectedSkillData.category}
                                                                </span>
                                                            )}
                                                        </div>
                                                        <div className="flex-1 p-4 overflow-y-auto">
                                                            {selectedSkillData ? (
                                                                <div className="space-y-4">
                                                                    <div>
                                                                        <h3 className="text-lg font-bold text-cyber-warning mb-2">{selectedSkillData.name}</h3>
                                                                        <div className="text-xs space-y-1 text-cyber-text-secondary">
                                                                            <div>{t('skills.author')}: <span className="text-cyber-text">{selectedSkillData.author}</span></div>
                                                                            <div>{t('skills.category')}: <span className="text-cyber-text">{selectedSkillData.category}</span></div>
                                                                        </div>
                                                                    </div>
                                                                    <div className="border-t border-cyber-border pt-4">
                                                                        <div className="text-xs text-cyber-text-secondary mb-2">{t('skills.description')}</div>
                                                                        {isLoadingDescription ? (
                                                                            <div className="text-sm text-cyber-text-secondary">Loading...</div>
                                                                        ) : (
                                                                            <p className="text-sm text-cyber-text leading-relaxed whitespace-pre-wrap">
                                                                                {skillDescription || t('skills.noDescription')}
                                                                            </p>
                                                                        )}
                                                                    </div>
                                                                    <div className="border-t border-cyber-border pt-4">
                                                                        <div className="text-xs text-cyber-text-secondary mb-2">GITHUB</div>
                                                                        <span
                                                                            onClick={(e) => { e.preventDefault(); (window as any).electron?.openExternal(`https://github.com/${selectedSkillData.author}`); }}
                                                                            className="text-sm text-cyber-warning hover:underline cursor-pointer"
                                                                        >
                                                                            github.com/{selectedSkillData.author}
                                                                        </span>
                                                                    </div>
                                                                </div>
                                                            ) : (
                                                                <p className="text-cyber-text-secondary text-center py-10">
                                                                    {t('skills.selectToView')}
                                                                </p>
                                                            )}
                                                        </div>
                                                        {selectedSkillData && (
                                                            <div className="p-3 bg-black/20 border-t border-cyber-border">
                                                                <span
                                                                    onClick={(e) => { e.preventDefault(); (window as any).electron?.openExternal(`https://github.com/${selectedSkillData.author}/tree/main/skills/${selectedSkillData.id}`); }}
                                                                    className="w-full py-2 text-sm font-bold transition-colors bg-cyber-warning text-black hover:bg-cyber-warning/80 flex items-center justify-center gap-2 cursor-pointer"
                                                                >
                                                                    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
                                                                        <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
                                                                    </svg>
                                                                    {t('skills.viewGithub')}
                                                                </span>
                                                            </div>
                                                        )}
                                                    </>
                                                )}

                                                {/* Agent Worker: CONFIG PANEL WITH TABS */}
                                                {activePage === 'agents' && (
                                                    <>
                                                        {/* Tab Header */}
                                                        <div className="p-2 flex items-center justify-between bg-transparent">
                                                            <div className="flex gap-1">
                                                                <button
                                                                    onClick={() => setAgentConfigTab('models')}
                                                                    className={`px-3 py-1.5 text-xs font-bold rounded transition-colors ${agentConfigTab === 'models'
                                                                        ? 'bg-cyber-accent text-black'
                                                                        : 'text-cyber-text-secondary hover:text-cyber-text'
                                                                        }`}
                                                                >
                                                                    {t('agent.modelsTab')}
                                                                </button>
                                                                <button
                                                                    onClick={() => setAgentConfigTab('skills')}
                                                                    className={`px-3 py-1.5 text-xs font-bold rounded transition-colors ${agentConfigTab === 'skills'
                                                                        ? 'bg-cyber-warning text-black'
                                                                        : 'text-cyber-text-secondary hover:text-cyber-text'
                                                                        }`}
                                                                >
                                                                    {t('agent.skillsTab')}
                                                                </button>
                                                            </div>
                                                            {selectedToolData && (
                                                                <span className="text-[10px] text-cyber-accent">
                                                                    {selectedToolData.name}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {/* Content */}
                                                        <div className="flex-1 p-4 overflow-y-auto">
                                                            {selectedToolData ? (
                                                                <>
                                                                    {/* Models Tab */}
                                                                    {agentConfigTab === 'models' && (
                                                                        <div className="space-y-2">
                                                                            {/* 模型列表：先分离本地模型和云端模型 */}
                                                                            {(() => {
                                                                                const toolProtocols = selectedToolData.apiProtocol || ['openai', 'anthropic'];
                                                                                const compatibleModels = userModels.filter(model => {
                                                                                    const hasOpenAI = toolProtocols.includes('openai') && !!model.baseUrl;
                                                                                    const hasAnthropic = toolProtocols.includes('anthropic') && !!model.anthropicUrl;
                                                                                    return hasOpenAI || hasAnthropic;
                                                                                });

                                                                                const localModels = compatibleModels.filter(m => m.internalId === 'local-server');
                                                                                const cloudModels = compatibleModels.filter(m => m.internalId !== 'local-server');

                                                                                // 渲染单个模型卡片的函数
                                                                                const renderModelCard = (model: typeof userModels[0]) => {
                                                                                    const isSelected = toolModelConfig[selectedTool!] === model.internalId;
                                                                                    const isLocalServer = model.internalId === 'local-server';

                                                                                    const modelHasBoth = !!(model.baseUrl && model.anthropicUrl);
                                                                                    const toolSupportsBoth = toolProtocols.includes('openai') && toolProtocols.includes('anthropic');
                                                                                    const showSwitcher = modelHasBoth && toolSupportsBoth;

                                                                                    let currentProtocol = 'openai';
                                                                                    if (toolSupportsBoth) {
                                                                                        currentProtocol = modelProtocolSelection[model.modelId || ''] ||
                                                                                            (toolProtocols[0] === 'anthropic' ? 'anthropic' : 'openai');
                                                                                    } else {
                                                                                        currentProtocol = toolProtocols[0];
                                                                                    }

                                                                                    const displayUrl = currentProtocol === 'anthropic'
                                                                                        ? (model.anthropicUrl || model.baseUrl)
                                                                                        : (model.baseUrl || model.anthropicUrl);
                                                                                    const apiPath = (() => {
                                                                                        try {
                                                                                            const url = new URL(displayUrl || '');
                                                                                            const path = url.pathname === '/' ? '' : url.pathname;
                                                                                            return url.hostname + path;
                                                                                        } catch {
                                                                                            return displayUrl || 'No URL Configured';
                                                                                        }
                                                                                    })();

                                                                                    return (
                                                                                        <div
                                                                                            key={model.internalId}
                                                                                            className={`p-3 border rounded cursor-pointer transition-all mb-2 flex items-center gap-3 ${isSelected
                                                                                                ? isLocalServer
                                                                                                    ? 'border-cyan-400 bg-cyan-400/10'
                                                                                                    : 'border-cyber-accent bg-cyber-accent/10'
                                                                                                : isLocalServer
                                                                                                    ? 'border-cyan-400/50 hover:border-cyan-400 bg-cyan-400/5'
                                                                                                    : 'border-cyber-border hover:border-cyber-accent/50'
                                                                                                }`}
                                                                                            onClick={() => handleSelectModel(selectedTool!, model.internalId)}
                                                                                        >
                                                                                            {/* Left: Radio + Icon */}
                                                                                            <div className="flex items-center gap-3 flex-shrink-0">
                                                                                                <div className={`w-4 h-4 rounded-full border-2 flex items-center justify-center ${isSelected
                                                                                                    ? isLocalServer ? 'border-cyan-400' : 'border-cyber-accent'
                                                                                                    : 'border-cyber-border'
                                                                                                    }`}>
                                                                                                    {isSelected && <div className={`w-2 h-2 rounded-full ${isLocalServer ? 'bg-cyan-400' : 'bg-cyber-accent'}`} />}
                                                                                                </div>
                                                                                                {isLocalServer ? (
                                                                                                    <div className="w-6 h-6 rounded bg-cyan-400/15 flex items-center justify-center text-cyan-400">
                                                                                                        <ServerIcon size={14} />
                                                                                                    </div>
                                                                                                ) : (
                                                                                                    getModelIcon(model.name, model.modelId || '') && (
                                                                                                        <img src={getModelIcon(model.name, model.modelId || '')!} alt="" className="w-6 h-6" />
                                                                                                    )
                                                                                                )}
                                                                                            </div>

                                                                                            {/* Right: Two-row layout */}
                                                                                            <div className="flex-1 min-w-0 flex flex-col justify-center h-10">
                                                                                                <div className="flex items-center gap-2">
                                                                                                    <div className={`text-sm font-bold truncate leading-none flex-1 min-w-0 ${isLocalServer ? 'text-cyan-400' : ''}`}>{model.name || 'Untitled Model'}</div>
                                                                                                    {showSwitcher && (
                                                                                                        <span
                                                                                                            className={`text-[10px] font-mono cursor-pointer select-none flex-shrink-0 transition-colors ${isLocalServer
                                                                                                                ? 'text-cyan-400/60 hover:text-cyan-400'
                                                                                                                : 'text-cyber-text-muted/60 hover:text-cyber-accent'
                                                                                                                }`}
                                                                                                            onClick={(e) => {
                                                                                                                e.stopPropagation();
                                                                                                                const newProtocol = currentProtocol === 'openai' ? 'anthropic' : 'openai';
                                                                                                                setModelProtocolSelection(prev => ({ ...prev, [model.modelId || '']: newProtocol }));
                                                                                                            }}
                                                                                                        >
                                                                                                            {currentProtocol === 'openai' ? 'OpenAI' : 'Anthropic'} <span className="text-[8px]">⇄</span>
                                                                                                        </span>
                                                                                                    )}
                                                                                                </div>
                                                                                                <div className="text-[10px] text-cyber-text-secondary truncate leading-tight mt-1 opacity-70">
                                                                                                    {apiPath}
                                                                                                </div>
                                                                                            </div>
                                                                                        </div>
                                                                                    );
                                                                                };

                                                                                return (
                                                                                    <>
                                                                                        {/* 本地模型区域 */}
                                                                                        {localModels.length > 0 && (
                                                                                            <div className="mb-4">
                                                                                                <div className="text-xs text-cyan-400/80 mb-2">
                                                                                                    {t('agent.myLocalModel')}:
                                                                                                </div>
                                                                                                {localModels.map(renderModelCard)}
                                                                                            </div>
                                                                                        )}

                                                                                        {/* 云端模型区域 */}
                                                                                        <div className="text-xs text-cyber-text-secondary mb-3">
                                                                                            {t('agent.selectModelFor')} {selectedToolData.name}:
                                                                                        </div>
                                                                                        {cloudModels.length > 0 ? (
                                                                                            <div className="space-y-2">
                                                                                                {cloudModels.map(renderModelCard)}
                                                                                            </div>
                                                                                        ) : null}
                                                                                    </>
                                                                                );
                                                                            })()}
                                                                        </div>
                                                                    )}

                                                                    {/* Skills Tab - Display installed skills for tool */}
                                                                    {agentConfigTab === 'skills' && (
                                                                        <div className="space-y-2">
                                                                            <div className="text-xs text-cyber-text-secondary mb-3">
                                                                                {t('agent.installedSkillsFor')} {selectedToolData.name}:
                                                                            </div>
                                                                            {/* Display skills path - Click to open folder */}
                                                                            {selectedToolData.skillsPath && (
                                                                                <div
                                                                                    className="text-[10px] text-cyber-warning mb-2 p-2 bg-cyber-warning/10 rounded truncate cursor-pointer hover:bg-cyber-warning/20 transition-colors"
                                                                                    onClick={() => window.electron?.openFolder(selectedToolData.skillsPath!)}
                                                                                >
                                                                                    📁 {selectedToolData.skillsPath}
                                                                                </div>
                                                                            )}
                                                                            {isLoadingToolSkills ? (
                                                                                <div className="text-center py-8">
                                                                                    <div className="text-cyber-text-secondary text-sm">{t('skills.loading')}</div>
                                                                                </div>
                                                                            ) : toolInstalledSkills.length > 0 ? (
                                                                                toolInstalledSkills.map(skill => (
                                                                                    <div
                                                                                        key={skill.id}
                                                                                        className="p-3 border border-cyber-border rounded hover:border-cyber-warning/50 transition-all cursor-pointer select-none"
                                                                                    >
                                                                                        <div className="flex items-center justify-between gap-2">
                                                                                            <div className="flex-1 min-w-0">
                                                                                                <div className="text-sm font-bold truncate text-cyber-warning">{skill.name}</div>
                                                                                                <div className="text-[10px] text-cyber-text-secondary truncate">
                                                                                                    .../{skill.path.split(/[/\\]/).slice(-3).join('/')}
                                                                                                </div>
                                                                                            </div>
                                                                                            <button
                                                                                                onClick={() => window.electron?.openFolder(skill.path)}
                                                                                                className="px-3 py-1 text-[10px] font-bold rounded border border-cyber-warning text-cyber-warning hover:bg-cyber-warning/10 transition-colors"
                                                                                            >
                                                                                                {t('btn.open')}
                                                                                            </button>
                                                                                        </div>
                                                                                    </div>
                                                                                ))
                                                                            ) : (
                                                                                <div className="text-center py-8">
                                                                                    <div className="text-cyber-text-secondary text-sm">{t('agent.noSkills')}</div>
                                                                                </div>
                                                                            )}
                                                                        </div>
                                                                    )}
                                                                </>
                                                            ) : (
                                                                <p className="text-cyber-text-secondary text-center py-10">
                                                                    {t('agent.selectTool')}
                                                                </p>
                                                            )}
                                                        </div>

                                                    </>
                                                )}

                                                {/* LOCAL RUNTIME: MODEL STORE */}
                                                <div style={{ display: activePage === 'player' ? 'block' : 'none', height: '100%' }}>
                                                    <ModelStore onSelectModel={(path) => setLocalModelPath(path)} />
                                                </div>

                                                {/* Logs & Debug: DEBUG_CONSOLE */}
                                                {activePage === 'logs' && (
                                                    <>
                                                        <div className="px-2 pt-0.5 pb-3 text-sm flex items-center justify-between bg-transparent">
                                                            <span className="font-mono text-cyber-accent-secondary">{t('debug.console')}</span>
                                                            {debugModelData && (
                                                                <span className="text-[10px] text-cyber-accent font-mono">
                                                                    {debugModelData.name}
                                                                </span>
                                                            )}
                                                        </div>

                                                        {/* 模型选择器 */}
                                                        <div className="pb-2">
                                                            <MiniSelect
                                                                value={debugModel || ''}
                                                                onChange={(val) => {
                                                                    setDebugModel(val || null);
                                                                    setDebugOutput([]);
                                                                }}
                                                                options={[
                                                                    { id: '', label: t('debug.selectModelForAI') },
                                                                    ...userModels.map(m => ({ id: m.internalId, label: m.name }))
                                                                ]}
                                                            />
                                                        </div>

                                                        {/* 调试输出区 */}
                                                        <div className="flex-1 p-4 overflow-y-auto text-xs font-mono space-y-1 bg-cyber-terminal">
                                                            {debugModel ? (
                                                                <div className="space-y-1">
                                                                    <p className="text-cyber-accent-secondary">{t('debug.ready')}</p>
                                                                    <p className="text-cyber-text-secondary">$ model = {debugModelData?.name || 'unknown'}</p>
                                                                    {debugOutput.map((line, i) => (
                                                                        <p key={i} className={`break-words whitespace-pre-wrap ${line.startsWith('> ') ? 'text-white' :
                                                                            line.includes('Analysis complete') || line.includes('ms):') ? 'text-green-400' :
                                                                                line.includes('failed') || line.includes('Error') ? 'text-red-400' :
                                                                                    line.startsWith('> Sending') ? 'text-cyber-accent-secondary' :
                                                                                        'text-cyber-text-muted/80'
                                                                            }`}>{line}</p>
                                                                    ))}
                                                                    {isDebugging ? (
                                                                        <p className="text-cyber-accent-secondary font-mono">{t('debug.analyzing')}</p>
                                                                    ) : (
                                                                        <p className="text-cyber-accent-secondary">{t('debug.idle')}</p>
                                                                    )}
                                                                    <div ref={debugEndRef} />
                                                                </div>
                                                            ) : (
                                                                <p className="text-cyber-text-secondary text-center py-10">
                                                                    {t('debug.selectModelHint')}
                                                                </p>
                                                            )}
                                                        </div>

                                                        {/* 底部操作区 - 与 Model Nexus 一致，外层 py-3 无横向 padding */}
                                                        <div className="py-3 space-y-2">
                                                            {/* 一键发送日志按钮 */}
                                                            <button
                                                                onClick={handleSendLogsToAI}
                                                                disabled={!debugModel || isDebugging || appLogs.length === 0}
                                                                className="w-full text-[10px] font-mono py-1.5 border border-cyber-accent-secondary/30 text-cyber-accent-secondary hover:bg-cyber-accent-secondary/10 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
                                                            >
                                                                📋 {t('btn.sendLogs')} ({appLogs.filter(l => l.category === 'ERROR').length} {t('debug.errors')})
                                                            </button>
                                                            {/* 输入框 - bg-cyber-terminal 撑满面板宽度 */}
                                                            <div
                                                                className="flex items-center gap-2 bg-cyber-terminal p-2 cursor-text"
                                                                onClick={() => debugInputRef.current?.focus()}
                                                            >
                                                                <span className="text-xs font-mono text-cyber-accent-secondary select-none">~\Debug {'>'}</span>
                                                                <div className="flex-1 relative flex items-center">
                                                                    <input
                                                                        ref={debugInputRef}
                                                                        type="text"
                                                                        value={debugInput}
                                                                        onChange={(e) => {
                                                                            setDebugInput(e.target.value);
                                                                            setDebugCursorPos(e.target.selectionStart || 0);
                                                                        }}
                                                                        onFocus={() => setDebugInputFocused(true)}
                                                                        onBlur={() => setDebugInputFocused(false)}
                                                                        onSelect={(e) => setDebugCursorPos(e.currentTarget.selectionStart || 0)}
                                                                        onKeyUp={(e) => {
                                                                            setDebugCursorPos(e.currentTarget.selectionStart || 0);
                                                                            if (e.key === 'Enter' && !isDebugging) {
                                                                                handleDebugSend();
                                                                            }
                                                                        }}
                                                                        className="w-full bg-transparent text-xs font-mono focus:outline-none text-cyber-text"
                                                                        disabled={!debugModel || isDebugging}
                                                                        placeholder={debugModel ? '' : t('debug.selectModelFirst')}
                                                                        style={{ caretColor: 'transparent' }}
                                                                    />
                                                                    {/* 自定义下划线光标 */}
                                                                    <div className="absolute inset-0 flex items-end pb-[2px] pointer-events-none text-xs font-mono text-cyber-text overflow-hidden whitespace-pre">
                                                                        <span className="invisible">{debugInput.slice(0, debugCursorPos)}</span>
                                                                        {debugInputFocused && (
                                                                            <span
                                                                                className="inline-block w-[0.6em] h-[2px] bg-cyber-accent-secondary shadow-[0_0_8px_rgba(0,212,255,0.8)]"
                                                                                style={{ animation: 'blink 1s step-end infinite' }}
                                                                            ></span>
                                                                        )}
                                                                    </div>
                                                                </div>
                                                            </div>
                                                        </div>
                                                    </>
                                                )}
                                            </aside>
                                        )}
                                    </div>

                                    {/* 底部启动区 - 固定在最下方，不随内容滚动 */}
                                    {activePage === 'agents' && (
                                        <div className="flex-shrink-0 flex flex-col mt-2">
                                            <div className="mx-2 border-t border-cyber-border"></div>
                                            <div className="flex items-center justify-end gap-8 px-6 py-5">
                                                {/* 开发者邀请提示 */}
                                                <div className="flex-1 text-[13px] text-cyber-text-muted/40">{t('hint.devInvite')}</div>
                                                {/* 左侧：固定宽度按钮 */}
                                                <button
                                                    onClick={async () => {
                                                        if (!selectedTool || isLaunching) return;
                                                        setIsLaunching(true);
                                                        setTimeout(() => setIsLaunching(false), 3000); // 3 秒冷却
                                                        // 始终应用模型配置（如果有选择模型）
                                                        if (toolModelConfig[selectedTool]) {
                                                            const applyResult = await applyModelConfig(selectedTool, toolModelConfig[selectedTool]!);
                                                            if (applyResult !== true) {
                                                                // 应用失败（如密钥已自毁），弹窗提示并阻止后续启动
                                                                setApplyError(t('key.destroyed'));
                                                                setIsLaunching(false);
                                                                return;
                                                            }
                                                        }
                                                        // 仅在勾选时启动工具
                                                        if (launchAfterApply) {
                                                            const toolData = detectedTools.find(t => t.id === selectedTool);
                                                            if (toolData?.launchFile) {
                                                                // 可启动工具（如游戏）：弹出独立窗口，传递模型配置
                                                                const selectedModelId = toolModelConfig[selectedTool];
                                                                const selectedModel = selectedModelId ? userModels.find(m => m.internalId === selectedModelId) : undefined;
                                                                const modelConfig = selectedModel ? {
                                                                    baseUrl: selectedModel.baseUrl,
                                                                    anthropicUrl: selectedModel.anthropicUrl,
                                                                    apiKey: selectedModel.apiKey,
                                                                    model: selectedModel.modelId || selectedModel.name || 'unknown',
                                                                    name: selectedModel.name,
                                                                    protocol: modelProtocolSelection[selectedModel.modelId || ''] || 'openai',
                                                                } : undefined;
                                                                const result = await (window as any).electron?.launchGame?.(selectedTool, toolData.launchFile, modelConfig);
                                                                if (result && !result.success) {
                                                                    console.error('Failed to launch:', result.message);
                                                                }
                                                            } else {
                                                                // 常规工具：启动 CLI 进程
                                                                const result = await window.electron?.startTool(selectedTool);
                                                                if (!result?.success) {
                                                                    console.error('Failed to launch tool:', result?.error);
                                                                }
                                                            }
                                                        }
                                                    }}
                                                    disabled={!selectedTool || !agreedConfigPolicy || !toolModelConfig[selectedTool] || isLaunching}
                                                    className={`w-64 h-14 text-lg font-bold font-mono tracking-widest transition-all flex-shrink-0 rounded-lg cjk-btn ${(!selectedTool || !agreedConfigPolicy || !toolModelConfig[selectedTool] || isLaunching)
                                                        ? 'bg-cyber-border text-cyber-text-secondary cursor-not-allowed'
                                                        : 'bg-cyber-accent text-black hover:bg-cyber-accent/90 hover:shadow-[0_0_15px_rgba(0,255,157,0.35)] shadow-[0_0_8px_rgba(0,255,157,0.15)]'
                                                        }`}
                                                >
                                                    {launchAfterApply ? t('btn.launchApp') : t('btn.modifyOnly')}
                                                </button>
                                                {/* 右侧：竖排复选框组 */}
                                                <div className="flex flex-col gap-2">
                                                    {/* Apply & Launch 复选框 */}
                                                    <label className="flex items-center gap-2 cursor-pointer select-none" onClick={() => setLaunchAfterApply(!launchAfterApply)}>
                                                        <div className={`w-3.5 h-3.5 border flex items-center justify-center transition-all flex-shrink-0 ${launchAfterApply ? 'border-cyber-accent bg-cyber-accent/20' : 'border-cyber-border hover:border-cyber-text-muted'
                                                            }`}>
                                                            {launchAfterApply && (
                                                                <svg width="8" height="8" viewBox="0 0 10 10" fill="none">
                                                                    <path d="M2 5L4 7L8 3" stroke="#00FF9D" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                                                                </svg>
                                                            )}
                                                        </div>
                                                        <span className={`text-[10px] font-mono transition-colors ${launchAfterApply ? 'text-cyber-accent' : 'text-cyber-text-secondary'}`}>
                                                            {t('agent.applyAndLaunch')}
                                                        </span>
                                                    </label>
                                                    {/* 配置合规声明 */}
                                                    <label className="flex items-center gap-2 cursor-pointer select-none" onClick={() => setAgreedConfigPolicy(!agreedConfigPolicy)}>
                                                        <div className={`w-3.5 h-3.5 border flex items-center justify-center transition-all flex-shrink-0 ${agreedConfigPolicy ? 'border-cyber-accent bg-cyber-accent/20' : 'border-cyber-border hover:border-cyber-text-muted'
                                                            }`}>
                                                            {agreedConfigPolicy && (
                                                                <svg width="8" height="8" viewBox="0 0 10 10" fill="none">
                                                                    <path d="M2 5L4 7L8 3" stroke="#00FF9D" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                                                                </svg>
                                                            )}
                                                        </div>
                                                        <span className={`text-[10px] font-mono transition-colors ${agreedConfigPolicy ? 'text-cyber-accent' : 'text-cyber-text-secondary'}`}>
                                                            {t('agent.appliedVia')}
                                                        </span>
                                                    </label>
                                                </div>
                                            </div>
                                        </div>
                                    )}
                                </div>

                                {/* Add Model Modal */}
                                {
                                    showAddModelModal && (
                                        <div
                                            className={`fixed inset-0 z-[9998] flex items-center justify-center transition-all duration-200 ${modelModalAnimatingOut ? 'opacity-0' : 'opacity-100'}`}
                                            onKeyDown={e => { if (e.key === 'Escape') closeModelModal(); }}
                                        >
                                            {/* 背景蒙层 */}
                                            <div
                                                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                                                onClick={closeModelModal}
                                            />

                                            <div
                                                className={`relative w-[450px] max-w-[90vw] border border-cyber-accent/30 bg-cyber-bg shadow-[0_0_30px_rgba(0,255,157,0.08)] rounded-xl overflow-hidden transition-all duration-200 ${modelModalAnimatingOut ? 'scale-95 opacity-0' : 'scale-100 opacity-100'}`}
                                                onClick={e => e.stopPropagation()}
                                            >
                                                {/* 顶部渐变线 */}
                                                <div className="h-[2px] w-full bg-gradient-to-r from-cyber-accent/60 via-cyber-accent-secondary/40 to-transparent" />

                                                {/* Header */}
                                                <div className="px-5 pt-4 pb-3 flex items-center justify-between">
                                                    <div className="flex items-center gap-2">
                                                        <span className="text-cyber-accent font-mono text-xs opacity-60">&gt;_</span>
                                                        <span className="text-sm font-mono font-bold tracking-wider text-cyber-accent">{editingModelId ? t('model.editConfig') : t('btn.addModel')}</span>
                                                    </div>
                                                    <button
                                                        onClick={closeModelModal}
                                                        className="text-cyber-text-secondary hover:text-cyber-text transition-colors"
                                                    >
                                                        <X size={14} />
                                                    </button>
                                                </div>

                                                {/* 表单内容 */}
                                                <div className="px-5 pb-5">
                                                    <div className="space-y-4">
                                                        <div>
                                                            <label className="block text-xs text-cyber-text-secondary mb-1">Name</label>
                                                            <input
                                                                type="text"
                                                                placeholder="e.g. OpenRouter Claude"
                                                                value={newModelForm.name}
                                                                onChange={e => setNewModelForm(prev => ({ ...prev, name: e.target.value }))}
                                                                className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                            />
                                                        </div>
                                                        <div>
                                                            <label className="block text-xs text-cyber-text-secondary mb-1">Base URL (OpenAI API)</label>
                                                            <input
                                                                type="text"
                                                                placeholder="https://x.x.com/v1 [NO→/chat/completions]"
                                                                value={newModelForm.baseUrl}
                                                                onChange={e => setNewModelForm(prev => ({ ...prev, baseUrl: e.target.value }))}
                                                                className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                            />
                                                        </div>
                                                        <div>
                                                            <label className="block text-xs text-cyber-text-secondary mb-1">Base URL (Anthropic API)</label>
                                                            <input
                                                                type="text"
                                                                placeholder="https://x.x.com/anthropic [NO→/v1/messages]"
                                                                value={newModelForm.anthropicUrl}
                                                                onChange={e => setNewModelForm(prev => ({ ...prev, anthropicUrl: e.target.value }))}
                                                                className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                            />
                                                        </div>
                                                        <div>
                                                            <label className="block text-xs text-cyber-text-secondary mb-1">Model ID</label>
                                                            <input
                                                                type="text"
                                                                placeholder="e.g. anthropic/claude-opus-4.5"
                                                                value={newModelForm.modelId}
                                                                onChange={e => setNewModelForm(prev => ({ ...prev, modelId: e.target.value }))}
                                                                className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                            />
                                                        </div>
                                                        <div>
                                                            <label className="block text-xs text-cyber-text-secondary mb-1">API Key</label>
                                                            <div className="relative">
                                                                <input
                                                                    type="text"
                                                                    placeholder="sk-..."
                                                                    value={newModelForm.apiKey.startsWith('enc:v1:') ? '••••••••••••••••' : newModelForm.apiKey}
                                                                    onChange={e => setNewModelForm(prev => ({ ...prev, apiKey: e.target.value }))}
                                                                    className="w-full bg-black border border-cyber-border px-2 py-1.5 pr-8 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                                    readOnly={newModelForm.apiKey.startsWith('enc:v1:')}
                                                                />
                                                                {/* 锁图标：切换加密状态 */}
                                                                {newModelForm.apiKey && newModelForm.apiKey !== 'local' && (
                                                                    <button
                                                                        type="button"
                                                                        disabled={!editingModelId}
                                                                        onClick={async () => {
                                                                            if (!editingModelId) return;
                                                                            const result = await window.electron?.toggleKeyEncryption(editingModelId);
                                                                            if (result?.success) {
                                                                                setNewModelForm(prev => ({ ...prev, apiKey: result.apiKey }));
                                                                                setUserModels(prev => prev.map(m =>
                                                                                    m.internalId === editingModelId ? { ...m, apiKey: result.apiKey } : m
                                                                                ));
                                                                            }
                                                                        }}
                                                                        className={`absolute right-2 top-1/2 -translate-y-1/2 transition-colors hover:opacity-80 ${!editingModelId ? 'opacity-40' : ''}`}
                                                                    >
                                                                        {newModelForm.apiKey.startsWith('enc:v1:') ? (
                                                                            /* 闭合锁（已加密） */
                                                                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-cyber-accent">
                                                                                <rect x="3" y="11" width="18" height="11" rx="2" />
                                                                                <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                                                                            </svg>
                                                                        ) : (
                                                                            /* 开放锁（未加密） */
                                                                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-cyber-text-muted">
                                                                                <rect x="3" y="11" width="18" height="11" rx="2" />
                                                                                <path d="M7 11V7a5 5 0 0 1 9.9-1" />
                                                                            </svg>
                                                                        )}
                                                                    </button>
                                                                )}
                                                            </div>
                                                            {/* 加密状态提示（固定预留 2 行高度，避免窗口抖动） */}
                                                            <div className="min-h-[36px] mt-1">
                                                                {newModelForm.apiKey.startsWith('enc:v1:') && (
                                                                    <div className={`text-xs leading-tight ${keyDestroyed ? 'text-red-400' : 'text-cyber-accent/60'}`}>
                                                                        {keyDestroyed ? t('key.destroyed') : t('key.encrypted')}
                                                                    </div>
                                                                )}
                                                            </div>
                                                        </div>

                                                        {/* SS proxy configuration area */}
                                                        <div className="border-t border-cyber-accent/20 pt-4 mt-4">
                                                            <label className="flex items-center gap-3 cursor-pointer group">
                                                                <div className="relative">
                                                                    <input
                                                                        type="checkbox"
                                                                        checked={newModelForm.useProxy}
                                                                        onChange={e => setNewModelForm(prev => ({ ...prev, useProxy: e.target.checked }))}
                                                                        className="sr-only peer"
                                                                    />
                                                                    <div className="w-5 h-5 border-2 border-cyber-accent/50 bg-black peer-checked:bg-cyber-accent peer-checked:border-cyber-accent transition-all flex items-center justify-center">
                                                                        {newModelForm.useProxy && <span className="text-black text-xs font-bold">✓</span>}
                                                                    </div>
                                                                </div>
                                                                <span className="text-sm text-cyber-text font-mono group-hover:text-cyber-accent transition-colors">{t('model.proxyTunnel')} <span className="text-cyber-text-secondary">({t('model.specificProxy')})</span></span>
                                                            </label>

                                                            {newModelForm.useProxy && (
                                                                <div className="grid grid-cols-2 gap-3 mt-3">
                                                                    <div className="col-span-2">
                                                                        <label className="block text-xs text-cyber-text-secondary mb-1">SS Server *</label>
                                                                        <input
                                                                            type="text"
                                                                            placeholder="sg1.expressvpn.com"
                                                                            value={newModelForm.ssServer}
                                                                            onChange={e => setNewModelForm(prev => ({ ...prev, ssServer: e.target.value }))}
                                                                            className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                                        />
                                                                    </div>
                                                                    <div>
                                                                        <label className="block text-xs text-cyber-text-secondary mb-1">Port *</label>
                                                                        <input
                                                                            type="number"
                                                                            placeholder="52324"
                                                                            value={newModelForm.ssPort}
                                                                            onChange={e => setNewModelForm(prev => ({ ...prev, ssPort: e.target.value }))}
                                                                            className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button no-spinner"
                                                                        />
                                                                    </div>
                                                                    <div>
                                                                        <label className="block text-xs text-cyber-text-secondary mb-1">Cipher</label>
                                                                        <MiniSelect
                                                                            value={newModelForm.ssCipher}
                                                                            onChange={value => setNewModelForm(prev => ({ ...prev, ssCipher: value }))}
                                                                            options={[
                                                                                { id: 'aes-128-gcm', label: 'aes-128-gcm' },
                                                                                { id: 'aes-256-gcm', label: 'aes-256-gcm' },
                                                                                { id: 'chacha20-ietf-poly1305', label: 'chacha20-ietf-poly1305' }
                                                                            ]}
                                                                            className="w-full"
                                                                        />
                                                                    </div>
                                                                    <div className="col-span-2">
                                                                        <label className="block text-xs text-cyber-text-secondary mb-1">Password *</label>
                                                                        <input
                                                                            type="password"
                                                                            placeholder="SS Password / UUID"
                                                                            value={newModelForm.ssPassword}
                                                                            onChange={e => setNewModelForm(prev => ({ ...prev, ssPassword: e.target.value }))}
                                                                            className="w-full bg-black border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-accent focus:outline-none rounded-button"
                                                                        />
                                                                    </div>
                                                                </div>
                                                            )}
                                                        </div>
                                                    </div>
                                                </div>

                                                {/* 底部按钮 */}
                                                <div className="flex border-t border-cyber-border">
                                                    <button
                                                        onClick={closeModelModal}
                                                        className="flex-1 px-4 py-2.5 text-xs font-mono font-bold tracking-wider text-cyber-text-secondary hover:text-cyber-text hover:bg-white/5 transition-all border-r border-cyber-border"
                                                    >
                                                        {t('model.escCancel')}
                                                    </button>
                                                    <button
                                                        onClick={async () => {
                                                            // Removed mandatory validation, users can fill as they wish
                                                            // If proxy enabled, validate SS config
                                                            if (newModelForm.useProxy) {
                                                                if (!newModelForm.ssServer || !newModelForm.ssPort || !newModelForm.ssPassword) {
                                                                    console.warn('[App] SS proxy config incomplete, saving anyway as per hacker style');
                                                                    // alert('Please fill in SS proxy configuration'); 
                                                                    // return; // Allow proceeding
                                                                }
                                                            }

                                                            if (window.electron?.addModel) {
                                                                let proxyUrl: string | undefined = undefined;
                                                                let ssNode: SSNodeConfig | undefined = undefined;

                                                                // If proxy enabled, add proxy route first
                                                                if (newModelForm.useProxy && window.electron.addSSProxyRoute) {
                                                                    ssNode = {
                                                                        name: newModelForm.name,
                                                                        server: newModelForm.ssServer,
                                                                        port: parseInt(newModelForm.ssPort),
                                                                        cipher: newModelForm.ssCipher,
                                                                        password: newModelForm.ssPassword
                                                                    };
                                                                    // Generate temp ID
                                                                    const tempId = editingModelId || `model_${Date.now()}`;
                                                                    const result = await window.electron.addSSProxyRoute(tempId, newModelForm.baseUrl, ssNode);
                                                                    if (result.success) {
                                                                        proxyUrl = result.proxyUrl;
                                                                    }
                                                                }

                                                                const modelData = {
                                                                    name: newModelForm.name,
                                                                    baseUrl: newModelForm.baseUrl,
                                                                    anthropicUrl: newModelForm.anthropicUrl || undefined,
                                                                    apiKey: newModelForm.apiKey,
                                                                    modelId: newModelForm.modelId,
                                                                    proxyUrl: proxyUrl,
                                                                    ssNode: ssNode
                                                                };

                                                                if (editingModelId && window.electron.updateModel) {
                                                                    // Edit mode - Update existing model
                                                                    const updatedModel = await window.electron.updateModel(editingModelId, modelData);
                                                                    if (updatedModel) {
                                                                        setUserModels(prev => prev.map(m => m.internalId === editingModelId ? updatedModel : m));
                                                                    }
                                                                } else {
                                                                    // Create mode - Add new model
                                                                    const newModel = await window.electron.addModel(modelData);
                                                                    setUserModels(prev => [...prev, newModel]);
                                                                }

                                                                // Reset form and state
                                                                setEditingModelId(null);
                                                                setNewModelForm({
                                                                    name: '',
                                                                    baseUrl: '',
                                                                    anthropicUrl: '',
                                                                    apiKey: '',
                                                                    modelId: '',
                                                                    useProxy: false,
                                                                    ssServer: '',
                                                                    ssPort: '',
                                                                    ssCipher: 'aes-128-gcm',
                                                                    ssPassword: ''
                                                                });
                                                                setShowAddModelModal(false);
                                                            }
                                                        }}
                                                        className="flex-1 px-4 py-2.5 text-xs font-mono font-bold tracking-wider text-cyber-accent hover:bg-cyber-accent/10 transition-all"
                                                    >
                                                        {t('model.enterSave')}
                                                    </button>
                                                </div>
                                            </div>
                                        </div>
                                    )
                                }
                            </div>
                            {/* 全局下载进度条 */}
                            <DownloadBar />
                            <SettingsDialog
                                isOpen={showSettings}
                                onClose={() => setShowSettings(false)}
                                locale={currentLocale}
                                onLocaleChange={setCurrentLocale}
                                showLogsPage={showLogsPage}
                                onShowLogsPageChange={setShowLogsPage}
                            />
                        </div>

                        {/* 密钥自毁/apply 失败错误弹窗 */}
                        {applyError && (
                            <div className="fixed inset-0 z-[9998] flex items-center justify-center">
                                <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={() => setApplyError(null)} />
                                <div className="relative w-[360px] max-w-[90vw] border border-red-500/40 bg-cyber-bg shadow-lg shadow-[0_0_20px_rgba(255,60,60,0.1)] rounded-xl overflow-hidden">
                                    <div className="h-[2px] w-full bg-red-500/60" />
                                    <div className="px-5 pt-4 pb-2 flex items-center gap-2">
                                        <svg className="w-4 h-4 text-red-400 flex-shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" /><line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" /></svg>
                                        <span className="text-sm font-mono font-bold tracking-wider text-red-400">API Key Warning</span>
                                    </div>
                                    <div className="px-5 pb-5">
                                        <p className="text-xs text-cyber-text-secondary leading-relaxed font-mono">{applyError}</p>
                                    </div>
                                    <div className="flex border-t border-cyber-border">
                                        <button
                                            onClick={() => setApplyError(null)}
                                            className="flex-1 px-4 py-2.5 text-xs font-mono font-bold tracking-wider text-red-400 hover:bg-red-500/10 hover:text-red-300 transition-all"
                                        >
                                            {t('common.confirm')}
                                        </button>
                                    </div>
                                </div>
                            </div>
                        )}
                    </GatewayProvider>
                </DownloadProvider>
            </ConfirmDialogProvider>
        </ToastProvider>
    );
}

export default App;
