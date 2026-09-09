// ModelCard component

import React, { useState, useEffect } from 'react';
import { RefreshCw } from 'lucide-react';
import { useConfirm } from '../ConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import type { ModelUsageData } from '../../api/tauri';
import type { TKey } from '../../i18n/types';

// Smart icon detection — match model name/ID to icon file
export const getModelIcon = (name: string, modelId?: string): string | null => {
  const nameText = name.toLowerCase();
  const modelIdText = (modelId || '').toLowerCase();

  // Provider/directory rows do not pass a real modelId. In that case the
  // vendor name must win over model-family words embedded in the title.
  if (!modelIdText) {
    if (['ccvibe', 'cc vibe', 'cc-vibe'].some((kw) => nameText.includes(kw))) {
      return './icons/models/ccvibe.png';
    }
  }

  const text = `${nameText} ${modelIdText}`;

  // Matching rules: keywords -> icon file
  const iconMap: [string[], string][] = [
    // Qwen model (紫色): model id (qwen3.8-max-preview etc.) or「通义」.
    // MUST precede the platform rule — the platform row's text is CJK「千问」
    // (no `qwen` token), and the token-plan URL carries no `qwen` substring, so
    // this rule only fires when a real qwen modelId is present (config tab).
    [['qwen', '通义', 'tongyi'], 'qwen'],
    // Qwen platform (白底): platform name「千问AI平台」or token-plan/bailian
    // host. Catches the usage-tab URL and the right-panel provider row, which
    // have no modelId and thus miss the model rule above.
    [['千问', 'qianwenai', 'maas.aliyuncs', 'bailian'], 'qianwen'],
    [['claude', 'anthropic', 'sonnet', 'opus', 'haiku'], 'claude'],
    [['gpt', 'openai', 'chatgpt', 'o1', 'o3'], 'chatgpt'],
    [['gemma'], 'google'],
    [['gemini', 'palm'], 'gemini'],
    [['deepseek'], 'deepseek'],
    [['mistral', 'mixtral'], 'mistral'],
    [['minimax'], 'minimax'],
    [['grok', 'x.ai'], 'grok'],
    [['groq'], 'groq'],
    [['cerebras'], 'cerebras'],
    [['modelscope'], 'modelscope'],
    [['ollama'], 'ollama'],
    [['sambanova', 'samba nova'], 'sambanova'],
    [['scaleway'], 'scaleway'],
    [['typhoon', 'opentyphoon'], 'typhoon'],
    [['w&b', 'wandb', 'weights & biases'], 'wandb'],
    [['kimi', 'moonshot'], 'kimi'],
    [['longcat', '美团', 'meituan', '龙猫'], 'longcat'],
    [['glm', 'zhipu', '智谱', 'z.ai', 'bigmodel'], 'glm'],
    [['ernie', 'wenxin', '文心'], 'ernie'],
    [['hunyuan', '混元'], 'hunyuan'],
    [['cohere', 'command'], 'cohere'],
    [['perplexity', 'pplx'], 'perplexity'],
    [['together'], 'together'],
    [['volcengine', 'volces', '火山', 'ark.cn-beijing'], 'volcengine'],
    [['byteplus', 'bytepluses'], 'byteplus'],
    [['doubao', '豆包', 'bytedance'], 'bytedance'],
    [['xiaomi', '小米', 'mimo'], 'xiaomi'],
    [['nemotron', 'nvidia'], 'nemotron'],
    [['stepfun', 'step', '阶跃'], 'stepfun'],
    [['granite', 'ibm'], 'granite'],
    [['meta'], 'meta'],
    [['openrouter'], 'openrouter'],
    [['worldrouter'], 'worldrouter'],
    [['b.ai', 'bai'], 'b-ai'],
    [['agnes'], 'agnes'],
    // OpenCode (Zen / Go) — a gateway hosting many vendors' models, matched
    // by the provider-row name. The model brand in the id must win, so this
    // stays below the model-brand rules.
    [['opencode', 'open code'], 'opencode'],
    // Resellers (pure compute providers that host third-party models, e.g.
    // CC Vibe) go LAST. A model card carries a modelId that identifies the
    // actual model brand (glm/kimi/deepseek/minimax), and the model logo must
    // win. The vendor logo only matches when the modelId has no recognized
    // brand — such as provider rows, which pass modelId=''. Model ID and vendor
    // are separate concerns; do not move resellers above model brands.
    [['ccvibe', 'cc vibe', 'cc-vibe'], 'ccvibe'],
    // 88API — token aggregation relay hosting many model brands; matches on
    // its own name only, so it never outranks a recognized model brand above.
    [['88api'], '88api'],
    // APIMart — API image/video relay; own-name match only (model brands win).
    [['apimart', 'api mart'], 'apimart'],
  ];

  for (const [keywords, icon] of iconMap) {
    if (keywords.some((kw) => text.includes(kw))) {
      if (icon === 'worldrouter') return './icons/models/worldrouter.png';
      if (icon === 'b-ai') return './icons/models/b-ai.ico';
      if (icon === 'agnes') return './icons/models/agnes.png';
      if (icon === 'ccvibe') return './icons/models/ccvibe.png';
      if (icon === '88api') return './icons/models/88api.png';
      if (icon === 'apimart') return './icons/models/apimart.png';
      if (icon === 'byteplus') return './icons/models/byteplus.png';
      if (icon === 'qianwen') return './icons/models/qianwen.png';
      return `./icons/models/${icon}.svg`;
    }
  }
  return null;
};

// Card skeleton (loading state)
export const ModelCardSkeleton = () => (
  <div className="h-48 p-4 bg-cyber-surface rounded-card animate-pulse">
    <div className="h-3 w-16 bg-cyber-border rounded mb-2"></div>
    <div className="h-5 w-32 bg-cyber-border rounded mb-4"></div>
    <div className="space-y-2">
      <div className="h-3 w-full bg-cyber-border/50 rounded"></div>
      <div className="h-3 w-3/4 bg-cyber-border/50 rounded"></div>
      <div className="h-3 w-1/2 bg-cyber-border/50 rounded"></div>
    </div>
    <div className="mt-4 flex gap-2">
      <div className="h-5 w-14 bg-cyber-border/30 rounded"></div>
      <div className="h-5 w-14 bg-cyber-border/30 rounded"></div>
    </div>
  </div>
);

// ModelCard props interface
export interface ModelCardProps {
  id: string;
  name: string;
  type: string; // provider / category
  baseUrl?: string; // API endpoint (OpenAI)
  anthropicUrl?: string; // API endpoint (Anthropic)
  modelId?: string; // model ID (provider-defined)
  latency?: number; // latency in ms, undefined = untested
  protocols?: ('openai' | 'anthropic')[]; // supported API protocols
  openaiTested?: boolean; // OpenAI protocol tested
  anthropicTested?: boolean; // Anthropic protocol tested
  isPinging?: boolean; // currently pinging (shows decode animation)
  viewMode?: 'config' | 'usage'; // display mode
  usageData?: ModelUsageData; // usage quota data
  onEdit?: () => void; // edit callback
  onDelete?: () => void; // delete callback
  onRefresh?: () => void; // refresh usage callback (usage mode only)
  isRefreshingUsage?: boolean; // usage refresh in progress (usage mode only)
  onAccessKey?: () => void; // open AK/SK config modal (Volcengine usage mode)
  akSkMissing?: boolean; // AK/SK not yet configured -> show hint
  /** Add top/bottom spacing around the name row so the overlaid top-left drag
   *  handle doesn't cover it, keeping the name left-aligned. Only used by the
   *  Model Center's sortable grid. */
  dragHandlePad?: boolean;
}

// Matrix decode animation — characters scramble then lock in sequence
const MATRIX_CHARS = 'ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789';
const TARGET_TEXT = 'ECHOBIRD';

// Format countdown time (ms to human readable) - i18n aware
const formatCountdown = (ms: number, t: (key: TKey) => string): string => {
  const totalSeconds = Math.floor(Math.max(0, ms) / 1000);
  const days = Math.floor(totalSeconds / 86400);
  const hours = Math.floor((totalSeconds % 86400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);

  if (days > 0) {
    return t('model.countdown.days')
      .replace('{d}', String(days))
      .replace('{h}', String(hours))
      .replace('{m}', String(minutes));
  } else if (hours > 0) {
    return t('model.countdown.hours').replace('{h}', String(hours)).replace('{m}', String(minutes));
  } else {
    return t('model.countdown.minutes').replace('{m}', String(minutes));
  }
};

// Generate random character
const randomChar = () => MATRIX_CHARS[Math.floor(Math.random() * MATRIX_CHARS.length)];

export const MatrixDecode = ({ duration = 2000 }: { duration?: number }) => {
  // Initialize with random chars immediately
  const [chars, setChars] = useState<string[]>(() =>
    Array(TARGET_TEXT.length)
      .fill(0)
      .map(() => randomChar())
  );
  const [locked, setLocked] = useState<boolean[]>(Array(TARGET_TEXT.length).fill(false));

  useEffect(() => {
    // Calculate lock interval for each character
    const totalSteps = TARGET_TEXT.length;
    // Reserve 20% of time for final state, allocate remaining 80% for sequential locking
    const stepInterval = (duration * 0.8) / totalSteps;
    // Character scramble speed (min 30ms to stay visible)
    const tickRate = Math.max(30, stepInterval / 2);

    // Random character rolling
    const interval = setInterval(() => {
      setChars((prev) => prev.map((_, i) => (locked[i] ? TARGET_TEXT[i] : randomChar())));
    }, tickRate);

    // Lock characters one by one
    const lockTimers = TARGET_TEXT.split('').map(
      (_, i) =>
        setTimeout(
          () => {
            setLocked((prev) => {
              const next = [...prev];
              next[i] = true;
              return next;
            });
            setChars((prev) => {
              const next = [...prev];
              next[i] = TARGET_TEXT[i];
              return next;
            });
          },
          duration * 0.2 + i * stepInterval
        ) // Initial delay 20%
    );

    return () => {
      clearInterval(interval);
      lockTimers.forEach((t) => clearTimeout(t));
    };
    // Animation is keyed on `duration`; it reads `locked` for the current
    // frame but must not restart when `locked` changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [duration]);

  return (
    <span className="font-mono inline-flex gap-[2px] text-xs">
      {chars.map((char, i) => (
        <span
          key={i}
          className={`inline-block transition-all ${
            locked[i] ? 'text-cyber-text' : 'text-green-500 opacity-80'
          }`}
          style={{
            transitionDuration: `${Math.max(50, duration / 20)}ms`,
            textShadow: locked[i]
              ? '0 0 8px rgba(0, 255, 136, 0.8)'
              : '0 0 4px rgba(0, 255, 0, 0.5)',
          }}
        >
          {char}
        </span>
      ))}
    </span>
  );
};

// ModelCard component
export const ModelCard = React.memo(
  ({
    name,
    type: _type,
    baseUrl,
    anthropicUrl,
    modelId,
    latency,
    protocols = [],
    openaiTested = false,
    anthropicTested = false,
    isPinging = false,
    viewMode = 'config',
    usageData,
    onEdit,
    onDelete,
    onRefresh,
    isRefreshingUsage = false,
    onAccessKey,
    akSkMissing,
    dragHandlePad = false,
  }: ModelCardProps) => {
    // Config tab: resolve icon from the MODEL ID only (platform name must not
    // leak in — otherwise a Qwen platform card configured with model id
    // `glm-5.2` matches the「千问」platform rule and shows qianwen.png instead
    // of glm.svg). The 用量 tab below resolves from baseUrl instead, so the
    // two tabs use two separate icon lookups (model-id vs vendor) by design.
    const iconPath = getModelIcon('', modelId);
    const confirm = useConfirm();
    const { t } = useI18n();

    // Real-time countdown update for usage mode
    const [, setTick] = useState(0);
    useEffect(() => {
      if (viewMode !== 'usage' || !usageData) return;
      const timer = setInterval(() => setTick((prev) => prev + 1), 60000); // Update every minute
      return () => clearInterval(timer);
    }, [viewMode, usageData]);

    return (
      <div className="h-48 p-4 border border-transparent bg-cyber-surface hover:bg-cyber-elevated relative overflow-hidden rounded-card cursor-default transition-colors flex flex-col">
        {/* Action buttons — top right, different for config vs usage mode */}
        {viewMode === 'config' ? (
          // Config mode: [删除] [编辑]
          (onEdit || onDelete) && (
            <div className="absolute top-2 right-2 flex gap-0.5">
              {onDelete && (
                <button
                  className="text-xs font-mono text-cyber-text-muted/70 hover:text-red-500 transition-colors"
                  onClick={async (e) => {
                    e.stopPropagation();
                    const ok = await confirm({
                      title: t('model.deleteTitle'),
                      message: t('model.deleteConfirm'),
                      confirmText: t('btn.delete'),
                      cancelText: t('btn.cancel'),
                      type: 'danger',
                    });
                    if (ok) {
                      onDelete();
                    }
                  }}
                >
                  [{t('btn.delete')}]
                </button>
              )}
              {onEdit && (
                <button
                  className="text-xs font-mono text-cyber-text-muted/70 hover:text-cyber-text transition-colors"
                  onClick={(e) => {
                    e.stopPropagation();
                    onEdit();
                  }}
                >
                  [{t('btn.edit')}]
                </button>
              )}
            </div>
          )
        ) : (
          // Usage mode: [访问权限] [刷新]
          <div className="absolute top-2 right-2 flex gap-1.5">
            {onAccessKey && (
              <button
                className="usage-card-action-button text-xs font-mono"
                onClick={(e) => {
                  e.stopPropagation();
                  onAccessKey();
                }}
              >
                [{t('model.accessKey')}]
              </button>
            )}
            {onRefresh && (
              <button
                className="usage-card-action-button text-xs font-mono"
                disabled={isRefreshingUsage}
                aria-busy={isRefreshingUsage}
                aria-label={t('btn.refresh')}
                onClick={(e) => {
                  e.stopPropagation();
                  if (!isRefreshingUsage) {
                    onRefresh();
                  }
                }}
              >
                <span className={isRefreshingUsage ? 'invisible' : 'visible'}>
                  {t('btn.refresh')}
                </span>
                {isRefreshingUsage && (
                  <span
                    className="absolute inset-0 flex items-center justify-center"
                    aria-hidden="true"
                  >
                    <RefreshCw size={11} className="animate-spin" />
                  </span>
                )}
              </button>
            )}
          </div>
        )}
        <div className={`flex items-center gap-2 ${dragHandlePad ? 'mt-5 mb-4' : 'mb-3'}`}>
          {/* Show provider logo in usage mode (based on baseUrl) */}
          {viewMode === 'usage' &&
            (() => {
              const url = baseUrl || anthropicUrl || '';
              const providerIcon = getModelIcon('', url);
              return providerIcon ? (
                <img
                  src={providerIcon}
                  alt=""
                  className="w-6 h-6 flex-shrink-0"
                  onError={(e) => {
                    (e.target as HTMLImageElement).style.display = 'none';
                  }}
                />
              ) : null;
            })()}
          <div className="text-lg font-bold truncate h-7 flex-1">
            {name || <span className="invisible">-</span>}
          </div>
        </div>

        {/* Content area - switches based on viewMode */}
        {viewMode === 'usage' ? (
          // Usage mode - show quota bars or balance
          <div className="flex-1 flex flex-col justify-center space-y-1">
            {usageData?.quotas && usageData.quotas.length > 0 ? (
              usageData.quotas.map((quota, idx) => (
                <div key={idx} className="space-y-1">
                  {quota.balance !== undefined && quota.balance !== null ? (
                    // Balance display (for providers like DeepSeek) - centered, one line
                    <div className="flex items-center justify-center gap-2">
                      <span className="text-cyber-text font-bold text-2xl">
                        {t('model.balance')}
                      </span>
                      <span className="text-cyber-text font-bold text-2xl">
                        {quota.balance.toFixed(2)} {quota.balanceUnit || 'CNY'}
                      </span>
                    </div>
                  ) : (
                    // Percentage progress bar (for quota-based providers)
                    <>
                      <div className="flex items-center justify-between text-xs">
                        <span className="text-cyber-text font-bold">
                          {`${quota.percentage.toFixed(1)}%`}
                        </span>
                        <span className="text-cyber-text-muted text-[10px] translate-y-[2px]">
                          {formatCountdown(
                            // eslint-disable-next-line react-hooks/purity
                            quota.resetAt - Date.now(),
                            t
                          )}
                        </span>
                      </div>
                      <div className="h-1.5 bg-cyber-border/30 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-gradient-to-r from-cyber-accent to-cyber-accent/70 rounded-full transition-all duration-300"
                          style={{ width: `${quota.percentage}%` }}
                        />
                      </div>
                    </>
                  )}
                </div>
              ))
            ) : (
              <div className="text-center text-cyber-text-muted text-xs">
                {akSkMissing ? t('model.akSkRequired') : t('model.noUsageData')}
              </div>
            )}
          </div>
        ) : (
          // Config mode - show existing info
          <div className="text-xs space-y-1.5 font-mono">
            <div className="flex items-center gap-1 truncate">
              <span className="text-cyber-text/60">{t('model.label')}:</span>
              <span className="truncate text-cyber-text/60">{modelId || '-'}</span>
            </div>
            <div className="flex items-center gap-1 truncate">
              <span className="text-cyber-text/60">{t('model.source')}:</span>
              <span className="truncate text-cyber-text/60">
                {(() => {
                  const url = baseUrl || anthropicUrl;
                  if (!url) return '-';
                  try {
                    return new URL(url).hostname;
                  } catch {
                    return url;
                  }
                })()}
              </span>
            </div>

            <div className="flex items-center gap-1">
              <span className="text-cyber-text/60">{t('model.latency')}:</span>
              {isPinging ? (
                <MatrixDecode />
              ) : latency === -1 ? (
                <span className="text-red-500 font-bold">Error</span>
              ) : latency !== undefined ? (
                <span
                  className={
                    latency < 200
                      ? 'text-green-500'
                      : latency < 500
                        ? 'text-yellow-500'
                        : 'text-red-500'
                  }
                >
                  {latency}ms
                </span>
              ) : (
                <span className="text-cyber-text-muted/70 text-xs">{t('model.notTested')}</span>
              )}
            </div>

            {/* Protocol row */}
            <div className="flex items-center gap-1 truncate">
              <span className="truncate text-cyber-text/60">
                {protocols.includes('openai') && (
                  <span className={openaiTested ? 'text-cyber-text/60' : 'text-cyber-text/30'}>
                    [OpenAI]
                  </span>
                )}
                {protocols.includes('openai') && protocols.includes('anthropic') && ' '}
                {protocols.includes('anthropic') && (
                  <span className={anthropicTested ? 'text-cyber-text/60' : 'text-cyber-text/30'}>
                    [Anthropic]
                  </span>
                )}
                {protocols.length === 0 && '-'}
              </span>
            </div>
          </div>
        )}

        {/* Model icon bottom-right - only show in config mode */}
        {iconPath && viewMode === 'config' && (
          <img
            src={iconPath}
            alt={name}
            className="absolute bottom-3 right-3 w-8 h-8"
            onError={(e) => {
              (e.target as HTMLImageElement).style.display = 'none';
            }}
          />
        )}
      </div>
    );
  },
  (prev, next) => {
    // Custom comparator: skip function props (new refs each render)
    const keys: (keyof ModelCardProps)[] = [
      'id',
      'name',
      'type',
      'baseUrl',
      'anthropicUrl',
      'modelId',
      'latency',
      'openaiTested',
      'anthropicTested',
      'isPinging',
      'viewMode',
      'isRefreshingUsage',
      'akSkMissing',
    ];
    const p = prev as unknown as Record<string, unknown>;
    const n = next as unknown as Record<string, unknown>;
    for (const k of keys) {
      if (p[k] !== n[k]) return false;
    }
    // Compare protocols array by value
    const pp = prev.protocols || [],
      np = next.protocols || [];
    if (pp.length !== np.length || pp.some((v, i) => v !== np[i])) return false;
    // Compare usageData (shallow)
    if (p.usageData !== n.usageData) return false;
    return true;
  }
);
