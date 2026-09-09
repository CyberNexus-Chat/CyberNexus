// Model Nexus Page — Model cards, debug console, add/edit modal
// Extracted from App.tsx with Provider pattern for shared state

import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { readText as readClipboardText } from '@tauri-apps/plugin-clipboard-manager';
import {
  DndContext,
  PointerSensor,
  KeyboardSensor,
  closestCenter,
  DragOverlay,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  useSortable,
  rectSortingStrategy,
  sortableKeyboardCoordinates,
  arrayMove,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { X, Box, ExternalLink, Plus, Lock, Unlock, RefreshCw, GripVertical } from 'lucide-react';
import { ModelCard, ModelCardSkeleton, getModelIcon, ModelIdCombobox } from '../../components';
import { useToast } from '../../components/Toast';
import { useI18n } from '../../hooks/useI18n';
import * as api from '../../api/tauri';
import type { ModelConfig } from '../../api/types';
import { normalizeAnthropicUrl, normalizeOpenaiUrl } from '../../utils/normalizeUrl';
import { ModelNexusContext, useModelNexus } from './context';
import type { NewModelForm } from './context';
import type { ModelUsageData } from '../../api/tauri';
import modelDirectory from '../../data/modelDirectory.json';
import { useFreeModels } from '../FreeModels/FreeModels';

// ===== Provider =====

/** Whether a model's endpoint is Volcengine (cn) - needs AK/SK for usage.
 *  Mirrors the backend can_handle + URL-selection (baseUrl if non-empty, else anthropicUrl). */
const isVolcengineUrl = (baseUrl: string, anthropicUrl?: string | null) => {
  const url = (baseUrl || anthropicUrl || '').toLowerCase();
  return url.includes('ark.cn-beijing') || url.includes('volcengine') || url.includes('volces.com');
};

const visibleModelNexusModels = (models: ModelConfig[]) =>
  models.filter(
    (model) => model.internalId !== 'local-server' && model.internalId !== 'smart-router'
  );

const isValidModelBaseUrl = (value: string) => {
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:') && Boolean(url.host);
  } catch {
    return false;
  }
};

export function ModelNexusProvider({ children }: { children: React.ReactNode }) {
  // Models state
  const [userModels, setUserModels] = useState<ModelConfig[]>([]);
  const [isLoadingModels, setIsLoadingModels] = useState(true);
  const [selectedModel, setSelectedModel] = useState<string | null>('gpt4o');
  const [pingingModelIds, setPingingModelIds] = useState<Set<string>>(new Set());

  // View mode state
  const [viewMode, setViewMode] = useState<'config' | 'usage'>('config');
  const [modelUsageData, setModelUsageData] = useState<Record<string, ModelUsageData>>({});
  const [isRefreshingUsage, setIsRefreshingUsage] = useState(false);
  const [refreshingUsageIds, setRefreshingUsageIds] = useState<Set<string>>(new Set());
  // Volcengine AK/SK (per-model: one account per model)
  const { showToast } = useToast();
  const [volcAkSkMissingIds, setVolcAkSkMissingIds] = useState<Set<string>>(new Set());
  const [volcAkSkModelId, setVolcAkSkModelId] = useState<string | null>(null);

  // Modal state
  const [showAddModelModal, setShowAddModelModal] = useState(false);
  const [modelModalDestination, setModelModalDestination] = useState<'modelNexus' | 'freeRouter'>(
    'modelNexus'
  );
  const [modelModalAnimatingOut, setModelModalAnimatingOut] = useState(false);
  const [editingModelId, setEditingModelId] = useState<string | null>(null);
  const [showApiKey, setShowApiKey] = useState(false);
  const [keyDestroyed, setKeyDestroyed] = useState(false);
  const [newModelForm, setNewModelForm] = useState<NewModelForm>({
    name: '',
    baseUrl: '',
    anthropicUrl: '',
    apiKey: '',
    modelId: '',
  });

  const closeModelModal = useCallback(() => {
    setModelModalAnimatingOut(true);
    setTimeout(() => {
      setModelModalAnimatingOut(false);
      setShowAddModelModal(false);
      setEditingModelId(null);
      setModelModalDestination('modelNexus');
    }, 200);
  }, []);

  // Test state
  const [testInput, setTestInput] = useState('');
  const [testOutput, setTestOutput] = useState<string[]>([]);
  const [isTesting, setIsTesting] = useState(false);
  const [arrowIndex, setArrowIndex] = useState(0);
  const [modelLatencies, setModelLatencies] = useState<Record<string, number>>({});
  const [modelTerminals, setModelTerminals] = useState<
    Record<string, { input: string; output: string[] }>
  >({});
  const [testProtocol, setTestProtocol] = useState<'openai' | 'anthropic'>('openai');
  const testInputRef = useRef<HTMLInputElement>(null!);
  const [inputFocused, setInputFocused] = useState(false);
  const [cursorPos, setCursorPos] = useState(0);

  // Derived
  const selectedModelData = userModels.find((m) => m.internalId === selectedModel);

  // Load models from config
  useEffect(() => {
    const loadModels = async () => {
      setIsLoadingModels(true);
      if (api.getModels) {
        try {
          const models = await api.getModels();
          setUserModels(visibleModelNexusModels(models));
        } catch (error) {
          console.error('Load models failed:', error);
        }
      }
      setIsLoadingModels(false);
    };
    loadModels();
  }, []);

  // Auto-fill Model ID and API Key for local models
  useEffect(() => {
    const isLocal = (url: string) => url.includes('localhost') || url.includes('127.0.0.1');
    const hasLocalUrl = isLocal(newModelForm.baseUrl) || isLocal(newModelForm.anthropicUrl);

    if (hasLocalUrl) {
      setNewModelForm((prev) => {
        const updates: { modelId?: string; apiKey?: string } = {};
        if (!prev.modelId) updates.modelId = 'local-model';
        if (!prev.apiKey) updates.apiKey = 'not-needed';
        return Object.keys(updates).length > 0 ? { ...prev, ...updates } : prev;
      });
    }
  }, [newModelForm.baseUrl, newModelForm.anthropicUrl]);

  // Marquee animation
  useEffect(() => {
    if (!isTesting) return;
    const timer = setInterval(() => {
      setArrowIndex((prev) => (prev + 1) % 4);
    }, 200);
    return () => clearInterval(timer);
  }, [isTesting]);

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
    testInputRef.current?.focus();
    // Restore terminal history only when the selected model changes; reading the
    // latest modelTerminals here is intentional (no re-run on map updates).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedModel]);

  // Listen for protocol change - focus input
  useEffect(() => {
    testInputRef.current?.focus();
  }, [testProtocol]);

  // ping --all
  const pingAllModels = async () => {
    if (isTesting) return;
    setIsTesting(true);
    const allModels = userModels;
    setPingingModelIds(new Set(allModels.map((m) => m.internalId)));
    for (const model of allModels) {
      try {
        const result = await api.pingModel(model.internalId);
        setPingingModelIds((prev) => {
          const next = new Set(prev);
          next.delete(model.internalId);
          return next;
        });
        // -1 is the agreed-upon "tested and failed" sentinel that ModelCard
        // turns into a red "Error" label. Leaving latency unset would
        // collapse the failure back into "未测试" (never tested), which
        // misleads the user who just watched the ping run.
        setModelLatencies((prev) => ({
          ...prev,
          [model.internalId]: result?.success ? result.latency : -1,
        }));
      } catch {
        setPingingModelIds((prev) => {
          const next = new Set(prev);
          next.delete(model.internalId);
          return next;
        });
        setModelLatencies((prev) => ({ ...prev, [model.internalId]: -1 }));
      }
    }
    setPingingModelIds(new Set());
    setIsTesting(false);
  };

  // Refresh usage for all models
  const refreshAllUsage = useCallback(async () => {
    if (isRefreshingUsage) return;
    setIsRefreshingUsage(true);

    // Parallel fetch; merge into existing so failed models keep their last data.
    const results = await Promise.allSettled(
      userModels.map((model) => api.queryModelUsage(model.internalId))
    );
    setModelUsageData((prev) => {
      const next = { ...prev };
      results.forEach((r, i) => {
        if (r.status === 'fulfilled' && r.value.success && r.value.data) {
          next[userModels[i].internalId] = r.value.data;
        }
      });
      return next;
    });
    setIsRefreshingUsage(false);
  }, [isRefreshingUsage, userModels]);

  // Refresh usage for a single model
  const refreshSingleUsage = async (modelId: string) => {
    if (refreshingUsageIds.has(modelId)) return;

    const model = userModels.find((m) => m.internalId === modelId);
    if (!model) return;

    setRefreshingUsageIds((prev) => new Set(prev).add(model.internalId));
    try {
      const result = await api.queryModelUsage(model.internalId);
      if (result.error === 'VOLC_AKSK_REQUIRED') {
        setVolcAkSkMissingIds((prev) => new Set(prev).add(model.internalId));
        return;
      }
      if (result.success && result.data) {
        const data = result.data;
        setModelUsageData((prev) => ({
          ...prev,
          [model.internalId]: data,
        }));
        setVolcAkSkMissingIds((prev) => {
          if (!prev.has(model.internalId)) return prev;
          const next = new Set(prev);
          next.delete(model.internalId);
          return next;
        });
      } else if (result.error) {
        showToast('error', result.error);
      }
    } catch {
      /* silent */
    } finally {
      setRefreshingUsageIds((prev) => {
        const next = new Set(prev);
        next.delete(model.internalId);
        return next;
      });
    }
  };

  // On mount / when models load, mark which Volcengine models lack AK/SK.
  useEffect(() => {
    const volcModels = userModels.filter((m) => isVolcengineUrl(m.baseUrl, m.anthropicUrl));
    Promise.all(
      volcModels.map(async (m) => {
        try {
          return { id: m.internalId, missing: !(await api.hasVolcAksk(m.internalId)) };
        } catch {
          return { id: m.internalId, missing: true };
        }
      })
    ).then((results) => {
      setVolcAkSkMissingIds(new Set(results.filter((r) => r.missing).map((r) => r.id)));
    });
  }, [userModels]);

  // Save AK/SK for a specific model, then refresh that model's usage.
  const saveVolcAksk = async (internalId: string, accessKey: string, secretKey: string) => {
    try {
      await api.saveVolcAksk(internalId, accessKey, secretKey);
      setVolcAkSkMissingIds((prev) => {
        const next = new Set(prev);
        next.delete(internalId);
        return next;
      });
      setVolcAkSkModelId(null);
      await refreshSingleUsage(internalId);
    } catch (e) {
      showToast('error', typeof e === 'string' ? e : String(e));
    }
  };

  // Model test function
  const handleTestModel = async () => {
    if (!testInput.trim() || !selectedModel || isTesting) return;

    const prompt = testInput.trim();
    setTestInput('');
    setIsTesting(true);
    testInputRef.current?.blur();

    // Smart protocol selection
    let effectiveProtocol = testProtocol;
    if (selectedModelData) {
      if (!selectedModelData.baseUrl && selectedModelData.anthropicUrl) {
        effectiveProtocol = 'anthropic';
      } else if (selectedModelData.baseUrl && !selectedModelData.anthropicUrl) {
        effectiveProtocol = 'openai';
      }
    }

    setTestOutput((prev) => [
      ...prev,
      `> ${prompt}`,
      `Sending request via ${effectiveProtocol === 'openai' ? 'OpenAI' : 'Anthropic'}...`,
    ]);

    try {
      if (!api.testModel) {
        setTestOutput((prev) => [...prev, 'Test API not available']);
        return;
      }

      const result = await api.testModel(selectedModel, prompt, effectiveProtocol);

      if (result.success) {
        setModelLatencies((prev) => ({ ...prev, [selectedModel]: result.latency }));
        setTestOutput((prev) => [
          ...prev,
          `Response in ${result.latency}ms`,
          result.response || 'No response',
        ]);
        // Reload model list to refresh test status
        if (api.getModels) {
          const updatedModels = await api.getModels();
          setUserModels(visibleModelNexusModels(updatedModels));
        }
      } else {
        // Sentinel -1 so the model card shows "Error" instead of the
        // pre-test "未测试" placeholder — see pingAllModels for the
        // same reasoning.
        setModelLatencies((prev) => ({ ...prev, [selectedModel]: -1 }));
        setTestOutput((prev) =>
          [
            ...prev,
            result.error || 'Unknown error',
            result.latency > 0 ? `(failed after ${result.latency}ms)` : '',
          ].filter(Boolean)
        );
      }
    } catch (error) {
      setModelLatencies((prev) => ({ ...prev, [selectedModel]: -1 }));
      setTestOutput((prev) => [...prev, String(error)]);
    } finally {
      setIsTesting(false);
    }
  };

  return (
    <ModelNexusContext.Provider
      value={{
        userModels,
        setUserModels,
        isLoadingModels,
        selectedModel,
        setSelectedModel,
        selectedModelData,
        viewMode,
        setViewMode,
        modelUsageData,
        isRefreshingUsage,
        refreshingUsageIds,
        setModelUsageData,
        volcAkSkMissingIds,
        setVolcAkSkMissingIds,
        volcAkSkModelId,
        setVolcAkSkModelId,
        saveVolcAksk,
        testInput,
        setTestInput,
        testOutput,
        setTestOutput,
        isTesting,
        arrowIndex,
        testProtocol,
        setTestProtocol,
        modelLatencies,
        pingingModelIds,
        modelTerminals,
        setModelTerminals,
        testInputRef,
        inputFocused,
        setInputFocused,
        cursorPos,
        setCursorPos,
        showAddModelModal,
        setShowAddModelModal,
        modelModalDestination,
        setModelModalDestination,
        modelModalAnimatingOut,
        editingModelId,
        setEditingModelId,
        newModelForm,
        setNewModelForm,
        showApiKey,
        setShowApiKey,
        keyDestroyed,
        setKeyDestroyed,
        closeModelModal,
        pingAllModels,
        refreshAllUsage,
        refreshSingleUsage,
        handleTestModel,
      }}
    >
      {children}
    </ModelNexusContext.Provider>
  );
}

// ===== Title Actions (view mode tabs + ping/refresh button) =====

export function ModelNexusTitleActions() {
  const { t } = useI18n();
  const { viewMode, setViewMode, pingAllModels, refreshAllUsage, isTesting, isRefreshingUsage } =
    useModelNexus();

  return (
    <div className="ml-auto flex-shrink-0 flex items-center gap-3">
      {/* View mode tabs */}
      <div className="flex gap-1 border border-cyber-border rounded-button overflow-hidden">
        <button
          onClick={() => setViewMode('config')}
          className={`px-3 py-1.5 text-sm font-mono transition-colors ${
            viewMode === 'config'
              ? 'bg-cyber-elevated text-cyber-text'
              : 'text-cyber-text-secondary hover:text-cyber-text'
          }`}
        >
          {t('model.config')}
        </button>
        <button
          onClick={() => setViewMode('usage')}
          className={`px-3 py-1.5 text-sm font-mono transition-colors ${
            viewMode === 'usage'
              ? 'bg-cyber-elevated text-cyber-text'
              : 'text-cyber-text-secondary hover:text-cyber-text'
          }`}
        >
          {t('model.usage')}
        </button>
      </div>

      {/* Action button - same height and style as tabs */}
      {viewMode === 'config' ? (
        <button
          onClick={pingAllModels}
          disabled={isTesting}
          className={`flex items-center gap-1.5 text-sm font-mono px-3 py-1.5 border rounded-button transition-colors ${
            !isTesting
              ? 'border-cyber-border text-cyber-text hover:bg-cyber-text/10'
              : 'border-cyber-border text-cyber-text-muted cursor-not-allowed'
          }`}
        >
          <RefreshCw size={13} className={isTesting ? 'animate-spin' : ''} />
          {t('btn.pingAll')}
        </button>
      ) : (
        <button
          onClick={refreshAllUsage}
          disabled={isRefreshingUsage}
          className={`flex items-center gap-1.5 text-sm font-mono px-3 py-1.5 border rounded-button transition-colors ${
            !isRefreshingUsage
              ? 'border-cyber-border text-cyber-text hover:bg-cyber-text/10'
              : 'border-cyber-border text-cyber-text-muted cursor-not-allowed'
          }`}
        >
          <RefreshCw size={13} className={isRefreshingUsage ? 'animate-spin' : ''} />
          {t('btn.refreshUsage')}
        </button>
      )}
    </div>
  );
}

// ===== Main Content (model card grid) =====

// Volcengine AK/SK config modal. Two fields, mounted fresh each open.
function VolcAkskModal({
  onClose,
  onSave,
  initialAk = '',
  initialSk = '',
}: {
  onClose: () => void;
  onSave: (accessKey: string, secretKey: string) => Promise<void>;
  initialAk?: string;
  initialSk?: string;
}) {
  const { t } = useI18n();
  const [ak, setAk] = useState(initialAk);
  const [sk, setSk] = useState(initialSk);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    if (!ak.trim() || !sk.trim()) return;
    setSaving(true);
    await onSave(ak.trim(), sk.trim());
    setSaving(false);
  };

  const pasteButton = (setter: (v: string) => void) => (
    <button
      type="button"
      onClick={async () => {
        try {
          const text = (await readClipboardText()).trim();
          if (text) setter(text);
        } catch {
          /* clipboard empty / unreadable - no-op */
        }
      }}
      className="absolute right-2 top-1/2 -translate-y-1/2 cursor-pointer text-xs text-cyber-text-secondary"
    >
      {t('model.paste')}
    </button>
  );

  const inputClass =
    'w-full bg-cyber-input border border-cyber-border px-2 py-1.5 pr-16 text-xs text-cyber-text font-mono focus:border-cyber-border focus:outline-none rounded-button';

  return (
    <div
      className="fixed inset-0 z-[9998] flex items-center justify-center"
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
    >
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />
      <div
        className="relative w-[450px] max-w-[90vw] border border-cyber-border/30 bg-cyber-surface shadow-2xl rounded-xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="h-px w-full bg-cyber-border" />
        <div className="px-6 pt-5 pb-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-cyber-text font-mono text-sm opacity-60">&gt;_</span>
            <span className="text-base font-bold text-cyber-text">{t('model.accessKey')}</span>
          </div>
          <button
            onClick={onClose}
            className="text-cyber-text-secondary hover:text-cyber-text transition-colors"
          >
            <X size={18} />
          </button>
        </div>
        <div className="px-5 pb-5">
          <div className="space-y-4">
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.akSkAccessKey')}
              </label>
              <div className="relative">
                <input
                  type="text"
                  placeholder="AK..."
                  value={ak}
                  onChange={(e) => setAk(e.target.value)}
                  autoFocus
                  className={inputClass}
                />
                {pasteButton(setAk)}
              </div>
            </div>
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.akSkSecretKey')}
              </label>
              <div className="relative">
                <input
                  type="text"
                  placeholder="SK..."
                  value={sk}
                  onChange={(e) => setSk(e.target.value)}
                  className={inputClass}
                />
                {pasteButton(setSk)}
              </div>
            </div>
          </div>
          <button
            type="button"
            onClick={() =>
              shellOpen('https://console.volcengine.com/iam/keymanage').catch(() =>
                window.open('https://console.volcengine.com/iam/keymanage', '_blank')
              )
            }
            className="block text-xs text-cyber-accent hover:opacity-80 pt-3"
          >
            https://console.volcengine.com/iam/keymanage
          </button>
          <div className="flex justify-end gap-3 pt-5">
            <button
              className="text-xs font-mono text-cyber-text-secondary hover:text-cyber-text px-3 py-1"
              onClick={onClose}
            >
              [{t('btn.cancel')}]
            </button>
            <button
              className="text-xs font-mono text-cyber-accent hover:opacity-80 px-3 py-1 disabled:opacity-40"
              onClick={handleSave}
              disabled={saving || !ak.trim() || !sk.trim()}
            >
              [{t('btn.save')}]
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// dnd-kit sortable wrapper for the model card grid. The grip handle at the
// bottom-left corner is the ONLY drag surface, so the card's own clicks and
// buttons (select / edit / delete / refresh) keep working untouched.
function SortableModelCard({
  id,
  dragLabel,
  children,
}: {
  id: string;
  dragLabel: string;
  children: React.ReactNode;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id,
  });
  return (
    <div
      ref={setNodeRef}
      data-drag-model={id}
      className={isDragging ? 'relative opacity-0' : 'relative'}
      style={{ transform: CSS.Transform.toString(transform), transition }}
    >
      {children}
      {/* No title attr: this is desktop software, not a web page — hover
          tooltips feel out of place. aria-label stays for screen readers. */}
      <button
        {...attributes}
        {...listeners}
        aria-label={dragLabel}
        className="absolute top-2 left-2 z-10 p-0.5 text-cyber-text-muted/60 hover:text-cyber-text transition-colors outline-none"
      >
        <GripVertical size={14} />
      </button>
    </div>
  );
}

export function ModelNexusMain() {
  const { t } = useI18n();
  const {
    userModels,
    isLoadingModels,
    viewMode,
    modelUsageData,
    setTestOutput: _setTestOutput,
    testProtocol: _testProtocol,
    modelLatencies,
    pingingModelIds,
    modelTerminals: _modelTerminals,
    isTesting: _isTesting,
    editingModelId: _editingModelId,
    setEditingModelId,
    setNewModelForm,
    setShowAddModelModal,
    setUserModels,
    keyDestroyed: _keyDestroyed,
    setKeyDestroyed,
    refreshSingleUsage,
    refreshingUsageIds,
    volcAkSkMissingIds,
    volcAkSkModelId,
    setVolcAkSkModelId,
    saveVolcAksk,
  } = useModelNexus();

  // Pre-fill values for the AK/SK modal (fetched when opening for a model).
  const [volcAkSkInitial, setVolcAkSkInitial] = useState<{
    access_key: string;
    secret_key: string;
  } | null>(null);

  const openAkskModal = async (internalId: string) => {
    try {
      setVolcAkSkInitial(await api.getVolcAksk(internalId));
    } catch {
      setVolcAkSkInitial(null);
    }
    setVolcAkSkModelId(internalId);
  };

  const handleCardEdit = useCallback(
    async (model: (typeof userModels)[0]) => {
      // Reload fresh model data from disk to get latest apiKey state
      let freshModel = model;
      try {
        const freshModels = await api.getModels();
        const found = freshModels.find((m) => m.internalId === model.internalId);
        if (found) {
          freshModel = found;
          // Also update the models list with fresh data
          setUserModels(visibleModelNexusModels(freshModels));
        }
      } catch {
        /* fallback to stale model */
      }

      setEditingModelId(freshModel.internalId);
      if (freshModel.apiKey?.startsWith('enc:v1:') && api.isKeyDestroyed) {
        api.isKeyDestroyed(freshModel.internalId).then((destroyed) => setKeyDestroyed(destroyed));
      } else {
        setKeyDestroyed(false);
      }
      setNewModelForm({
        name: freshModel.name,
        baseUrl: freshModel.baseUrl,
        anthropicUrl: freshModel.anthropicUrl || '',
        apiKey: freshModel.apiKey,
        modelId: freshModel.modelId || '',
      });
      setShowAddModelModal(true);
    },
    [setEditingModelId, setKeyDestroyed, setNewModelForm, setShowAddModelModal, setUserModels]
  );

  const handleCardDelete = useCallback(
    async (modelId: string) => {
      await api.deleteModel(modelId);
      setUserModels((prev) => prev.filter((m) => m.internalId !== modelId));
    },
    [setUserModels]
  );

  // Drag-reorder: pointer (5px activation so plain clicks pass through) +
  // keyboard (a11y). On drop, reorder in place and persist the full visible
  // order — a failed write just reverts on next reload (best-effort).
  // autoScroll is disabled on purpose: the grid lives in an overflow-y-auto
  // container and edge-drag auto-scroll felt janky. A DragOverlay (portal)
  // floats the dragged card above everything, so the scroll container never
  // clips the lifted card or its shadow.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates })
  );

  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [activeDragWidth, setActiveDragWidth] = useState(0);

  const handleDragStart = (event: DragStartEvent) => {
    const id = String(event.active.id);
    setActiveDragId(id);
    // dnd-kit's active.rect is not measured yet at onDragStart (null), so
    // measure the grid node directly for the DragOverlay ghost's width.
    const el = document.querySelector<HTMLElement>(`[data-drag-model="${id}"]`);
    setActiveDragWidth(el?.offsetWidth ?? 0);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    setActiveDragId(null);
    const activeId = String(event.active.id);
    const overId = String(event.over?.id ?? '');
    if (!overId || activeId === overId) return;
    const oldIndex = userModels.findIndex((m) => m.internalId === activeId);
    const newIndex = userModels.findIndex((m) => m.internalId === overId);
    if (oldIndex < 0 || newIndex < 0) return;
    const next = arrayMove(userModels, oldIndex, newIndex);
    setUserModels(next);
    api.reorderModels(next.map((m) => m.internalId)).catch((err) => {
      console.error('reorderModels failed:', err);
    });
  };

  const handleDragCancel = () => setActiveDragId(null);

  const activeDragModel = activeDragId
    ? userModels.find((m) => m.internalId === activeDragId)
    : undefined;

  // Model card body, shared by the grid item and the DragOverlay ghost (the
  // ghost is wrapped in pointer-events-none so its interactions are inert).
  const renderModelCard = (model: (typeof userModels)[0]) => {
    const protocols: ('openai' | 'anthropic')[] = [];
    if (model.baseUrl) protocols.push('openai');
    if (model.anthropicUrl) protocols.push('anthropic');
    const isDemo = model.modelType === 'DEMO';
    return (
      <ModelCard
        id={model.internalId}
        name={model.name}
        type={model.modelType || ''}
        baseUrl={model.baseUrl}
        anthropicUrl={model.anthropicUrl}
        modelId={model.modelId || ''}
        protocols={protocols}
        latency={modelLatencies[model.internalId] ?? model.openaiLatency}
        openaiTested={model.openaiTested}
        anthropicTested={model.anthropicTested}
        isPinging={pingingModelIds.has(model.internalId)}
        viewMode={viewMode}
        usageData={modelUsageData[model.internalId]}
        dragHandlePad
        onEdit={isDemo ? undefined : () => handleCardEdit(model)}
        onDelete={isDemo ? undefined : () => handleCardDelete(model.internalId)}
        onRefresh={() => refreshSingleUsage(model.internalId)}
        isRefreshingUsage={refreshingUsageIds.has(model.internalId)}
        onAccessKey={
          isVolcengineUrl(model.baseUrl, model.anthropicUrl)
            ? () => openAkskModal(model.internalId)
            : undefined
        }
        akSkMissing={volcAkSkMissingIds.has(model.internalId)}
      />
    );
  };

  return (
    <>
      <div className="flex-1 overflow-y-auto">
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          onDragCancel={handleDragCancel}
          autoScroll={false}
        >
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
                {/* User custom models (drag to reorder) */}
                <SortableContext
                  items={userModels.map((m) => m.internalId)}
                  strategy={rectSortingStrategy}
                >
                  {userModels.map((model) => (
                    <SortableModelCard
                      key={model.internalId}
                      id={model.internalId}
                      dragLabel={t('model.dragSort')}
                    >
                      {renderModelCard(model)}
                    </SortableModelCard>
                  ))}
                </SortableContext>

                {/* Add new model button */}
                <div
                  className="h-48 border border-dashed border-cyber-border flex flex-col items-center justify-center hover:border-cyber-border cursor-pointer transition-all rounded-card text-cyber-text-secondary hover:text-cyber-text"
                  onClick={() => {
                    setNewModelForm({
                      name: '',
                      baseUrl: '',
                      anthropicUrl: '',
                      apiKey: '',
                      modelId: '',
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
          {/* Floating drag ghost — rendered in a portal so the scroll
              container never clips the lifted card or its shadow. */}
          <DragOverlay>
            {activeDragModel && (
              <div
                style={{ width: activeDragWidth }}
                className="pointer-events-none relative shadow-2xl"
              >
                {renderModelCard(activeDragModel)}
                <span
                  aria-hidden="true"
                  className="absolute top-2 left-2 z-10 p-0.5 text-cyber-text-muted/60"
                >
                  <GripVertical size={14} />
                </span>
              </div>
            )}
          </DragOverlay>
        </DndContext>
      </div>
      {volcAkSkModelId && (
        <VolcAkskModal
          onClose={() => setVolcAkSkModelId(null)}
          onSave={(ak, sk) => saveVolcAksk(volcAkSkModelId, ak, sk)}
          initialAk={volcAkSkInitial?.access_key ?? ''}
          initialSk={volcAkSkInitial?.secret_key ?? ''}
        />
      )}
    </>
  );
}

// ===== Right Panel (Provider / Relay tabs) =====

// Right-panel Providers + Relays list. Two tiers:
// • Remote-first: `api.getModelDirectory()` hits echobird.ai/api/model-
//   directory/index.json (with backend-side disk cache). Lets us add a
//   vendor or fix a baseUrl without shipping an app release.
// • Bundled fallback: `src/data/modelDirectory.json` shipped in the app
//   binary. Used when both remote and disk-cache are unavailable
//   (offline, first install + firewall, etc.), and as the immediate
//   first paint before the network round-trip lands.
//
// Per-entry fields (name / url / baseUrl / anthropicUrl / modelId /
// region) and ordering rules (zh locale → 'cn' first, others → 'global'
// first) are unchanged. Edit either the remote JSON or the bundled
// JSON; remote wins when both are present.
type DirectoryEntry = {
  name: string;
  url: string;
  baseUrl: string;
  anthropicUrl: string;
  modelId: string;
  // Optional list of model ids this vendor serves on the same endpoint (e.g.
  // Volcengine exposes many). When present (≥2), clicking the row surfaces a
  // dropdown in the Add-Model modal so users pick instead of hand-typing.
  // model id list is the same across the OpenAI/Anthropic endpoints, so no
  // per-protocol split is needed.
  modelIds?: string[];
  region: 'cn' | 'global';
};

const BUNDLED_PROVIDERS: DirectoryEntry[] = modelDirectory.providers as DirectoryEntry[];
const BUNDLED_RELAYS: DirectoryEntry[] = modelDirectory.relays as DirectoryEntry[];

// Locale-aware reorder: zh* surfaces 'cn' entries first; everything else
// surfaces 'global' first. Within each region we keep the curated JSON order
// (Array.prototype.sort is stable in modern engines).
function sortByLocale(list: DirectoryEntry[], locale: string): DirectoryEntry[] {
  const cnFirst = locale.toLowerCase().startsWith('zh');
  const weight = (e: DirectoryEntry) =>
    cnFirst ? (e.region === 'cn' ? 0 : 1) : e.region === 'global' ? 0 : 1;
  return [...list].sort((a, b) => weight(a) - weight(b));
}

function ProviderRow({ entry, onAdd }: { entry: DirectoryEntry; onAdd: () => void }) {
  const iconSrc = getModelIcon(entry.name, '');
  const hostname = (() => {
    try {
      return new URL(entry.url).hostname;
    } catch {
      return entry.url;
    }
  })();
  const openSite = () => shellOpen(entry.url).catch(() => window.open(entry.url, '_blank'));
  // Two click+hover zones (50/50). Buttons sit underneath; the visual content
  // floats on top with pointer-events-none so clicks pass through to whichever
  // half they land on. Named groups (group/left, group/right) let the icons
  // brighten in sync with their half's hover state.
  return (
    <div className="relative flex items-stretch rounded overflow-hidden bg-cyber-surface">
      {/* Click + hover layer (two equal halves) */}
      <button
        type="button"
        onClick={onAdd}
        aria-label={`Add model: ${entry.name}`}
        className="group/left flex-1 min-h-[64px] bg-gradient-to-r from-transparent to-transparent hover:from-cyber-text/15 hover:to-transparent transition-[background-image] duration-200"
      />
      <button
        type="button"
        onClick={openSite}
        aria-label={`Open ${entry.name} website`}
        className="group/right flex-1 min-h-[64px] bg-gradient-to-l from-transparent to-transparent hover:from-cyber-text/15 hover:to-transparent transition-[background-image] duration-200"
      />

      {/* Visual content overlay (does not capture clicks) */}
      <div className="pointer-events-none absolute inset-0 flex items-center gap-3 px-3">
        <Plus
          size={22}
          strokeWidth={2.5}
          className="flex-shrink-0 text-cyber-text-muted group-hover/left:text-cyber-text group-hover/left:scale-110 transition-all"
        />
        <div className="flex-shrink-0">
          {iconSrc ? (
            <img
              src={iconSrc}
              alt=""
              className="w-6 h-6"
              onError={(e) => {
                (e.target as HTMLImageElement).style.display = 'none';
              }}
            />
          ) : (
            <div className="w-6 h-6 flex items-center justify-center text-cyber-text">
              <Box size={22} />
            </div>
          )}
        </div>
        <div className="flex-1 min-w-0 flex flex-col justify-center">
          <div className="text-sm font-bold truncate leading-none">{entry.name}</div>
          <div className="text-[10px] text-cyber-text-secondary truncate leading-tight mt-1 opacity-70">
            {hostname}
          </div>
        </div>
        <ExternalLink
          size={18}
          strokeWidth={2.25}
          className="flex-shrink-0 text-cyber-text-muted group-hover/right:text-cyber-text group-hover/right:scale-110 transition-all"
        />
      </div>
    </div>
  );
}

export function ModelNexusPanel() {
  const { t, locale } = useI18n();

  // Bundled JSON paints immediately, remote swaps in if newer content
  // is available. Failure modes (remote down + cache miss): backend
  // returns null, we keep the bundled state forever. No flicker, no
  // blank panel.
  const [providers, setProviders] = useState<DirectoryEntry[]>(BUNDLED_PROVIDERS);
  const [relays, setRelays] = useState<DirectoryEntry[]>(BUNDLED_RELAYS);

  useEffect(() => {
    let cancelled = false;
    api
      .getModelDirectory()
      .then((remote) => {
        if (cancelled || !remote) return;
        setProviders(remote.providers as DirectoryEntry[]);
        setRelays(remote.relays as DirectoryEntry[]);
      })
      .catch(() => {
        /* keep bundled */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // One scrolling list under two headed sections (no tab switcher). Group order
  // flips with the UI language: zh locales put the 中转站 (cn) section on top;
  // other locales surface the providers (global) section first, 中转站 below.
  const sections = useMemo(() => {
    const relaysSorted = sortByLocale(relays, locale);
    const providersSorted = sortByLocale(providers, locale);
    const cnFirst = locale.toLowerCase().startsWith('zh');
    const ordered: { key: string; title: string; entries: DirectoryEntry[] }[] = cnFirst
      ? [
          { key: 'relays', title: t('model.relays'), entries: relaysSorted },
          { key: 'providers', title: t('model.providers'), entries: providersSorted },
        ]
      : [
          { key: 'providers', title: t('model.providers'), entries: providersSorted },
          { key: 'relays', title: t('model.relays'), entries: relaysSorted },
        ];
    // Hide a group the directory doesn't currently provide.
    return ordered.filter((s) => s.entries.length > 0);
  }, [locale, providers, relays, t]);
  const { setNewModelForm, setEditingModelId, setShowAddModelModal } = useModelNexus();

  const handleAddFromEntry = useCallback(
    (entry: DirectoryEntry) => {
      const options = entry.modelIds ?? [];
      setNewModelForm({
        name: entry.name,
        baseUrl: entry.baseUrl,
        anthropicUrl: entry.anthropicUrl,
        apiKey: '',
        // Default to the curated default, else the first listed id, else blank.
        modelId: entry.modelId || options[0] || '',
        modelIdOptions: options,
      });
      setEditingModelId(null);
      setShowAddModelModal(true);
    },
    [setNewModelForm, setEditingModelId, setShowAddModelModal]
  );

  return (
    <div className="flex-1 p-2 overflow-y-auto">
      {sections.map((section) => (
        <div key={section.key} className="mb-3">
          <div className="px-1 pb-1.5 flex items-center gap-2">
            <span className="flex-1 h-px bg-cyber-border/60" />
            <span className="text-[11px] font-semibold uppercase tracking-wider text-cyber-text-secondary whitespace-nowrap">
              {section.title}
            </span>
            <span className="flex-1 h-px bg-cyber-border/60" />
          </div>
          <div className="space-y-2">
            {section.entries.map((entry) => (
              <ProviderRow key={entry.name} entry={entry} onAdd={() => handleAddFromEntry(entry)} />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

// ===== Add/Edit Model Modal =====

export function AddModelModal() {
  const { t } = useI18n();
  const { showToast } = useToast();
  const [isSavingModel, setIsSavingModel] = useState(false);
  const { addSelectedModel, selectedIds } = useFreeModels();
  const {
    showAddModelModal,
    modelModalAnimatingOut,
    editingModelId,
    setEditingModelId,
    newModelForm,
    setNewModelForm,
    keyDestroyed,
    closeModelModal,
    setUserModels,
    setShowAddModelModal,
    modelModalDestination,
    setModelModalDestination,
  } = useModelNexus();

  if (!showAddModelModal) return null;

  // One-click paste affordance for the modal's free-text fields. Plain text,
  // no button styling / hover effect — the label never changes. The `normalize`
  // arg lets a URL field run the pasted text through the SAME normalization the
  // typing path uses, so a pasted full endpoint (`/v1/chat/completions`) is
  // trimmed to a clean base instead of silently keeping a doubled path that
  // would 404 every request while manually typed URLs work.
  //
  // These URL fields have NO overlay control to their right (unlike the API
  // key field, which has the encrypt/decrypt lock button at right-2), so the
  // paste label sits flush at the right edge — right-2 — and the input keeps
  // enough right padding (pr-16) to clear the widest locale's "貼り付け" text.
  const pasteButton = (field: 'baseUrl' | 'anthropicUrl', normalize: (v: string) => string) => (
    <button
      type="button"
      onClick={async () => {
        try {
          const text = (await readClipboardText()).trim();
          if (text) {
            const v = normalize(text);
            setNewModelForm((prev) => ({ ...prev, [field]: v }));
          }
        } catch {
          /* clipboard empty / unreadable — no-op */
        }
      }}
      className="absolute right-2 top-1/2 -translate-y-1/2 cursor-pointer text-xs text-cyber-text-secondary"
    >
      {t('model.paste')}
    </button>
  );

  return (
    <div
      className={`fixed inset-0 z-[9998] flex items-center justify-center transition-all duration-200 ${modelModalAnimatingOut ? 'opacity-0' : 'opacity-100'}`}
      onKeyDown={(e) => {
        if (e.key === 'Escape') closeModelModal();
      }}
    >
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={closeModelModal} />

      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="model-config-dialog-title"
        className={`relative w-[450px] max-w-[90vw] border border-cyber-border/30 bg-cyber-surface shadow-2xl rounded-xl overflow-hidden transition-all duration-200 ${modelModalAnimatingOut ? 'scale-95 opacity-0' : 'scale-100 opacity-100'}`}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Top accent line */}
        <div className="h-px w-full bg-cyber-border" />

        {/* Header */}
        <div className="px-6 pt-5 pb-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-cyber-text font-mono text-sm opacity-60">&gt;_</span>
            <span id="model-config-dialog-title" className="text-base font-bold text-cyber-text">
              {editingModelId
                ? t('model.editConfig')
                : modelModalDestination === 'freeRouter'
                  ? t('freeModels.addToRouter')
                  : t('btn.addModel')}
            </span>
          </div>
          <button
            onClick={closeModelModal}
            aria-label={t('btn.close')}
            className="text-cyber-text-secondary hover:text-cyber-text transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        {/* Form */}
        <div className="px-5 pb-5">
          <div className="space-y-4">
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.name')}
              </label>
              <input
                type="text"
                placeholder="e.g. My Model"
                value={newModelForm.name}
                onChange={(e) => setNewModelForm((prev) => ({ ...prev, name: e.target.value }))}
                className="w-full bg-cyber-input border border-cyber-border px-2 py-1.5 text-xs text-cyber-text font-mono focus:border-cyber-border focus:outline-none rounded-button"
              />
            </div>
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.openaiUrl')}
              </label>
              <div className="relative">
                <input
                  type="text"
                  placeholder="https://x.x.com/v1"
                  value={newModelForm.baseUrl}
                  onChange={(e) =>
                    setNewModelForm((prev) => ({
                      ...prev,
                      baseUrl: normalizeOpenaiUrl(e.target.value),
                    }))
                  }
                  className="w-full bg-cyber-input border border-cyber-border px-2 py-1.5 pr-16 text-xs text-cyber-text font-mono focus:border-cyber-border focus:outline-none rounded-button"
                />
                {pasteButton('baseUrl', normalizeOpenaiUrl)}
              </div>
            </div>
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.anthropicUrl')}
              </label>
              <div className="relative">
                <input
                  type="text"
                  placeholder="https://x.x.com/anthropic"
                  value={newModelForm.anthropicUrl}
                  onChange={(e) =>
                    setNewModelForm((prev) => ({
                      ...prev,
                      anthropicUrl: normalizeAnthropicUrl(e.target.value),
                    }))
                  }
                  className="w-full bg-cyber-input border border-cyber-border px-2 py-1.5 pr-16 text-xs text-cyber-text font-mono focus:border-cyber-border focus:outline-none rounded-button"
                />
                {pasteButton('anthropicUrl', normalizeAnthropicUrl)}
              </div>
            </div>
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.modelId')}
              </label>
              {/* Single searchable combobox: free-type any id, with a filtered
                  suggestion dropdown when the clicked directory entry carries a
                  model id list. No options → plain input. */}
              <ModelIdCombobox
                value={newModelForm.modelId}
                onChange={(v) => setNewModelForm((prev) => ({ ...prev, modelId: v }))}
                options={newModelForm.modelIdOptions}
                placeholder={t('model.modelIdPlaceholder')}
                onPaste={async () => {
                  try {
                    const text = (await readClipboardText()).trim();
                    if (text) setNewModelForm((prev) => ({ ...prev, modelId: text }));
                  } catch {
                    /* clipboard empty / unreadable — no-op */
                  }
                }}
                pasteLabel={t('model.paste')}
              />
            </div>
            <div>
              <label className="block text-xs text-cyber-text-secondary mb-1">
                {t('model.apiKey')}
              </label>
              <div className="relative">
                <input
                  type="text"
                  placeholder="sk-..."
                  value={
                    newModelForm.apiKey.startsWith('enc:v1:')
                      ? '•••••••••••••••'
                      : newModelForm.apiKey
                  }
                  onChange={(e) => setNewModelForm((prev) => ({ ...prev, apiKey: e.target.value }))}
                  className="w-full bg-cyber-input border border-cyber-border px-2 py-1.5 pr-20 text-xs text-cyber-text font-mono focus:border-cyber-border focus:outline-none rounded-button"
                  readOnly={newModelForm.apiKey.startsWith('enc:v1:')}
                />
                {/* One-click paste from clipboard — plain text affordance (no
                    border / button styling, no hover effect, label never
                    changes). Shown only while the key is editable (plaintext);
                    hidden once encrypted (the field is read-only then). */}
                {newModelForm.apiKey !== 'not-needed' &&
                  !newModelForm.apiKey.startsWith('enc:v1:') && (
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          const text = (await readClipboardText()).trim();
                          if (text) {
                            setNewModelForm((prev) => ({ ...prev, apiKey: text }));
                          }
                        } catch {
                          /* clipboard empty / unreadable — no-op */
                        }
                      }}
                      className="absolute right-8 top-1/2 -translate-y-1/2 cursor-pointer text-xs text-cyber-text-secondary"
                    >
                      {t('model.paste')}
                    </button>
                  )}
                {newModelForm.apiKey !== 'not-needed' && (
                  <button
                    type="button"
                    disabled={!newModelForm.apiKey}
                    onClick={async () => {
                      if (newModelForm.apiKey.startsWith('enc:v1:')) {
                        try {
                          const plain = await api.decryptSecret(newModelForm.apiKey);
                          const newKey = plain || '';
                          setNewModelForm((prev) => ({ ...prev, apiKey: newKey }));
                          if (editingModelId) {
                            setUserModels((prev) =>
                              prev.map((m) =>
                                m.internalId === editingModelId ? { ...m, apiKey: newKey } : m
                              )
                            );
                          }
                        } catch {
                          setNewModelForm((prev) => ({ ...prev, apiKey: '' }));
                        }
                      } else {
                        try {
                          const encrypted = await api.encryptSecret(newModelForm.apiKey);
                          setNewModelForm((prev) => ({ ...prev, apiKey: encrypted }));
                          if (editingModelId) {
                            setUserModels((prev) =>
                              prev.map((m) =>
                                m.internalId === editingModelId ? { ...m, apiKey: encrypted } : m
                              )
                            );
                          }
                        } catch {
                          // stay plaintext on failure
                        }
                      }
                    }}
                    className={`absolute right-2 top-1/2 -translate-y-1/2 transition-colors hover:opacity-80 ${!newModelForm.apiKey ? 'opacity-30 cursor-not-allowed' : ''}`}
                  >
                    {newModelForm.apiKey.startsWith('enc:v1:') ? (
                      <Lock size={14} className="text-cyber-accent" />
                    ) : (
                      <Unlock size={14} className="text-cyber-text/70" />
                    )}
                  </button>
                )}
              </div>
              {/* Encryption hint — always rendered (locale-aware exact
                  height, no residual space) and toggled via visibility
                  so the form doesn't shift on encrypt / destroy. */}
              <div
                aria-hidden={!newModelForm.apiKey.startsWith('enc:v1:')}
                className={`mt-1 text-xs leading-tight ${
                  keyDestroyed ? 'text-red-400' : 'text-cyber-text/60'
                } ${!newModelForm.apiKey.startsWith('enc:v1:') ? 'invisible' : ''}`}
              >
                {keyDestroyed ? t('key.destroyed') : t('key.encrypted')}
              </div>
            </div>
          </div>
        </div>

        {/* Footer buttons */}
        <div className="flex border-t border-cyber-border">
          <button
            onClick={closeModelModal}
            className="flex-1 px-4 py-3 text-[14px] font-semibold text-cyber-text-secondary hover:text-cyber-text hover:bg-cyber-elevated transition-all border-r border-cyber-border"
          >
            {t('model.escCancel')}
          </button>
          <button
            type="button"
            disabled={isSavingModel}
            onClick={async () => {
              if (isSavingModel) return;
              if (
                !newModelForm.name.trim() ||
                !newModelForm.baseUrl.trim() ||
                !newModelForm.modelId.trim() ||
                !newModelForm.apiKey.trim()
              ) {
                showToast('warning', t('model.requiredFields'));
                return;
              }
              if (!isValidModelBaseUrl(newModelForm.baseUrl)) {
                showToast('warning', t('model.invalidOpenaiUrl'));
                return;
              }
              if (
                modelModalDestination === 'freeRouter' &&
                !editingModelId &&
                selectedIds.size >= api.SMART_ROUTER_CANDIDATE_LIMIT
              ) {
                showToast('warning', t('freeModels.router.limitReached'));
                return;
              }

              setIsSavingModel(true);
              try {
                if (editingModelId) {
                  const updatedModel = await api.updateModel(editingModelId, {
                    name: newModelForm.name,
                    baseUrl: newModelForm.baseUrl,
                    anthropicUrl: newModelForm.anthropicUrl,
                    apiKey: newModelForm.apiKey,
                    modelId: newModelForm.modelId,
                  });
                  if (updatedModel) {
                    setUserModels((prev) =>
                      prev.map((m) => (m.internalId === editingModelId ? updatedModel : m))
                    );
                  }
                } else {
                  const newModel = await api.addModel({
                    name: newModelForm.name,
                    baseUrl: newModelForm.baseUrl,
                    anthropicUrl: newModelForm.anthropicUrl || undefined,
                    apiKey: newModelForm.apiKey,
                    modelId: newModelForm.modelId,
                    scope: modelModalDestination === 'freeRouter' ? 'smartRouter' : 'modelCenter',
                  });
                  if (modelModalDestination === 'freeRouter') {
                    try {
                      await addSelectedModel({
                        internalId: newModel.internalId,
                        name: newModel.name,
                        baseUrl: newModel.baseUrl,
                        modelId: newModel.modelId ?? newModelForm.modelId,
                      });
                    } catch (error) {
                      await api.deleteModel(newModel.internalId).catch((rollbackError) => {
                        console.error('Rollback smart router model failed:', rollbackError);
                      });
                      throw error;
                    }
                  } else {
                    setUserModels((prev) => [...prev, newModel]);
                  }
                }

                setEditingModelId(null);
                setNewModelForm({
                  name: '',
                  baseUrl: '',
                  anthropicUrl: '',
                  apiKey: '',
                  modelId: '',
                });
                setShowAddModelModal(false);
                setModelModalDestination('modelNexus');
              } catch (error) {
                console.error('Save model failed:', error);
                showToast('error', t('error.requestFailed'));
              } finally {
                setIsSavingModel(false);
              }
            }}
            className={`flex-1 px-4 py-3 text-[14px] font-semibold transition-all ${
              isSavingModel
                ? 'text-cyber-text-muted cursor-not-allowed'
                : 'text-cyber-text hover:bg-cyber-text/10'
            }`}
          >
            {t('model.enterSave')}
          </button>
        </div>
      </div>
    </div>
  );
}
