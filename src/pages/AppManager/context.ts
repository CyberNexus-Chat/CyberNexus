import { createContext, useContext } from 'react';
import type { ModelConfig, LocalTool } from '../../api/types';

// ===== Context =====

export interface AppManagerContextType {
  // Internalized state
  selectedTool: string | null;
  setSelectedTool: (id: string | null) => void;
  launchAfterApply: boolean;
  setLaunchAfterApply: (v: boolean) => void;
  isLaunching: boolean;
  agreedConfigPolicy: boolean;
  setAgreedConfigPolicy: (v: boolean) => void;
  toolModelConfig: Record<string, string | null>;
  handleSelectModel: (toolId: string, modelId: string) => void;
  /** Restore the tool's config back to its official vendor endpoint */
  handleRestoreModel: (toolId: string) => Promise<void>;
  selectedToolData: LocalTool | undefined;
  applyError: string | null;
  setApplyError: (v: string | null) => void;
  // Shared props (from App.tsx)
  detectedTools: LocalTool[];
  setDetectedTools: React.Dispatch<React.SetStateAction<LocalTool[]>>;
  isScanning: boolean;
  scanTools: () => Promise<void>;
  userModels: ModelConfig[];
  modelProtocolSelection: Record<string, 'openai' | 'anthropic'>;
  setModelProtocolSelection: React.Dispatch<
    React.SetStateAction<Record<string, 'openai' | 'anthropic'>>
  >;
  /** Codex-only "Responses passthrough" toggle. */
  codexResponsesPassthrough: boolean;
  setCodexResponsesPassthrough: (v: boolean) => void;
  /** Codex-only web-search toggle. OFF ⇒ apply writes web_search="disabled"
   *  so Codex removes its built-in search tool; default ON ⇒ web_search="live"
   *  (real-time retrieval). NOT Codex's "cached" — OpenAI index, no external
   *  web access, useless for our third-party upstreams. */
  codexWebSearch: boolean;
  setCodexWebSearch: (v: boolean) => void;
  /** Claude Desktop routing toggle. Kept separate from Codex because the
   *  two apps target different protocols / different relay-station compat. */
  claudeDesktopRelayMode: boolean;
  setClaudeDesktopRelayMode: (v: boolean) => void;
  /** Claude Code routing toggle. Separate flag from Claude Desktop so the two
   *  Claude apps can point at different upstreams independently (each has its
   *  own proxy route + relay file on the backend). */
  claudeCodeRelayMode: boolean;
  setClaudeCodeRelayMode: (v: boolean) => void;
  /** Claude Code relay-only 1M-context toggle. When on AND API Router is on,
   *  apply_claudecode appends `[1m]` to the model id (MODEL / OPUS / SONNET / FABLE
   *  env vars only — HAIKU + SUBAGENT stay bare) so Claude Code budgets the
   *  1M window. CC strips the suffix before sending upstream, so the provider
   *  still sees the bare id. Hidden when API Router is off; no effect in bridge
   *  mode. */
  claude1mMode: boolean;
  setClaude1mMode: (v: boolean) => void;
  /** One-shot pulse trigger. Set to the just-applied model's internalId (or the
   *  official sentinel) with a bumped nonce the instant a config takes effect,
   *  so that model's card plays the apply-confirmation pulse once. Null at rest. */
  appliedPulse: { id: string; nonce: number } | null;
  // Launch handler
  handleLaunch: () => Promise<void>;
  // Navigation — internal handler: (toolId, toolName) => fetch install info → call prop
  onGoToMother: (toolId: string, toolName: string) => void;
  // AI-installable tool IDs (from bundled tools/install/index.json)
  aiInstallableIds: string[];
  viewMode: 'desktop' | 'install';
  setViewMode: (mode: 'desktop' | 'install') => void;
}

export const AppManagerContext = createContext<AppManagerContextType | null>(null);

export const useAppManager = () => {
  const ctx = useContext(AppManagerContext);
  if (!ctx) throw new Error('useAppManager must be used within AppManagerProvider');
  return ctx;
};
