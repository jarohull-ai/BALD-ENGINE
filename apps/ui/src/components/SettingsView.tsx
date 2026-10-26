import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Bot,
  Box,
  Check,
  Cloud,
  Cpu,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  KeyRound,
  Laptop,
  LoaderCircle,
  LockKeyhole,
  RefreshCw,
  Save,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Wifi,
  Search,
  Compass,
} from 'lucide-react';
import {
  checkScoutModels,
  discoverModels,
  downloadEmbeddedModel,
  fetchEmbeddedModels,
  fetchScoutReport,
  fetchSettings,
  saveSettings,
  startTrialModel,
} from '../api';
import { useI18n } from '../i18n';
import { cn } from '../lib';
import type {
  DiscoveredEngine,
  EmbeddedModelStatusItem,
  ScoutReport,
  SettingsUpdate,
  SettingsView,
} from '../types';
import { InfoTooltip } from './InfoTooltip';
import {
  Badge,
  CardShell,
  KeySourceBadge,
  SectionHeading,
  Spinner,
  Toggle,
  inputClass,
  labelClass,
  selectClass,
} from './ui';

interface Draft {
  language: string;
  searxngEnabled: boolean;
  searxngBaseUrl: string;
  searxngLanguage: string;
  searxngSafesearch: number;
  braveEnabled: boolean;
  braveCountry: string;
  llmProvider: string;
  llmBaseUrl: string;
  llmModel: string;
  embeddedModel: string;
  llmApiKeyEnv: string;
  fallbackProvider: string;
  fallbackBaseUrl: string;
  fallbackModel: string;
  fallbackApiKeyEnv: string;
  timeoutSecs: number;
  connectTimeoutSecs: number;
  maxClaims: number;
  maxInputChars: number;
  temperature: number;
  maxTokens: number;
  scoutEnabled: boolean;
  scoutIntervalDays: number;
}

interface Secrets {
  brave: string;
  llm: string;
  fallback: string;
}

interface Clears {
  brave: boolean;
  llm: boolean;
  fallback: boolean;
}

const emptySecrets: Secrets = { brave: '', llm: '', fallback: '' };
const emptyClears: Clears = { brave: false, llm: false, fallback: false };

function buildDraft(view: SettingsView): Draft {
  return {
    language: view.language,
    searxngEnabled: view.searxng.enabled,
    searxngBaseUrl: view.searxng.baseUrl,
    searxngLanguage: view.searxng.language ?? '',
    searxngSafesearch: view.searxng.safesearch,
    braveEnabled: view.brave.enabled,
    braveCountry: view.brave.country ?? '',
    llmProvider: view.llm.provider,
    llmBaseUrl: view.llm.baseUrl,
    llmModel: view.llm.model,
    embeddedModel: view.llm.embeddedModel || 'qwen2.5-0.5b-instruct',
    llmApiKeyEnv: view.llm.apiKeyEnv,
    fallbackProvider: view.llm.fallbackProvider,
    fallbackBaseUrl: view.llm.fallbackBaseUrl,
    fallbackModel: view.llm.fallbackModel,
    fallbackApiKeyEnv: view.llm.fallbackApiKeyEnv,
    timeoutSecs: view.llm.timeoutSecs,
    connectTimeoutSecs: view.llm.connectTimeoutSecs,
    maxClaims: view.llm.maxClaims,
    maxInputChars: view.llm.maxInputChars,
    temperature: view.llm.temperature,
    maxTokens: view.llm.maxTokens,
    scoutEnabled: view.scout?.enabled ?? true,
    scoutIntervalDays: view.scout?.intervalDays ?? 7,
  };
}

function toUpdate(draft: Draft, secrets: Secrets, clears: Clears): SettingsUpdate {
  return {
    language: draft.language,
    searxngEnabled: draft.searxngEnabled,
    searxngBaseUrl: draft.searxngBaseUrl,
    searxngLanguage: draft.searxngLanguage,
    searxngSafesearch: draft.searxngSafesearch,
    braveEnabled: draft.braveEnabled,
    braveCountry: draft.braveCountry,
    braveApiKey: secrets.brave.trim() || undefined,
    clearBraveApiKey: clears.brave || undefined,
    llmProvider: draft.llmProvider,
    llmBaseUrl: draft.llmBaseUrl,
    llmModel: draft.llmModel,
    embeddedModel: draft.embeddedModel,
    llmApiKeyEnv: draft.llmApiKeyEnv,
    llmApiKey: secrets.llm.trim() || undefined,
    clearLlmApiKey: clears.llm || undefined,
    fallbackProvider: draft.fallbackProvider,
    fallbackBaseUrl: draft.fallbackBaseUrl,
    fallbackModel: draft.fallbackModel,
    fallbackApiKeyEnv: draft.fallbackApiKeyEnv,
    fallbackApiKey: secrets.fallback.trim() || undefined,
    clearFallbackApiKey: clears.fallback || undefined,
    timeoutSecs: draft.timeoutSecs,
    connectTimeoutSecs: draft.connectTimeoutSecs,
    temperature: draft.temperature,
    maxTokens: draft.maxTokens,
    maxInputChars: draft.maxInputChars,
    maxClaims: draft.maxClaims,
    scoutEnabled: draft.scoutEnabled,
    scoutIntervalDays: draft.scoutIntervalDays,
  };
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className={labelClass}>{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-[10px] leading-relaxed text-slate-600">{hint}</span>}
    </label>
  );
}

function KeyField({
  label,
  value,
  onChange,
  configured,
  source,
  clearPending,
  onClear,
  show,
  onToggleShow,
  disabled,
  optional,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  configured: boolean;
  source: string;
  clearPending: boolean;
  onClear: () => void;
  show: boolean;
  onToggleShow: () => void;
  disabled?: boolean;
  optional?: boolean;
}) {
  const { t } = useI18n();
  const fromEnv = source === 'env';
  const canClear = configured && source === 'config' && !fromEnv;

  return (
    <div>
      <div className="mb-1.5 flex flex-wrap items-center justify-between gap-2">
        <span className={cn(labelClass, 'mb-0')}>
          {label}
          {optional && (
            <span className="ml-1.5 font-normal normal-case tracking-normal text-slate-700">
              {t('settings.optional')}
            </span>
          )}
        </span>
        <KeySourceBadge source={source} />
      </div>
      <div className="flex gap-2">
        <div className="relative min-w-0 flex-1">
          <KeyRound className="pointer-events-none absolute left-3.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-slate-600" />
          <input
            type={show ? 'text' : 'password'}
            value={value}
            disabled={disabled || fromEnv}
            onChange={(event) => onChange(event.target.value)}
            placeholder={
              configured
                ? clearPending
                  ? '—'
                  : '••••••••••••••••'
                : t('settings.apiKeyPlaceholder')
            }
            autoComplete="off"
            spellCheck={false}
            className={cn(inputClass, 'pl-10 pr-10 font-mono text-xs')}
          />
          <button
            type="button"
            onClick={onToggleShow}
            disabled={disabled || fromEnv}
            aria-label={show ? 'Ukryj klucz' : 'Pokaż klucz'}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-slate-600 transition hover:bg-white/5 hover:text-slate-300 disabled:opacity-30"
          >
            {show ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </button>
        </div>
        {canClear && (
          <button
            type="button"
            onClick={onClear}
            className={cn(
              'inline-flex shrink-0 items-center gap-1.5 rounded-xl border px-3 text-[10px] font-semibold transition',
              clearPending
                ? 'border-amber-300/30 bg-amber-400/10 text-amber-200'
                : 'border-white/10 text-slate-500 hover:border-rose-300/25 hover:text-rose-200',
            )}
          >
            <Trash2 className="h-3 w-3" />
            {clearPending ? '—' : t('settings.clearKey')}
          </button>
        )}
      </div>
      {fromEnv && (
        <p className="mt-2 text-[10px] leading-relaxed text-cyan-300/50">
          {t('settings.envNote')}
        </p>
      )}
      {clearPending && (
        <p className="mt-2 text-[10px] text-amber-300/70">{t('settings.unsaved')}</p>
      )}
    </div>
  );
}

function PresetCard({
  icon,
  title,
  body,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  body: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group rounded-xl border border-white/[0.08] bg-white/[0.02] p-3.5 text-left transition hover:border-amber-300/25 hover:bg-amber-400/[0.035]"
    >
      <span className="flex items-center gap-2 text-xs font-semibold text-slate-300 group-hover:text-amber-100">
        {icon}
        {title}
      </span>
      <span className="mt-1.5 block text-[10px] leading-relaxed text-slate-600">{body}</span>
    </button>
  );
}

function LoadingSettings() {
  return (
    <div className="space-y-4">
      {Array.from({ length: 3 }).map((_, index) => (
        <div
          key={index}
          className="animate-pulse rounded-2xl border border-white/[0.06] bg-white/[0.02] p-6"
        >
          <div className="h-4 w-40 rounded bg-white/[0.06]" />
          <div className="mt-4 grid gap-3 sm:grid-cols-2">
            <div className="h-10 rounded-xl bg-white/[0.04]" />
            <div className="h-10 rounded-xl bg-white/[0.04]" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function SettingsView() {
  const { t } = useI18n();
  const [view, setView] = useState<SettingsView | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [secrets, setSecrets] = useState<Secrets>(emptySecrets);
  const [clears, setClears] = useState<Clears>(emptyClears);
  const [showBrave, setShowBrave] = useState(false);
  const [showLlm, setShowLlm] = useState(false);
  const [showFallback, setShowFallback] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [discoveredEngines, setDiscoveredEngines] = useState<DiscoveredEngine[]>([]);
  const [discovering, setDiscovering] = useState(false);
  const [discoveryScanned, setDiscoveryScanned] = useState(false);
  const [selectionNotice, setSelectionNotice] = useState<string | null>(null);
  const [embeddedModels, setEmbeddedModels] = useState<EmbeddedModelStatusItem[]>([]);
  const [downloadingModelId, setDownloadingModelId] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [scoutReport, setScoutReport] = useState<ScoutReport | null>(null);
  const [checkingScout, setCheckingScout] = useState(false);
  const [trialStartingId, setTrialStartingId] = useState<string | null>(null);
  const [scoutNotice, setScoutNotice] = useState<string | null>(null);

  const loadScout = useCallback(async () => {
    try {
      const rep = await fetchScoutReport();
      setScoutReport(rep);
    } catch {
      setScoutReport(null);
    }
  }, []);

  useEffect(() => {
    void loadScout();
  }, [loadScout]);

  const handleCheckScout = async () => {
    setCheckingScout(true);
    setScoutNotice(null);
    try {
      const rep = await checkScoutModels();
      setScoutReport(rep);
      setScoutNotice(`Szperacz zakończył skanowanie. Liczba dostępnych modeli: ${rep.candidates.length}`);
      window.setTimeout(() => setScoutNotice(null), 4000);
    } catch (err) {
      setDownloadError(err instanceof Error ? err.message : String(err));
    } finally {
      setCheckingScout(false);
    }
  };

  const handleStartTrial = async (modelId: string) => {
    setTrialStartingId(modelId);
    setScoutNotice(null);
    try {
      const res = await startTrialModel(modelId);
      setScoutNotice(res.message);
      await loadScout();
      await loadEmbedded();
      update('llmProvider', 'embedded');
      update('embeddedModel', modelId);
      window.setTimeout(() => setScoutNotice(null), 4000);
    } catch (err) {
      setDownloadError(err instanceof Error ? err.message : String(err));
    } finally {
      setTrialStartingId(null);
    }
  };

  const loadEmbedded = useCallback(async () => {
    try {
      const resp = await fetchEmbeddedModels();
      setEmbeddedModels(resp.models);
    } catch {
      setEmbeddedModels([]);
    }
  }, []);

  useEffect(() => {
    void loadEmbedded();
  }, [loadEmbedded]);

  async function handleDownloadEmbedded(modelId: string) {
    setDownloadingModelId(modelId);
    setDownloadError(null);
    try {
      await downloadEmbeddedModel(modelId);
      await loadEmbedded();
      update('llmProvider', 'embedded');
      update('embeddedModel', modelId);
      setSelectionNotice(t('settings.selectedModel', { model: modelId }));
      window.setTimeout(() => setSelectionNotice(null), 3500);
    } catch (err) {
      setDownloadError(err instanceof Error ? err.message : String(err));
    } finally {
      setDownloadingModelId(null);
    }
  }

  const handleDiscover = useCallback(async () => {
    setDiscovering(true);
    try {
      const resp = await discoverModels();
      setDiscoveredEngines(resp.engines);
    } catch {
      setDiscoveredEngines([]);
    } finally {
      setDiscoveryScanned(true);
      setDiscovering(false);
    }
  }, []);

  useEffect(() => {
    void handleDiscover();
  }, [handleDiscover]);

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const next = await fetchSettings();
      setView(next);
      setDraft(buildDraft(next));
      setSecrets(emptySecrets);
      setClears(emptyClears);
      setSaveError(null);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const initial = useMemo(() => (view ? buildDraft(view) : null), [view]);
  const dirty = useMemo(() => {
    if (!draft || !initial) return false;
    if (JSON.stringify(draft) !== JSON.stringify(initial)) return true;
    if (Object.values(secrets).some((value) => value.trim() !== '')) return true;
    if (Object.values(clears).some(Boolean)) return true;
    return false;
  }, [clears, draft, initial, secrets]);

  function update<K extends keyof Draft>(key: K, value: Draft[K]) {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
  }

  function updateSecret(key: keyof Secrets, value: string) {
    setSecrets((current) => ({ ...current, [key]: value }));
    if (value.trim()) {
      setClears((current) => ({ ...current, [key]: false }));
    }
  }

  function requestClear(key: keyof Clears) {
    setClears((current) => ({ ...current, [key]: true }));
    setSecrets((current) => ({ ...current, [key]: '' }));
  }

  function applyPreset(preset: 'embedded' | 'local' | 'cloud' | 'rules') {
    setDraft((current) => {
      if (!current) return current;
      if (preset === 'embedded') {
        return {
          ...current,
          searxngEnabled: true,
          searxngBaseUrl: 'http://127.0.0.1:8080',
          searxngLanguage: '',
          braveEnabled: false,
          llmProvider: 'embedded',
          embeddedModel: 'qwen2.5-0.5b-instruct',
          fallbackProvider: 'heuristic',
        };
      }
      if (preset === 'local') {
        return {
          ...current,
          searxngEnabled: true,
          searxngBaseUrl: 'http://127.0.0.1:8080',
          searxngLanguage: '',
          braveEnabled: false,
          llmProvider: 'ollama',
          llmBaseUrl: 'http://127.0.0.1:11434',
          llmModel: 'llama3.1:8b',
          fallbackProvider: 'heuristic',
        };
      }
      if (preset === 'cloud') {
        return {
          ...current,
          searxngEnabled: false,
          braveEnabled: true,
          llmProvider: 'openai',
          llmBaseUrl: 'https://api.openai.com/v1',
          llmModel: 'gpt-4o-mini',
          fallbackProvider: 'heuristic',
        };
      }
      return {
        ...current,
        llmProvider: 'none',
        fallbackProvider: 'none',
      };
    });
  }

  async function save() {
    if (!draft) return;
    setSaving(true);
    setSaveError(null);
    try {
      const next = await saveSettings(toUpdate(draft, secrets, clears));
      setView(next);
      setDraft(buildDraft(next));
      setSecrets(emptySecrets);
      setClears(emptyClears);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2400);
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  if (loading && !draft) {
    return (
      <div className="space-y-6">
        <div>
          <div className="h-3 w-32 rounded bg-white/[0.04]" />
          <div className="mt-3 h-9 w-80 max-w-full rounded bg-white/[0.05]" />
        </div>
        <LoadingSettings />
      </div>
    );
  }

  if (!draft || !view) {
    return (
      <div className="rounded-2xl border border-rose-400/20 bg-rose-400/[0.05] p-6">
        <div className="flex items-center gap-2 text-sm font-semibold text-rose-200">
          <AlertTriangle className="h-4 w-4" />
          {t('error.title')}
        </div>
        <p className="mt-2 break-words font-mono text-[11px] text-rose-300/60">{loadError}</p>
        <button
          type="button"
          onClick={() => void load()}
          className="mt-4 inline-flex items-center gap-2 rounded-lg border border-rose-300/20 px-3 py-2 text-xs font-semibold text-rose-200 transition hover:bg-rose-400/10"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t('error.retry')}
        </button>
      </div>
    );
  }

  const primaryNeedsEndpoint =
    draft.llmProvider === 'ollama' || draft.llmProvider === 'openai';
  const fallbackNeedsEndpoint = draft.fallbackProvider === 'ollama' || draft.fallbackProvider === 'openai';

  return (
    <div className="space-y-6 pb-24">
      <div className="grid gap-6 lg:grid-cols-[1fr_auto] lg:items-end">
        <div className="animate-rise">
          <div className="mb-3 flex items-center gap-2">
            <span className="h-px w-6 bg-amber-400/50" />
            <span className="text-[10px] font-bold uppercase tracking-[0.22em] text-amber-300/80">
              {t('settings.eyebrow')}
            </span>
          </div>
          <h1 className="text-3xl font-semibold tracking-[-0.03em] text-slate-100 sm:text-4xl">
            {t('settings.title')}
          </h1>
          <p className="mt-3 max-w-2xl text-sm leading-relaxed text-slate-500">
            {t('settings.lede')}
          </p>
        </div>
        <button
          type="button"
          onClick={() => void load()}
          disabled={loading}
          className="inline-flex w-fit items-center gap-2 rounded-xl border border-white/10 bg-white/[0.035] px-4 py-2.5 text-xs font-semibold text-slate-300 transition hover:border-white/20 hover:bg-white/[0.06] disabled:opacity-50"
        >
          {loading ? <Spinner /> : <RefreshCw className="h-3.5 w-3.5" />}
          {t('settings.reload')}
        </button>
      </div>

      <CardShell className="p-4 sm:p-5">
        <div className="mb-3 flex items-center gap-2">
          <Sparkles className="h-3.5 w-3.5 text-amber-300/70" />
          <span className="text-[10px] font-bold uppercase tracking-[0.18em] text-slate-500">
            {t('settings.presets')}
          </span>
        </div>
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-5">
          <PresetCard
            icon={<Box className="h-3.5 w-3.5 text-amber-300" />}
            title={t('settings.preset.embedded')}
            body={t('settings.preset.embeddedBody')}
            onClick={() => applyPreset('embedded')}
          />
          <PresetCard
            icon={<Laptop className="h-3.5 w-3.5 text-emerald-300/80" />}
            title={t('settings.preset.local')}
            body={t('settings.preset.localBody')}
            onClick={() => applyPreset('local')}
          />
          <PresetCard
            icon={<Cloud className="h-3.5 w-3.5 text-cyan-300/80" />}
            title={t('settings.preset.cloud')}
            body={t('settings.preset.cloudBody')}
            onClick={() => applyPreset('cloud')}
          />
          <PresetCard
            icon={<Cpu className="h-3.5 w-3.5 text-violet-300/80" />}
            title={t('settings.preset.rules')}
            body={t('settings.preset.rulesBody')}
            onClick={() => applyPreset('rules')}
          />
          <PresetCard
            icon={<SlidersHorizontal className="h-3.5 w-3.5 text-slate-400" />}
            title={t('settings.preset.custom')}
            body={t('settings.preset.customBody')}
            onClick={() => undefined}
          />
        </div>
      </CardShell>

      <CardShell className="p-5 sm:p-6">
        <SectionHeading
          eyebrow="01 · search"
          title={t('settings.providers')}
          body={t('settings.providersBody')}
        />

        <div className="mt-6 grid gap-4 xl:grid-cols-2">
          <div className="rounded-2xl border border-white/[0.07] bg-black/15 p-4">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-white/[0.08] bg-white/[0.03]">
                  <Server className="h-4 w-4 text-emerald-300/80" />
                </span>
                <div>
                  <h3 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                    {t('settings.searxng')}
                    <Badge tone="emerald">local</Badge>
                  </h3>
                  <p className="mt-1 text-[11px] leading-relaxed text-slate-600">
                    {t('settings.searxngBody')}
                  </p>
                </div>
              </div>
              <Toggle
                checked={draft.searxngEnabled}
                onChange={(value) => update('searxngEnabled', value)}
                label={t('settings.enabled')}
              />
            </div>

            <div className="mt-4 space-y-3">
              <Field label={t('settings.baseUrl')}>
                <input
                  value={draft.searxngBaseUrl}
                  onChange={(event) => update('searxngBaseUrl', event.target.value)}
                  disabled={!draft.searxngEnabled}
                  spellCheck={false}
                  className={cn(inputClass, 'font-mono text-xs')}
                />
              </Field>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t('settings.language')}>
                  <select
                    value={draft.searxngLanguage}
                    onChange={(event) => update('searxngLanguage', event.target.value)}
                    disabled={!draft.searxngEnabled}
                    className={selectClass}
                  >
                    <option value="">{t('settings.anyLanguage')}</option>
                    <option value="pl">Polski</option>
                    <option value="en">English</option>
                  </select>
                </Field>
                <Field label={t('settings.safesearch')}>
                  <select
                    value={draft.searxngSafesearch}
                    onChange={(event) =>
                      update('searxngSafesearch', Number(event.target.value))
                    }
                    disabled={!draft.searxngEnabled}
                    className={selectClass}
                  >
                    <option value={0}>{t('settings.safesearch.off')}</option>
                    <option value={1}>{t('settings.safesearch.moderate')}</option>
                    <option value={2}>{t('settings.safesearch.strict')}</option>
                  </select>
                </Field>
              </div>
            </div>
          </div>

          <div className="rounded-2xl border border-white/[0.07] bg-black/15 p-4">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-white/[0.08] bg-white/[0.03]">
                  <Cloud className="h-4 w-4 text-cyan-300/80" />
                </span>
                <div>
                  <h3 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                    {t('settings.brave')}
                    <Badge tone="cyan">cloud</Badge>
                  </h3>
                  <p className="mt-1 text-[11px] leading-relaxed text-slate-600">
                    {t('settings.braveBody')}
                  </p>
                </div>
              </div>
              <Toggle
                checked={draft.braveEnabled}
                onChange={(value) => update('braveEnabled', value)}
                label={t('settings.enabled')}
              />
            </div>

            <div className="mt-4 space-y-3">
              <Field label={t('settings.country')} hint={t('settings.countryPlaceholder')}>
                <input
                  value={draft.braveCountry}
                  onChange={(event) => update('braveCountry', event.target.value)}
                  disabled={!draft.braveEnabled}
                  maxLength={2}
                  spellCheck={false}
                  className={cn(inputClass, 'font-mono text-xs uppercase')}
                />
              </Field>
              <KeyField
                label={t('settings.apiKey')}
                value={secrets.brave}
                onChange={(value) => updateSecret('brave', value)}
                configured={view.brave.apiKeyConfigured}
                source={view.brave.apiKeySource}
                clearPending={clears.brave}
                onClear={() => requestClear('brave')}
                show={showBrave}
                onToggleShow={() => setShowBrave((value) => !value)}
                disabled={!draft.braveEnabled}
              />
              <div className="pt-1 text-right">
                <a
                  href="https://brave.com/search/api/"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1.5 text-[11px] font-medium text-cyan-400/80 transition-colors hover:text-cyan-300"
                >
                  <ExternalLink className="h-3 w-3" />
                  {t('settings.braveKeyLink')}
                </a>
              </div>
            </div>
          </div>
        </div>
      </CardShell>

      <CardShell className="p-5 sm:p-6">
        <SectionHeading
          eyebrow="02 · extract"
          title={t('settings.llm')}
          body={t('settings.llmBody')}
          action={
            <Badge
              tone={
                draft.llmProvider === 'embedded'
                  ? 'amber'
                  : draft.llmProvider === 'ollama'
                    ? 'emerald'
                    : 'neutral'
              }
            >
              <Bot className="h-3 w-3" />
              {draft.llmProvider === 'embedded'
                ? t('settings.embedded')
                : draft.llmProvider === 'ollama'
                  ? t('settings.ollama')
                  : draft.llmProvider === 'openai'
                    ? t('settings.openai')
                    : t('settings.none')}
            </Badge>
          }
        />

        {/* Wbudowany silnik AI (Candle / Rust - Zero setup) */}
        <div className="mt-6 rounded-2xl border border-amber-400/25 bg-amber-400/[0.04] p-4 sm:p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2.5">
              <span className="grid h-8 w-8 place-items-center rounded-xl border border-amber-400/30 bg-amber-400/15 text-amber-300">
                <Box className="h-4 w-4" />
              </span>
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-200">
                    {t('settings.embeddedEngineTitle')}
                  </h3>
                  <span className="rounded-full bg-amber-400/15 px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider text-amber-300 border border-amber-400/25">
                    Zero Setup · Pure Rust
                  </span>
                </div>
                <p className="mt-0.5 text-[11px] text-slate-400">
                  {t('settings.embeddedEngineSubtitle')}
                </p>
              </div>
            </div>

            <button
              type="button"
              onClick={() => void loadEmbedded()}
              className="inline-flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-1 text-[11px] font-medium text-slate-300 transition hover:bg-white/[0.06]"
            >
              <RefreshCw className="h-3 w-3 text-slate-400" />
              {t('settings.reload')}
            </button>
          </div>

          {downloadError && (
            <div className="mt-3 rounded-lg border border-rose-400/20 bg-rose-400/10 p-2.5 text-xs text-rose-300">
              <AlertTriangle className="mr-1.5 inline h-3.5 w-3.5" />
              {downloadError}
            </div>
          )}

          <div className="mt-4 grid gap-3 sm:grid-cols-2">
            {embeddedModels.map((m) => {
              const isSelected = draft.llmProvider === 'embedded' && draft.embeddedModel === m.id;
              const isDownloading = downloadingModelId === m.id;
              const sizeMb = Math.round(m.size_bytes / (1024 * 1024));

              return (
                <div
                  key={m.id}
                  className={cn(
                    'relative rounded-xl border p-4 transition',
                    isSelected
                      ? 'border-amber-400/50 bg-amber-400/[0.08] shadow-sm'
                      : 'border-white/[0.08] bg-black/20 hover:border-white/15'
                  )}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-semibold text-slate-100">{m.name}</span>
                        {m.installed ? (
                          <span className="inline-flex items-center gap-1 rounded border border-emerald-400/20 bg-emerald-400/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-300">
                            <Check className="h-2.5 w-2.5" />
                            {t('settings.embeddedInstalled')}
                          </span>
                        ) : (
                          <span className="rounded bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-slate-400">
                            ~{sizeMb} MB
                          </span>
                        )}
                      </div>
                      <p className="mt-1 text-[11px] leading-relaxed text-slate-400">{m.description}</p>
                    </div>
                  </div>

                  <div className="mt-3 flex items-center justify-between gap-2 border-t border-white/[0.06] pt-2">
                    <span className="font-mono text-[10px] text-slate-500">ID: {m.id}</span>

                    {m.installed ? (
                      isSelected ? (
                        <span className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/30 bg-amber-400/10 px-3 py-1 text-xs font-semibold text-amber-200">
                          <Check className="h-3 w-3" />
                          {t('settings.embeddedActive')}
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={() => {
                            update('llmProvider', 'embedded');
                            update('embeddedModel', m.id);
                            setSelectionNotice(t('settings.selectedModel', { model: m.name }));
                            window.setTimeout(() => setSelectionNotice(null), 3500);
                          }}
                          className="inline-flex items-center gap-1 rounded-lg border border-white/15 bg-white/[0.06] px-3 py-1 text-xs font-medium text-slate-200 transition hover:border-amber-400/40 hover:bg-amber-400/10 hover:text-amber-200"
                        >
                          {t('settings.embeddedActivate')}
                        </button>
                      )
                    ) : (
                      <button
                        type="button"
                        onClick={() => void handleDownloadEmbedded(m.id)}
                        disabled={isDownloading || Boolean(downloadingModelId)}
                        className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/40 bg-amber-400/15 px-3 py-1 text-xs font-semibold text-amber-200 transition hover:bg-amber-400/25 disabled:opacity-50"
                      >
                        {isDownloading ? (
                          <>
                            <Spinner />
                            <span>Pobieranie (~{sizeMb} MB)…</span>
                          </>
                        ) : (
                          <>
                            <Download className="h-3 w-3" />
                            <span>{t('settings.embeddedDownload')} ({sizeMb} MB)</span>
                          </>
                        )}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Szperacz Modeli AI (Scout Bot) & Piaskownica A/B (Jazda Próbna) */}
        <div className="mt-6 rounded-2xl border border-amber-400/25 bg-amber-400/[0.04] p-4 sm:p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2.5">
              <span className="grid h-8 w-8 place-items-center rounded-xl border border-amber-400/30 bg-amber-400/15 text-amber-300">
                <Compass className="h-4 w-4" />
              </span>
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-200">
                    {t('settings.scoutTitle')}
                  </h3>
                  <InfoTooltip
                    title="Autonomiczny Szperacz Modeli AI"
                    content="Cykliczny bot sprawdzający repozytoria (Hugging Face) pod kątem nowszych, zoptymalizowanych modeli <3B z wysoką kulturą polszczyzny i niskim zużyciem RAM."
                  />
                  {scoutReport?.in_trial && (
                    <span className="rounded-full bg-amber-500/20 px-2 py-0.5 text-[9px] font-bold text-amber-300 border border-amber-500/30">
                      Tryb Jazdy Próbnej aktywny
                    </span>
                  )}
                </div>
                <p className="mt-0.5 text-[11px] text-slate-400">
                  {t('settings.scoutSubtitle')}
                </p>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => void handleCheckScout()}
                disabled={checkingScout}
                className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/30 bg-amber-400/10 px-3 py-1.5 text-xs font-semibold text-amber-200 transition hover:bg-amber-400/20 disabled:opacity-50"
              >
                {checkingScout ? (
                  <>
                    <Spinner />
                    <span>{t('settings.scoutChecking')}</span>
                  </>
                ) : (
                  <>
                    <Search className="h-3.5 w-3.5" />
                    <span>{t('settings.scoutCheckNow')}</span>
                  </>
                )}
              </button>
            </div>
          </div>

          {/* Scout settings bar: periodic scan interval */}
          <div className="mt-4 flex flex-wrap items-center justify-between gap-4 rounded-xl border border-white/[0.08] bg-black/30 p-3 text-xs">
            <div className="flex items-center gap-3">
              <Toggle
                label={t('settings.scoutEnabled')}
                checked={draft.scoutEnabled}
                onChange={(val) => update('scoutEnabled', val)}
              />
              <InfoTooltip
                content="Gdy ta opcja jest włączona, BALD w tle cyklicznie sprawdza czy pojawił się nowy, certyfikowany model o wyższych parametrach."
              />
            </div>

            <div className="flex items-center gap-2">
              <span className="text-[11px] text-slate-400">{t('settings.scoutInterval')}:</span>
              <select
                value={draft.scoutIntervalDays}
                onChange={(e) => update('scoutIntervalDays', Number(e.target.value))}
                className="rounded-lg border border-white/10 bg-neutral-900 px-2.5 py-1 text-xs text-slate-200 focus:outline-none focus:ring-1 focus:ring-amber-500/50"
              >
                <option value={7}>Co 7 dni (Zalecane)</option>
                <option value={14}>Co 14 dni</option>
                <option value={30}>Co 30 dni</option>
              </select>
            </div>
          </div>

          {scoutNotice && (
            <div className="mt-3 rounded-lg border border-emerald-400/20 bg-emerald-400/10 p-2.5 text-xs text-emerald-300 flex items-center gap-2">
              <Check className="h-3.5 w-3.5" />
              <span>{scoutNotice}</span>
            </div>
          )}

          {/* Candidates list */}
          <div className="mt-4 space-y-3">
            <div className="flex items-center justify-between">
              <h4 className="text-xs font-semibold text-slate-300">
                {t('settings.scoutCandidatesTitle')}
              </h4>
              {scoutReport?.last_checked && (
                <span className="text-[10px] text-slate-500">
                  Ostatnie skanowanie: {scoutReport.last_checked}
                </span>
              )}
            </div>

            <div className="grid gap-3 sm:grid-cols-2">
              {(scoutReport?.candidates ?? []).map((cand) => {
                const isCurrent = cand.is_current;
                const isTrial = cand.is_trial;
                const isStarting = trialStartingId === cand.id;

                return (
                  <div
                    key={cand.id}
                    className={cn(
                      'relative rounded-xl border p-4 transition flex flex-col justify-between',
                      isTrial
                        ? 'border-amber-400/50 bg-amber-400/[0.08] shadow-sm'
                        : isCurrent
                        ? 'border-emerald-400/40 bg-emerald-400/[0.04]'
                        : 'border-white/[0.08] bg-black/20 hover:border-white/15',
                    )}
                  >
                    <div>
                      <div className="flex items-start justify-between gap-2">
                        <div>
                          <div className="flex items-center gap-1.5 flex-wrap">
                            <span className="text-xs font-semibold text-slate-100">{cand.name}</span>
                            <span className="rounded bg-amber-400/15 border border-amber-400/30 px-1.5 py-0.5 text-[9px] font-bold text-amber-300">
                              {cand.param_size}
                            </span>
                            <span className="rounded bg-white/[0.06] px-1.5 py-0.5 text-[9px] text-slate-400">
                              ~{cand.size_mb} MB
                            </span>
                          </div>
                          <span className="text-[10px] font-mono text-slate-500">{cand.architecture}</span>
                        </div>
                        <span className="rounded-full bg-emerald-500/15 border border-emerald-500/25 px-2 py-0.5 text-[10px] font-bold text-emerald-300">
                          {cand.quality_score}/100 pkt
                        </span>
                      </div>

                      <p className="mt-2 text-[11px] leading-relaxed text-slate-400">{cand.summary}</p>

                      <div className="mt-2.5 flex flex-wrap gap-1">
                        {cand.highlights.map((h, hIdx) => (
                          <span
                            key={hIdx}
                            className="rounded bg-white/[0.03] border border-white/5 px-2 py-0.5 text-[9px] text-slate-300"
                          >
                            ✓ {h}
                          </span>
                        ))}
                      </div>
                    </div>

                    <div className="mt-4 flex items-center justify-between gap-2 border-t border-white/[0.06] pt-2.5">
                      <span className="font-mono text-[9px] text-slate-500">
                        {cand.installed ? 'Pobrany lokalnie' : 'Dostępny online'}
                      </span>

                      {isTrial ? (
                        <span className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/30 bg-amber-400/15 px-3 py-1 text-xs font-semibold text-amber-200">
                          <Sparkles className="h-3 w-3 text-amber-400" />
                          Trwa jazda próbna w czacie
                        </span>
                      ) : isCurrent ? (
                        <span className="inline-flex items-center gap-1.5 rounded-lg border border-emerald-400/30 bg-emerald-400/15 px-3 py-1 text-xs font-semibold text-emerald-200">
                          <Check className="h-3 w-3 text-emerald-400" />
                          Model domyślny
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={() => void handleStartTrial(cand.id)}
                          disabled={Boolean(trialStartingId)}
                          className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/40 bg-amber-400/15 px-3 py-1 text-xs font-semibold text-amber-200 transition hover:bg-amber-400/25 disabled:opacity-50"
                        >
                          {isStarting ? (
                            <>
                              <Spinner />
                              <span>{t('settings.trialStarting')}</span>
                            </>
                          ) : (
                            <>
                              <Sparkles className="h-3 w-3" />
                              <span>{t('settings.trialStart')}</span>
                            </>
                          )}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </div>

        {/* Auto-Discovery of Local AI Engines */}
        <div className="mt-6 rounded-2xl border border-white/[0.08] bg-black/25 p-4 sm:p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2.5">
              <span className="grid h-8 w-8 place-items-center rounded-xl border border-amber-400/20 bg-amber-400/10 text-amber-300">
                <Cpu className="h-4 w-4" />
              </span>
              <div>
                <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-200">
                  {t('settings.discoveredTitle')}
                </h3>
                <p className="text-[11px] text-slate-500">
                  {t('settings.discoveredHint')}
                </p>
              </div>
            </div>

            <button
              type="button"
              onClick={() => void handleDiscover()}
              disabled={discovering}
              className="inline-flex items-center gap-2 rounded-xl border border-white/10 bg-white/[0.04] px-3.5 py-1.5 text-xs font-semibold text-slate-200 transition hover:border-amber-400/30 hover:bg-white/[0.07] disabled:opacity-50"
            >
              {discovering ? <Spinner /> : <RefreshCw className="h-3.5 w-3.5 text-amber-300" />}
              {discovering ? t('settings.discovering') : t('settings.discoverModels')}
            </button>
          </div>

          {selectionNotice && (
            <div className="mt-3 inline-flex items-center gap-2 rounded-lg border border-emerald-400/20 bg-emerald-400/10 px-3 py-1.5 text-xs font-medium text-emerald-300">
              <Check className="h-3.5 w-3.5" />
              {selectionNotice}
            </div>
          )}

          <div className="mt-4">
            {discoveredEngines.filter((e) => e.available).length > 0 ? (
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {discoveredEngines
                  .filter((e) => e.available)
                  .map((engine) => (
                    <div
                      key={engine.id}
                      className="rounded-xl border border-emerald-400/20 bg-emerald-400/[0.04] p-3.5"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                          <span className="relative flex h-2 w-2">
                            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                            <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-500" />
                          </span>
                          <span className="text-xs font-bold text-slate-200">{engine.name}</span>
                        </div>
                        <span className="font-mono text-[10px] text-slate-500">{engine.baseUrl}</span>
                      </div>

                      <div className="mt-2.5 flex flex-wrap gap-1.5">
                        {engine.models.length > 0 ? (
                          engine.models.map((model) => {
                            const isCurrent =
                              draft.llmBaseUrl === engine.baseUrl && draft.llmModel === model;
                            return (
                              <button
                                key={model}
                                type="button"
                                onClick={() => {
                                  update(
                                    'llmProvider',
                                    engine.providerType === 'ollama' ? 'ollama' : 'openai'
                                  );
                                  update('llmBaseUrl', engine.baseUrl);
                                  update('llmModel', model);
                                  setSelectionNotice(t('settings.selectedModel', { model }));
                                  window.setTimeout(() => setSelectionNotice(null), 3500);
                                }}
                                className={cn(
                                  'rounded-lg border px-2 py-1 font-mono text-[11px] transition',
                                  isCurrent
                                    ? 'border-amber-400/50 bg-amber-400/15 font-semibold text-amber-200 shadow-sm'
                                    : 'border-white/[0.08] bg-white/[0.03] text-slate-300 hover:border-emerald-400/40 hover:bg-emerald-400/10 hover:text-emerald-200'
                                )}
                              >
                                {model}
                                {isCurrent && ' ✓'}
                              </button>
                            );
                          })
                        ) : (
                          <span className="text-[11px] text-slate-600">Brak pobranych modeli</span>
                        )}
                      </div>
                    </div>
                  ))}
              </div>
            ) : discoveryScanned ? (
              <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-slate-500">
                <span>{t('settings.noModelsFound')}</span>
                <a
                  href="https://ollama.com"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 font-medium text-emerald-400/80 hover:text-emerald-300"
                >
                  <ExternalLink className="h-3 w-3" />
                  {t('settings.ollamaDownloadLink')}
                </a>
              </div>
            ) : null}
          </div>
        </div>

        <div className="mt-6 grid gap-4 xl:grid-cols-2">
          <div className="rounded-2xl border border-white/[0.07] bg-black/15 p-4">
            <div className="flex items-center gap-2">
              <Wifi className="h-3.5 w-3.5 text-amber-300/70" />
              <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-400">
                Primary
              </h3>
            </div>
            <div className="mt-4 space-y-3">
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t('settings.provider')}>
                  <select
                    value={draft.llmProvider}
                    onChange={(event) => update('llmProvider', event.target.value)}
                    className={selectClass}
                  >
                    <option value="embedded">{t('settings.embedded')}</option>
                    <option value="ollama">{t('settings.ollama')}</option>
                    <option value="openai">{t('settings.openai')}</option>
                    <option value="none">{t('settings.none')}</option>
                  </select>
                </Field>
                {draft.llmProvider === 'embedded' ? (
                  <Field label="Wbudowany model">
                    <select
                      value={draft.embeddedModel}
                      onChange={(event) => update('embeddedModel', event.target.value)}
                      className={selectClass}
                    >
                      {embeddedModels.length > 0 ? (
                        embeddedModels.map((m) => (
                          <option key={m.id} value={m.id}>
                            {m.name} {m.installed ? '(Zainstalowany)' : '(Wymaga pobrania)'}
                          </option>
                        ))
                      ) : (
                        <option value="qwen2.5-0.5b-instruct">Qwen 2.5 0.5B Instruct</option>
                      )}
                    </select>
                  </Field>
                ) : (
                  <Field label={t('settings.model')}>
                    <input
                      value={draft.llmModel}
                      onChange={(event) => update('llmModel', event.target.value)}
                      disabled={!primaryNeedsEndpoint}
                      spellCheck={false}
                      className={cn(inputClass, 'font-mono text-xs')}
                    />
                  </Field>
                )}
              </div>
              <Field label={t('settings.baseUrl')}>
                <input
                  value={
                    draft.llmProvider === 'embedded'
                      ? 'Lokalny runtime Rust (Candle in-process)'
                      : draft.llmBaseUrl
                  }
                  onChange={(event) => update('llmBaseUrl', event.target.value)}
                  disabled={!primaryNeedsEndpoint}
                  spellCheck={false}
                  className={cn(inputClass, 'font-mono text-xs')}
                />
              </Field>
              <Field label={t('settings.apiKeyEnv')}>
                <input
                  value={draft.llmApiKeyEnv}
                  onChange={(event) => update('llmApiKeyEnv', event.target.value)}
                  disabled={!primaryNeedsEndpoint}
                  spellCheck={false}
                  className={cn(inputClass, 'font-mono text-xs')}
                />
              </Field>
              <KeyField
                label={t('settings.apiKey')}
                value={secrets.llm}
                onChange={(value) => updateSecret('llm', value)}
                configured={view.llm.apiKeyConfigured}
                source={view.llm.apiKeySource}
                clearPending={clears.llm}
                onClear={() => requestClear('llm')}
                show={showLlm}
                onToggleShow={() => setShowLlm((value) => !value)}
                disabled={!primaryNeedsEndpoint}
                optional={draft.llmProvider === 'ollama'}
              />
            </div>
          </div>

          <div className="rounded-2xl border border-white/[0.07] bg-black/15 p-4">
            <div className="flex items-center gap-2">
              <ShieldCheck className="h-3.5 w-3.5 text-violet-300/70" />
              <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-400">
                {t('settings.fallback')}
              </h3>
            </div>
            <p className="mt-2 text-[10px] leading-relaxed text-slate-600">
              {t('settings.fallbackBody')}
            </p>
            <div className="mt-4 space-y-3">
              <Field label={t('settings.fallbackProvider')}>
                <select
                  value={draft.fallbackProvider}
                  onChange={(event) => update('fallbackProvider', event.target.value)}
                  className={selectClass}
                >
                  <option value="heuristic">{t('settings.heuristic')}</option>
                  <option value="none">{t('settings.none')}</option>
                  <option value="ollama">{t('settings.ollama')}</option>
                  <option value="openai">{t('settings.openai')}</option>
                </select>
              </Field>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t('settings.fallbackModel')}>
                  <input
                    value={draft.fallbackModel}
                    onChange={(event) => update('fallbackModel', event.target.value)}
                    disabled={!fallbackNeedsEndpoint}
                    spellCheck={false}
                    className={cn(inputClass, 'font-mono text-xs')}
                  />
                </Field>
                <Field label={t('settings.fallbackApiKeyEnv')}>
                  <input
                    value={draft.fallbackApiKeyEnv}
                    onChange={(event) => update('fallbackApiKeyEnv', event.target.value)}
                    disabled={!fallbackNeedsEndpoint}
                    spellCheck={false}
                    className={cn(inputClass, 'font-mono text-xs')}
                  />
                </Field>
              </div>
              <Field label={t('settings.fallbackUrl')}>
                <input
                  value={draft.fallbackBaseUrl}
                  onChange={(event) => update('fallbackBaseUrl', event.target.value)}
                  disabled={!fallbackNeedsEndpoint}
                  spellCheck={false}
                  className={cn(inputClass, 'font-mono text-xs')}
                />
              </Field>
              {fallbackNeedsEndpoint && (
                <KeyField
                  label={t('settings.apiKey')}
                  value={secrets.fallback}
                  onChange={(value) => updateSecret('fallback', value)}
                  configured={view.llm.fallbackApiKeyConfigured}
                  source={view.llm.fallbackApiKeySource}
                  clearPending={clears.fallback}
                  onClear={() => requestClear('fallback')}
                  show={showFallback}
                  onToggleShow={() => setShowFallback((value) => !value)}
                  optional={draft.fallbackProvider === 'ollama'}
                />
              )}
            </div>
          </div>
        </div>

        {/* Browser-based keys and registration */}
        <div className="mt-5 rounded-2xl border border-white/[0.07] bg-black/15 p-4 sm:p-5">
          <div className="flex items-center gap-2">
            <ExternalLink className="h-3.5 w-3.5 text-amber-300/80" />
            <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-300">
              {t('settings.browserLoginTitle')}
            </h3>
          </div>
          <p className="mt-1 text-[11px] leading-relaxed text-slate-500">
            {t('settings.browserLoginHint')}
          </p>
          <div className="mt-3 flex flex-wrap gap-2.5">
            <a
              href="https://openrouter.ai/keys"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3.5 py-2 text-xs text-slate-200 transition hover:border-amber-400/40 hover:bg-amber-400/[0.05] hover:text-amber-300"
            >
              <Sparkles className="h-3.5 w-3.5 text-amber-300/70" />
              {t('settings.openrouterKeyLink')}
            </a>
            <a
              href="https://platform.openai.com/api-keys"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3.5 py-2 text-xs text-slate-200 transition hover:border-white/20 hover:bg-white/[0.05] hover:text-white"
            >
              <Bot className="h-3.5 w-3.5 text-slate-400" />
              {t('settings.openaiKeyLink')}
            </a>
            <a
              href="https://brave.com/search/api/"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3.5 py-2 text-xs text-slate-200 transition hover:border-cyan-400/40 hover:bg-cyan-400/[0.05] hover:text-cyan-300"
            >
              <Cloud className="h-3.5 w-3.5 text-cyan-300/70" />
              {t('settings.braveKeyLink')}
            </a>
            <a
              href="https://ollama.com"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3.5 py-2 text-xs text-slate-200 transition hover:border-emerald-400/40 hover:bg-emerald-400/[0.05] hover:text-emerald-300"
            >
              <Cpu className="h-3.5 w-3.5 text-emerald-300/70" />
              {t('settings.ollamaDownloadLink')}
            </a>
          </div>
        </div>

        <div className="mt-5 rounded-2xl border border-white/[0.07] bg-black/15 p-4">
          <div className="flex items-center gap-2">
            <SlidersHorizontal className="h-3.5 w-3.5 text-slate-500" />
            <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-slate-500">
              {t('settings.advanced')}
            </h3>
          </div>
          <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <Field label={t('settings.timeout')}>
              <input
                type="number"
                min={1}
                max={300}
                value={draft.timeoutSecs}
                onChange={(event) => update('timeoutSecs', Number(event.target.value))}
                className={inputClass}
              />
            </Field>
            <Field label={t('settings.connectTimeout')}>
              <input
                type="number"
                min={1}
                max={60}
                value={draft.connectTimeoutSecs}
                onChange={(event) => update('connectTimeoutSecs', Number(event.target.value))}
                className={inputClass}
              />
            </Field>
            <Field label={t('settings.maxClaims')}>
              <input
                type="number"
                min={1}
                max={50}
                value={draft.maxClaims}
                onChange={(event) => update('maxClaims', Number(event.target.value))}
                className={inputClass}
              />
            </Field>
            <Field label={t('settings.maxInput')}>
              <input
                type="number"
                min={1000}
                max={100000}
                step={500}
                value={draft.maxInputChars}
                onChange={(event) => update('maxInputChars', Number(event.target.value))}
                className={inputClass}
              />
            </Field>
            <Field label={t('settings.temperature')}>
              <input
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={draft.temperature}
                onChange={(event) => update('temperature', Number(event.target.value))}
                className={inputClass}
              />
            </Field>
            <Field label={t('settings.maxTokens')}>
              <input
                type="number"
                min={64}
                max={8192}
                step={64}
                value={draft.maxTokens}
                onChange={(event) => update('maxTokens', Number(event.target.value))}
                className={inputClass}
              />
            </Field>
          </div>
        </div>
      </CardShell>

      <div className="grid gap-4 xl:grid-cols-2">
        <CardShell className="p-5">
          <div className="flex items-start gap-3">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-emerald-400/15 bg-emerald-400/[0.05]">
              <LockKeyhole className="h-4 w-4 text-emerald-300/80" />
            </span>
            <div>
              <h3 className="text-sm font-semibold text-slate-200">
                {t('settings.securityTitle')}
              </h3>
              <p className="mt-1.5 text-[11px] leading-relaxed text-slate-600">
                {t('settings.securityBody')}
              </p>
              <p className="mt-2 text-[10px] leading-relaxed text-slate-700">
                {t('settings.identityBody')}
              </p>
            </div>
          </div>
        </CardShell>

        <CardShell className="p-5">
          <div className="flex items-start gap-3">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-white/[0.08] bg-white/[0.03]">
              <Server className="h-4 w-4 text-slate-400" />
            </span>
            <div className="min-w-0 flex-1">
              <h3 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-slate-500">
                {t('settings.configPath')}
              </h3>
              <p className="mt-1.5 break-all font-mono text-[10px] text-slate-500">
                {view.configPath}
              </p>
              {view.envOverrides.length > 0 && (
                <div className="mt-3 rounded-lg border border-cyan-400/15 bg-cyan-400/[0.04] px-3 py-2">
                  <p className="text-[10px] font-semibold text-cyan-200/80">
                    {t('settings.envNote')}
                  </p>
                  <p className="mt-1 break-all font-mono text-[9px] leading-relaxed text-cyan-300/50">
                    {view.envOverrides.join(', ')}
                  </p>
                </div>
              )}
            </div>
          </div>
        </CardShell>
      </div>

      {saveError && (
        <div className="flex items-start gap-2 rounded-xl border border-rose-400/20 bg-rose-400/[0.05] px-4 py-3">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-rose-300" />
          <div>
            <p className="text-xs font-semibold text-rose-200">{t('settings.error')}</p>
            <p className="mt-1 break-words font-mono text-[10px] text-rose-300/60">{saveError}</p>
          </div>
        </div>
      )}

      <div className="sticky bottom-4 z-20 flex flex-wrap items-center gap-3 rounded-2xl border border-white/[0.09] bg-[#0a0e15]/92 px-4 py-3 shadow-[0_18px_60px_rgba(0,0,0,0.45)] backdrop-blur-xl">
        <div className="flex items-center gap-2 text-[11px] text-slate-500">
          {dirty ? (
            <>
              <span className="h-1.5 w-1.5 rounded-full bg-amber-400 shadow-[0_0_6px_rgba(251,191,36,0.8)]" />
              {t('settings.unsaved')}
            </>
          ) : (
            <>
              <Check className="h-3.5 w-3.5 text-emerald-400" />
              {saved ? t('settings.saved') : t('settings.securityNote')}
            </>
          )}
        </div>
        <button
          type="button"
          onClick={() => void save()}
          disabled={saving || !dirty}
          className="ml-auto inline-flex items-center gap-2 rounded-xl bg-amber-400 px-4 py-2.5 text-xs font-bold text-amber-950 shadow-[0_8px_28px_rgba(251,191,36,0.18)] transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {saving ? (
            <LoaderCircle className="h-4 w-4 animate-spin" />
          ) : (
            <Save className="h-4 w-4" />
          )}
          {saving ? t('settings.saving') : t('settings.save')}
        </button>
      </div>
    </div>
  );
}
