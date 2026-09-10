import React, { useState, useEffect, useCallback, useRef } from 'react';
import { useConfirm } from '../../components/ConfirmDialog';
import { EFFORT_PULSE_ONESHOT_MS } from '../../components';
import { useI18n } from '../../hooks/useI18n';
import * as api from '../../api/tauri';
import type { ModelConfig } from '../../api/types';
import { AppManagerContext } from './context';
import { useToolsStore } from '../../stores/toolsStore';
import { useNavigationStore } from '../../stores/navigationStore';
import { getOfficialEndpoint, isOfficialModelSentinel } from '../../data/officialEndpoints';
import { open as openDialog } from '@tauri-apps/plugin-dialog';

// ===== Provider =====

// Only Xiaomi's MiMo model series gets the apply pulse + sound (per request);
// every other model applies silently. Same keys ModelCard uses for the Xiaomi
// icon — a model counts as MiMo when its name/modelId contains xiaomi / 小米 / mimo.
const MIMO_KEYS = ['xiaomi', '小米', 'mimo'];
const isMimoModel = (m?: ModelConfig): boolean => {
  if (!m) return false;
  const text = `${m.name} ${m.modelId || ''}`.toLowerCase();
  return MIMO_KEYS.some((k) => text.includes(k));
};

interface AppManagerProviderProps {
  children: React.ReactNode;
}

export const AppManagerProvider: React.FC<AppManagerProviderProps> = ({ children }) => {
  const { t, locale } = useI18n();
  const _confirm = useConfirm();

  // From stores (replaces drilled props)
  const {
    detectedTools,
    setDetectedTools,
    isScanning,
    scanTools,
    modelProtocolSelection,
    setModelProtocolSelection,
  } = useToolsStore();
  const { activePage, goToMother } = useNavigationStore();
  const isActive = activePage === 'apps';

  // Wrapped navigation: build prefill and go to Mother Agent (model check happens there)
  const handleGoToMother = useCallback(
    async (toolId: string, toolName: string) => {
      const prefill = t('mother.hintInstall').replace('{agent}', toolName);
      goToMother(prefill);
    },
    [t, goToMother]
  );

  // Load models internally. userModels is consumed by BOTH the AppManager
  // right panel AND the 我的AI项目 right panel (same ModelListSection
  // component), so reload on either activation — otherwise a model added
  // in 模型中心 only surfaces in 我的AI项目 after the user incidentally
  // bounces through 应用桌面 (which flips isActive). The extra trigger
  // is free in practice (IPC + a couple of file reads on local Rust).
  const [userModels, setUserModels] = useState<ModelConfig[]>([]);
  const userModelsActive = isActive || activePage === 'myProjects';
  useEffect(() => {
    if (!api.getModels) return;
    // Guard against out-of-order resolution: rapid 应用桌面/我的AI项目 toggles can
    // overlap getModels() calls, and a slower earlier one resolving last would
    // clobber userModels with stale data.
    let ignore = false;
    api
      .getModels()
      .then((models) => {
        if (!ignore) setUserModels(models);
      })
      .catch((e) => console.error('Load models failed:', e));
    return () => {
      ignore = true;
    };
  }, [userModelsActive]);

  // AI-installable IDs from bundled install/index.json (offline-first).
  const [aiInstallableIds, setAiInstallableIds] = useState<string[]>([]);
  useEffect(() => {
    if (!isActive) return;
    api
      .getInstallIndex()
      .then((s) => {
        try {
          const data = JSON.parse(s);
          if (Array.isArray(data?.ids)) setAiInstallableIds(data.ids);
        } catch {
          /* malformed — keep empty */
        }
      })
      .catch(() => {
        /* IPC error — keep empty */
      });
  }, [isActive]);

  // Internalized state
  const [selectedTool, setSelectedTool] = useState<string | null>(null);
  const [isLaunching, setIsLaunching] = useState(false);
  const [applyError, setApplyError] = useState<string | null>(null);

  // One-shot "applied!" pulse. When a model config takes effect (via
  // handleLaunch) the just-applied model's card plays the effort pulse once.
  // `nonce` bumps on every fire so re-applying the same model replays it; the
  // trigger auto-clears after the pulse's own duration so the card returns to rest.
  const [appliedPulse, setAppliedPulse] = useState<{ id: string; nonce: number } | null>(null);
  const pulseNonce = useRef(0);
  const pulseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const firePulse = useCallback((id: string) => {
    pulseNonce.current += 1;
    setAppliedPulse({ id, nonce: pulseNonce.current });
    if (pulseTimer.current) clearTimeout(pulseTimer.current);
    pulseTimer.current = setTimeout(() => setAppliedPulse(null), EFFORT_PULSE_ONESHOT_MS);
  }, []);
  useEffect(
    () => () => {
      if (pulseTimer.current) clearTimeout(pulseTimer.current);
    },
    []
  );
  // A pulse belongs to the tool it was applied to. `appliedPulse`/`userModels`
  // are global and the card gate matches only on model id, so switching tools
  // would otherwise replay the pulse on a different tool's card that happens to
  // list the same model — clear it whenever the selected tool changes. Adjust
  // state during render (React's "reset on prop change" pattern) rather than in
  // an effect, so it applies before paint without a setState-in-effect cascade.
  const [pulseTool, setPulseTool] = useState<string | null>(selectedTool);
  if (pulseTool !== selectedTool) {
    setPulseTool(selectedTool);
    setAppliedPulse(null);
  }

  // Bottom-bar checkbox states are persisted across sessions — users get tired of
  // re-checking the same boxes every launch. Default both to true so picking an app
  // and clicking the big button "just launches it"; users who only want to rewrite
  // config without launching can uncheck the launch box.
  const readBool = (key: string, fallback: boolean): boolean => {
    try {
      const v = localStorage.getItem(key);
      return v === null ? fallback : v === 'true';
    } catch {
      return fallback;
    }
  };
  const writeBool = (key: string, v: boolean) => {
    try {
      localStorage.setItem(key, String(v));
    } catch {
      /* private mode */
    }
  };
  const [launchAfterApply, setLaunchAfterApplyRaw] = useState<boolean>(() =>
    readBool('echobird_appmgr_launch_after', true)
  );
  const setLaunchAfterApply = (v: boolean) => {
    setLaunchAfterApplyRaw(v);
    writeBool('echobird_appmgr_launch_after', v);
  };
  const [agreedConfigPolicy, setAgreedConfigPolicyRaw] = useState<boolean>(() =>
    readBool('echobird_appmgr_apply_config', true)
  );
  const setAgreedConfigPolicy = (v: boolean) => {
    setAgreedConfigPolicyRaw(v);
    writeBool('echobird_appmgr_apply_config', v);
  };
  // Claude Desktop's own routing toggle. When ON, apply_claudedesktop writes
  // the real upstream URL + api key into the Desktop profile JSON so
  // Desktop's gateway talks straight to the relay station. Default OFF:
  // Desktop talks to our anthropic_proxy which does model-id rewrite
  // and protocol translation.
  const [claudeDesktopRelayMode, setClaudeDesktopRelayModeRaw] = useState<boolean>(() =>
    readBool('echobird_claudedesktop_relay_mode', false)
  );
  // Claude Code routing toggle. When ON, apply_claudecode writes the real
  // upstream URL + key + model id straight into ~/.claude/settings.json so
  // Claude Code talks to the relay station directly. Default OFF: settings.json
  // points ANTHROPIC_BASE_URL at our anthropic_proxy (/claudecode route, no
  // model id), which rewrites whatever claude-* id Claude Code sends to the
  // real upstream model — the same first-class path as Claude Desktop. Kept
  // separate from claudeDesktopRelayMode (own backend relay file) so a user
  // can run Claude Code and Claude Desktop on different providers.
  const [claudeCodeRelayMode, setClaudeCodeRelayModeRaw] = useState<boolean>(() =>
    readBool('echobird_claudecode_relay_mode', false)
  );
  // Codex-only "Responses passthrough" toggle. Default OFF (legacy Bridge
  // translation). When ON, config.toml still points at the 127.0.0.1 proxy (so
  // model-id rewrite keeps happening), but the proxy forwards to the upstream's
  // native /responses endpoint verbatim instead of translating down to Chat.
  // For third-party models that natively speak Responses. Shared across Codex
  // CLI + ChatGPT desktop (both read ~/.codex/config.toml).
  const [codexResponsesPassthrough, setCodexResponsesPassthroughRaw] = useState<boolean>(() =>
    readBool('echobird_codex_responses_passthrough', false)
  );
  // Codex-only web-search toggle. Default ON → write web_search="live"
  // (real-time retrieval); OFF writes web_search="disabled" so Codex removes
  // its built-in search tool. NOT Codex's default "cached" — that's an
  // OpenAI-maintained index with no external web access, useless for our
  // third-party upstreams.
  const [codexWebSearch, setCodexWebSearchRaw] = useState<boolean>(() =>
    readBool('echobird_codex_web_search', true)
  );
  // Claude Code relay-only 1M-context toggle. When on AND API Router is on,
  // apply_claudecode appends `[1m]` to the model id (MODEL / OPUS / SONNET / FABLE env
  // vars only — HAIKU + SUBAGENT stay bare) so Claude Code budgets the 1M
  // window. CC strips the suffix before sending upstream, so the provider
  // still sees the bare id. No effect in bridge mode — bridge writes no model
  // id (CC uses built-in claude-* ids, already full-window). Default off: only
  // opt in when the relay's upstream is a real Claude Sonnet 5 / Opus 4.8-class
  // model that supports 1M. Persisted under a NEW localStorage key
  // (echobird_claudecode_1m_mode) — deliberately not reusing the v5.3.8
  // echobird_claude_1m_mode key, which shared semantics with Claude Desktop.
  const [claude1mMode, setClaude1mModeRaw] = useState<boolean>(() =>
    readBool('echobird_claudecode_1m_mode', false)
  );
  const [viewMode, setViewModeRaw] = useState<'desktop' | 'install'>('desktop');
  const setViewMode = (mode: 'desktop' | 'install') => {
    if (mode === viewMode) return;
    setViewModeRaw(mode);
    setSelectedTool(null);
    setApplyError(null);
  };
  // Tool model config (single selection - one model per tool)
  const [toolModelConfig, setToolModelConfig] = useState<Record<string, string | null>>({
    claudecode: null,
    openclaw: null,
    opencode: null,
    codex: null,
    hermes: null,
    zcode: null,
  });

  // Set tool model (single selection) - UI state update
  const handleSelectModel = (toolId: string, modelId: string) => {
    setToolModelConfig((prev) => ({
      ...prev,
      [toolId]: modelId,
    }));
  };

  // Get selected tool data
  const selectedToolData = detectedTools.find(
    (t) => t.id === selectedTool && (!isActive || t.installed === (viewMode === 'desktop'))
  );

  // Apply model config to backend (internalized from App.tsx).
  // `relayOverride` lets callers (most importantly setClaudeDesktopRelayMode)
  // bypass the captured claudeDesktopRelayMode value when re-applying after
  // a toggle flip — React would otherwise stale-close on the old value.
  const applyModelConfig = async (
    toolId: string,
    internalId: string,
    relayOverride?: boolean,
    passthroughOverride?: boolean,
    webSearchOverride?: boolean,
    oneMOverride?: boolean
  ): Promise<true | string | false> => {
    const model = userModels.find((m) => m.internalId === internalId);
    if (!model) {
      console.error('Model not found:', internalId);
      return false;
    }

    const toolData = detectedTools.find((t) => t.id === toolId);
    const toolProtocols = toolData?.apiProtocol || ['openai'];

    const userSelectedProtocol = modelProtocolSelection[internalId];
    // Default to the protocol the model's URL actually speaks — NOT toolProtocols[0].
    // For a single-URL model (no ⇄ switcher) defaulting to toolProtocols[0] would
    // write an OpenAI-only model as @ai-sdk/anthropic against an OpenAI URL (or an
    // Anthropic-only model as openai-compatible), 404-ing every call at runtime.
    // Only a both-URL model falls back to toolProtocols[0] — its switcher lets the
    // user pick. (Improves zcode too: an anthropicUrl-only model now defaults correctly.)
    const modelHasBoth = !!(model.baseUrl && model.anthropicUrl);
    const defaultProtocol = modelHasBoth
      ? toolProtocols[0] === 'anthropic'
        ? 'anthropic'
        : 'openai'
      : model.anthropicUrl
        ? 'anthropic'
        : 'openai';
    const selectedProtocol = userSelectedProtocol || defaultProtocol;

    const useAnthropicUrl = selectedProtocol === 'anthropic' && model.anthropicUrl;
    const apiUrl = useAnthropicUrl ? model.anthropicUrl! : model.baseUrl;

    // eslint-disable-next-line no-console
    console.debug(
      `[AppManager] Applying model to ${toolId}: protocol=${selectedProtocol}, url=${apiUrl}`
    );

    // Codex apps + Claude Desktop honor the relay-mode toggle from the
    // right panel. Other tools ignore the field — apply_codex and
    // apply_claudedesktop are the only consumers and short-circuit on
    // tool_id mismatch.
    const isCodexApp = toolId === 'codex' || toolId === 'chatgptdesktop';
    const isClaudeDesktopApp = toolId === 'claudedesktop';
    const isClaudeCodeApp = toolId === 'claudecode';
    const isClaudeApp = isClaudeDesktopApp || isClaudeCodeApp;
    // Codex no longer exposes API Router (it has Web Search instead); relay is
    // Claude-only now. Claude Code rides the same model-id-rewrite proxy as
    // Claude Desktop (relay ON = direct, OFF = our proxy/bridge), with its own
    // claudeCodeRelayMode flag + backend relay file.
    const isRelayCapableApp = isClaudeApp;
    const currentRelayMode = isClaudeDesktopApp ? claudeDesktopRelayMode : claudeCodeRelayMode;
    const effectiveRelay = isClaudeApp ? (relayOverride ?? currentRelayMode) : false;
    const effectivePassthrough = isCodexApp && (passthroughOverride ?? codexResponsesPassthrough);
    const effectiveWebSearch = isCodexApp ? (webSearchOverride ?? codexWebSearch) : false;
    // 1M context — Claude Code relay-only. Guard on effectiveRelay so the
    // flag is never sent for bridge applies (bridge writes no model id, so
    // [1m] would be moot anyway — keeps the field semantically relay-only).
    const effective1m = isClaudeCodeApp && effectiveRelay && (oneMOverride ?? claude1mMode);

    try {
      const result = await api.applyModelToTool(toolId, {
        id: model.internalId,
        name: model.name,
        baseUrl: apiUrl,
        apiKey: model.apiKey,
        model: model.modelId || '',
        protocol: selectedProtocol,
        ...(isRelayCapableApp ? { relayMode: effectiveRelay } : {}),
        ...(isCodexApp
          ? { responsesPassthrough: effectivePassthrough, webSearch: effectiveWebSearch }
          : {}),
        ...(isClaudeCodeApp ? { oneMContext: effective1m } : {}),
      });

      if (result?.success) {
        setDetectedTools((prev) =>
          prev.map((t) =>
            t.id === toolId ? { ...t, activeModel: model.modelId || model.internalId } : t
          )
        );
        return true;
      } else {
        console.error('[AppManager] Failed to apply model:', result?.message);
        return result?.message || false;
      }
    } catch (error) {
      console.error('[AppManager] Error applying model to tool:', error);
      return false;
    }
  };

  // Responses-passthrough setter — mirrors setCodexWebSearch (shared across
  // Codex CLI + Desktop, re-applies on flip so the effect is immediate).
  const setCodexResponsesPassthrough = useCallback(
    (v: boolean) => {
      setCodexResponsesPassthroughRaw(v);
      writeBool('echobird_codex_responses_passthrough', v);
      const codexToolId = (['codex', 'chatgptdesktop'] as const).find(
        (id) => !!toolModelConfig[id]
      );
      if (!codexToolId) return;
      const pendingInternalId = toolModelConfig[codexToolId];
      if (!pendingInternalId || isOfficialModelSentinel(pendingInternalId)) return;
      void applyModelConfig(codexToolId, pendingInternalId, undefined, v).then((result) => {
        if (result !== true) {
          setApplyError(typeof result === 'string' ? result : t('key.destroyed'));
        }
      });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toolModelConfig, t, userModels]
  );

  // Web-search setter — mirrors setCodexResponsesPassthrough: persist + re-apply
  // the active Codex model so the change lands immediately.
  const setCodexWebSearch = useCallback(
    (v: boolean) => {
      setCodexWebSearchRaw(v);
      writeBool('echobird_codex_web_search', v);
      const codexToolId = (['codex', 'chatgptdesktop'] as const).find(
        (id) => !!toolModelConfig[id]
      );
      if (!codexToolId) return;
      const pendingInternalId = toolModelConfig[codexToolId];
      if (!pendingInternalId || isOfficialModelSentinel(pendingInternalId)) return;
      void applyModelConfig(codexToolId, pendingInternalId, undefined, undefined, v).then(
        (result) => {
          if (result !== true) {
            setApplyError(typeof result === 'string' ? result : t('key.destroyed'));
          }
        }
      );
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toolModelConfig, t, userModels]
  );

  // Claude Desktop relay-mode setter — mirrors the Codex toggle setters but
  // scoped to the claudedesktop tool. Re-applies on toggle flip so the
  // user sees an immediate effect (profile JSON gets rewritten with the
  // new gateway URL + key on the next /v1/messages request, no Desktop
  // restart required after the first 3p activation).
  const setClaudeDesktopRelayMode = useCallback(
    (v: boolean) => {
      setClaudeDesktopRelayModeRaw(v);
      writeBool('echobird_claudedesktop_relay_mode', v);
      const pendingInternalId = toolModelConfig['claudedesktop'];
      if (!pendingInternalId || isOfficialModelSentinel(pendingInternalId)) return;
      void applyModelConfig('claudedesktop', pendingInternalId, v).then((result) => {
        if (result !== true) {
          setApplyError(typeof result === 'string' ? result : t('key.destroyed'));
        }
      });
    },
    // (applyModelConfig stays excluded — it's recreated every render.)
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toolModelConfig, t, userModels]
  );

  // Claude Code relay-mode setter — mirrors setClaudeDesktopRelayMode but
  // scoped to the claudecode tool (own localStorage key + own backend relay
  // file). Re-applies on flip so settings.json is rewritten immediately.
  const setClaudeCodeRelayMode = useCallback(
    (v: boolean) => {
      setClaudeCodeRelayModeRaw(v);
      writeBool('echobird_claudecode_relay_mode', v);
      const pendingInternalId = toolModelConfig['claudecode'];
      if (!pendingInternalId || isOfficialModelSentinel(pendingInternalId)) return;
      void applyModelConfig('claudecode', pendingInternalId, v).then((result) => {
        if (result !== true) {
          setApplyError(typeof result === 'string' ? result : t('key.destroyed'));
        }
      });
    },
    // claude1mMode is a real dep: re-applying claudecode after a relay flip
    // reads it through applyModelConfig (oneMOverride defaults to it). Without
    // it here, flipping API Router after the 1M toggle re-writes settings.json
    // with a STALE 1M flag. (applyModelConfig stays excluded — recreated every
    // render.)
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toolModelConfig, t, userModels, claude1mMode]
  );

  // Claude Code 1M-context toggle (relay-only). Re-applies on flip so the
  // [1m] suffix lands/strips immediately. Only Claude Code is touched — the
  // toggle is hidden unless claudeCodeRelayMode is on, and bridge mode writes
  // no model id so the re-apply is a harmless no-op for [1m] there.
  const setClaude1mMode = useCallback(
    (v: boolean) => {
      setClaude1mModeRaw(v);
      writeBool('echobird_claudecode_1m_mode', v);
      const pendingInternalId = toolModelConfig['claudecode'];
      if (!pendingInternalId || isOfficialModelSentinel(pendingInternalId)) return;
      // oneMOverride is the 6th positional arg; relay/passthrough/webSearch
      // pass undefined so each resolves to its current state.
      void applyModelConfig(
        'claudecode',
        pendingInternalId,
        undefined,
        undefined,
        undefined,
        v
      ).then((result) => {
        if (result !== true) {
          setApplyError(typeof result === 'string' ? result : t('key.destroyed'));
        }
      });
    },
    // claudeCodeRelayMode is a real dep: the re-apply reads it through
    // applyModelConfig (relayOverride=undefined here). Omitting it would
    // re-write settings.json with a STALE relay flag, silently reverting
    // the user's API Router setting.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toolModelConfig, t, userModels, claudeCodeRelayMode]
  );

  // Restore = delete the tool's config file. The tool itself regenerates
  // a vendor-default config on next launch, so restore is symmetric with
  // a fresh install. Backend also clears the ~/.echobird/{tool}.json relay
  // for "custom" tools.
  const applyRestore = async (toolId: string): Promise<true | string | false> => {
    try {
      const result = await api.restoreToolToOfficial(toolId);
      if (result?.success) {
        const official = getOfficialEndpoint(toolId);
        setDetectedTools((prev) =>
          prev.map((t) => (t.id === toolId ? { ...t, activeModel: official?.name || '' } : t))
        );
        return true;
      }
      return result?.message || false;
    } catch (err) {
      console.error('[AppManager] Restore-to-official failed:', err);
      return String(err);
    }
  };

  // Direct restore — kept exported on context for any callers that want to
  // bypass the bottom-bar flow. The card click now selects (no immediate
  // apply); the actual restore runs from handleLaunch when the official
  // sentinel is the pending selection.
  const handleRestoreModel = async (toolId: string) => {
    const result = await applyRestore(toolId);
    if (result !== true) {
      setApplyError(typeof result === 'string' ? result : t('key.destroyed'));
    }
  };

  // Launch handler
  const handleLaunch = async () => {
    if (!selectedTool || isLaunching) return;
    setIsLaunching(true);
    setTimeout(() => setIsLaunching(false), 3000); // 3 second cooldown

    const toolData = detectedTools.find((t) => t.id === selectedTool);
    const isLaunchable = !!toolData?.launchFile;
    const noModelConfig = !!toolData?.noModelConfig;

    // Write model config to file only when the "apply via official config" checkbox is on.
    // Launchable tools (e.g. games) always pass config via URL hash, never via file write.
    // no-model-config tools (e.g. desktop apps) skip config writes entirely.
    if (!noModelConfig && agreedConfigPolicy && !isLaunchable && toolModelConfig[selectedTool]) {
      const pending = toolModelConfig[selectedTool]!;
      const applyResult = isOfficialModelSentinel(pending)
        ? await applyRestore(selectedTool)
        : await applyModelConfig(selectedTool, pending);
      if (applyResult !== true) {
        setApplyError(typeof applyResult === 'string' ? applyResult : t('key.destroyed'));
        setIsLaunching(false);
        return;
      }
      // Only Xiaomi's MiMo models get the apply pulse + sound; every other model
      // (and restore-to-official) applies silently.
      const appliedModel = userModels.find((m) => m.internalId === pending);
      if (readBool('echobird_easter_egg', true) && isMimoModel(appliedModel)) firePulse(pending);
    }
    // Launch tool when "launch directly" is checked, or unconditionally for desktop apps
    if (launchAfterApply || noModelConfig) {
      if (isLaunchable) {
        // Launchable tool (e.g. game): open independent window with model config
        const selectedModelId = toolModelConfig[selectedTool];
        const selectedModel = selectedModelId
          ? userModels.find((m) => m.internalId === selectedModelId)
          : undefined;
        const modelConfig = selectedModel
          ? {
              baseUrl: selectedModel.baseUrl,
              anthropicUrl: selectedModel.anthropicUrl,
              apiKey: selectedModel.apiKey,
              model: selectedModel.modelId || selectedModel.name || 'unknown',
              name: selectedModel.name,
              protocol: modelProtocolSelection[selectedModel.internalId] || 'openai',
              locale,
            }
          : { locale };
        const result = await api.launchGame(selectedTool, toolData!.launchFile!, modelConfig);
        if (result && !result.success) {
          console.error('Failed to launch:', result.message);
          if (result.message) setApplyError(result.message);
        } else if (selectedModel) {
          // Mirror the apply-path optimistic update (see line ~209). launchable
          // tools (games / WebView utilities) inject the model via
          // window.__MODEL_CONFIG__ instead of writing a config file, so the
          // apply path is skipped — without this, the tool card would forever
          // show "模型: -" even after a successful launch.
          setDetectedTools((prev) =>
            prev.map((t) =>
              t.id === selectedTool
                ? { ...t, activeModel: selectedModel.modelId || selectedModel.internalId }
                : t
            )
          );
          if (readBool('echobird_easter_egg', true) && isMimoModel(selectedModel))
            firePulse(selectedModel.internalId);
        }
      } else {
        // Only "CLI Code" tools prompt for a working folder — their launch goes
        // through start_cli_tool, which honors cwd. Desktop / IDE / Utility /
        // Game tools launch via start_gui_tool / launch_vscode / launchGame,
        // none of which consume cwd, so prompting would silently discard the
        // user's pick (Coffee-CLI-style launchpad, minus persistence — we
        // re-prompt every launch). If the user cancels the picker, abort the
        // launch entirely — don't fall back to home, which is the bug this
        // fixes (CLI tools used to spawn in ~/ and were useless for development).
        // A CLI tool may opt out per-tool via paths.json `noCwdPrompt: true`
        // (e.g. openclaw) — it launches directly with no prompt, spawning in
        // the home dir, which is fine for self-contained CLI agents.
        let cwd: string | undefined;
        if (toolData?.category === 'CLI Code' && !toolData?.noCwdPrompt) {
          try {
            const picked = await openDialog({ directory: true, multiple: false });
            if (typeof picked !== 'string' || !picked) {
              setIsLaunching(false);
              return;
            }
            cwd = picked;
          } catch (e) {
            console.error('[AppManager] folder picker failed:', e);
            setApplyError(e instanceof Error ? e.message : String(e));
            setIsLaunching(false);
            return;
          }
        }
        try {
          await api.startTool(selectedTool, toolData?.startCommand, cwd);
        } catch (err) {
          console.error('Failed to launch tool:', err);
          setApplyError(err instanceof Error ? err.message : String(err));
        }
      }
    }
  };

  return (
    <AppManagerContext.Provider
      value={{
        selectedTool,
        setSelectedTool,
        launchAfterApply,
        setLaunchAfterApply,
        isLaunching,
        agreedConfigPolicy,
        setAgreedConfigPolicy,
        toolModelConfig,
        handleSelectModel,
        handleRestoreModel,
        selectedToolData,
        applyError,
        setApplyError,
        detectedTools,
        setDetectedTools,
        isScanning,
        scanTools,
        userModels,
        modelProtocolSelection,
        setModelProtocolSelection,
        codexResponsesPassthrough,
        setCodexResponsesPassthrough,
        codexWebSearch,
        setCodexWebSearch,
        claudeDesktopRelayMode,
        setClaudeDesktopRelayMode,
        claudeCodeRelayMode,
        setClaudeCodeRelayMode,
        claude1mMode,
        setClaude1mMode,
        appliedPulse,
        handleLaunch,
        onGoToMother: handleGoToMother,
        aiInstallableIds,
        viewMode,
        setViewMode,
      }}
    >
      {children}
    </AppManagerContext.Provider>
  );
};
