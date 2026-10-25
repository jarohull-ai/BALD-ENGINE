import { useCallback, useEffect, useState } from 'react';
import {
  AlertCircle,
  Bot,
  CheckCircle2,
  Clock,
  History,
  LoaderCircle,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Search,
  Sparkles,
  Trash2,
  X,
  Zap,
} from 'lucide-react';
import { createBot, deleteBot, fetchBotRuns, fetchBots, runBotNow, updateBot } from '../api';
import { useI18n } from '../i18n';
import { cn, formatTimestamp } from '../lib';
import type { BotRunRecord, NewResearchBot, ResearchBot } from '../types';
import { Badge, inputClass, labelClass, selectClass } from './ui';

export function BotsView() {
  const { t } = useI18n();
  const [bots, setBots] = useState<ResearchBot[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [runningBotId, setRunningBotId] = useState<string | null>(null);

  // Form state
  const [name, setName] = useState('');
  const [query, setQuery] = useState('');
  const [intervalMins, setIntervalMins] = useState(360);
  const [maxSources, setMaxSources] = useState(3);
  const [autoIngest, setAutoIngest] = useState(true);
  const [saving, setSaving] = useState(false);

  // History drawer state
  const [historyBot, setHistoryBot] = useState<ResearchBot | null>(null);
  const [runs, setRuns] = useState<BotRunRecord[]>([]);
  const [loadingRuns, setLoadingRuns] = useState(false);

  const loadBots = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchBots();
      setBots(data);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadBots();
  }, [loadBots]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !query.trim()) return;

    setSaving(true);
    setError(null);
    try {
      const payload: NewResearchBot = {
        name: name.trim(),
        query: query.trim(),
        interval_mins: intervalMins,
        max_sources_per_run: maxSources,
        auto_ingest: autoIngest,
      };
      await createBot(payload);
      setName('');
      setQuery('');
      setIsCreating(false);
      await loadBots();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };

  const handleToggleEnabled = async (bot: ResearchBot) => {
    try {
      const updated: ResearchBot = { ...bot, enabled: !bot.enabled };
      await updateBot(updated);
      setBots((prev) => prev.map((b) => (b.id === bot.id ? updated : b)));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const handleDelete = async (bot: ResearchBot) => {
    if (!window.confirm(t('bots.deleteConfirm'))) return;
    try {
      await deleteBot(bot.id);
      setBots((prev) => prev.filter((b) => b.id !== bot.id));
      if (historyBot?.id === bot.id) {
        setHistoryBot(null);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const handleRunNow = async (bot: ResearchBot) => {
    setRunningBotId(bot.id);
    setError(null);
    try {
      await runBotNow(bot.id);
      await loadBots();
      if (historyBot?.id === bot.id) {
        await openHistory(bot);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRunningBotId(null);
    }
  };

  const openHistory = async (bot: ResearchBot) => {
    setHistoryBot(bot);
    setLoadingRuns(true);
    try {
      const history = await fetchBotRuns(bot.id);
      setRuns(history);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoadingRuns(false);
    }
  };

  return (
    <div className="space-y-7">
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <span className="text-[11px] font-bold uppercase tracking-[0.2em] text-amber-400">
            {t('bots.eyebrow')}
          </span>
          <h1 className="mt-1 text-2xl font-bold tracking-tight text-white sm:text-3xl">
            {t('bots.title')}
          </h1>
          <p className="mt-1 max-w-2xl text-xs text-slate-400 sm:text-sm">{t('bots.subtitle')}</p>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void loadBots()}
            className="flex items-center gap-2 rounded-xl border border-white/10 bg-white/[0.03] px-3 py-2 text-xs font-semibold text-slate-300 transition hover:bg-white/[0.08]"
            title={t('claims.refresh')}
          >
            <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
            <span className="hidden sm:inline">{t('claims.refresh')}</span>
          </button>

          <button
            type="button"
            onClick={() => setIsCreating(!isCreating)}
            className="flex items-center gap-2 rounded-xl bg-amber-400 px-4 py-2 text-xs font-bold text-black shadow-lg shadow-amber-400/20 transition hover:bg-amber-300 active:scale-95"
          >
            <Plus className="h-4 w-4" />
            <span>{t('bots.newBot')}</span>
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-3 rounded-2xl border border-rose-500/30 bg-rose-500/10 p-4 text-xs text-rose-200">
          <AlertCircle className="h-4 w-4 shrink-0 text-rose-400" />
          <span>{error}</span>
          <button
            type="button"
            onClick={() => setError(null)}
            className="ml-auto text-rose-400 hover:text-white"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      {/* Creation form */}
      {isCreating && (
        <form
          onSubmit={handleCreate}
          className="relative rounded-2xl border border-amber-400/30 bg-gradient-to-b from-[#10141f] to-[#0a0d14] p-5 shadow-2xl sm:p-6"
        >
          <div className="mb-4 flex items-center justify-between border-b border-white/10 pb-3">
            <h2 className="flex items-center gap-2 text-sm font-bold text-white">
              <Bot className="h-4 w-4 text-amber-400" />
              {t('bots.newBot')}
            </h2>
            <button
              type="button"
              onClick={() => setIsCreating(false)}
              className="text-slate-400 hover:text-white"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="sm:col-span-2">
              <label className={labelClass}>{t('bots.name')}</label>
              <input
                type="text"
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('bots.namePlaceholder')}
                className={inputClass}
              />
            </div>

            <div className="sm:col-span-2">
              <label className={labelClass}>{t('bots.query')}</label>
              <div className="relative">
                <Search className="pointer-events-none absolute left-3.5 top-3 h-4 w-4 text-slate-500" />
                <input
                  type="text"
                  required
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t('bots.queryPlaceholder')}
                  className={cn(inputClass, 'pl-10')}
                />
              </div>
            </div>

            <div>
              <label className={labelClass}>{t('bots.interval')}</label>
              <select
                value={intervalMins}
                onChange={(e) => setIntervalMins(Number(e.target.value))}
                className={selectClass}
              >
                <option value={60}>{t('bots.interval_1h')}</option>
                <option value={360}>{t('bots.interval_6h')}</option>
                <option value={1440}>{t('bots.interval_24h')}</option>
                <option value={10080}>{t('bots.interval_7d')}</option>
              </select>
            </div>

            <div>
              <label className={labelClass}>{t('bots.maxSources')}</label>
              <select
                value={maxSources}
                onChange={(e) => setMaxSources(Number(e.target.value))}
                className={selectClass}
              >
                <option value={1}>1 artykuł</option>
                <option value={3}>3 artykuły (zalecane)</option>
                <option value={5}>5 artykułów</option>
                <option value={10}>10 artykułów</option>
              </select>
            </div>

            <div className="sm:col-span-2">
              <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-white/5 bg-white/[0.02] p-3 transition hover:bg-white/[0.04]">
                <input
                  type="checkbox"
                  checked={autoIngest}
                  onChange={(e) => setAutoIngest(e.target.checked)}
                  className="h-4 w-4 rounded border-white/20 bg-black/40 text-amber-400 focus:ring-amber-400/20"
                />
                <span className="text-xs text-slate-200">{t('bots.autoIngest')}</span>
              </label>
            </div>
          </div>

          <div className="mt-5 flex items-center justify-end gap-2 border-t border-white/10 pt-4">
            <button
              type="button"
              onClick={() => setIsCreating(false)}
              className="rounded-xl px-4 py-2 text-xs font-semibold text-slate-400 hover:text-white"
            >
              {t('bots.cancel')}
            </button>
            <button
              type="submit"
              disabled={saving || !name.trim() || !query.trim()}
              className="flex items-center gap-2 rounded-xl bg-amber-400 px-5 py-2 text-xs font-bold text-black shadow-lg shadow-amber-400/20 transition hover:bg-amber-300 disabled:opacity-50"
            >
              {saving ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
              <span>{t('bots.create')}</span>
            </button>
          </div>
        </form>
      )}

      {/* Bot List */}
      {loading ? (
        <div className="flex h-48 items-center justify-center">
          <LoaderCircle className="h-6 w-6 animate-spin text-amber-400" />
        </div>
      ) : bots.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-white/10 bg-white/[0.015] p-12 text-center">
          <div className="grid h-12 w-12 place-items-center rounded-2xl border border-amber-400/20 bg-amber-400/10 text-amber-300">
            <Bot className="h-6 w-6" />
          </div>
          <h3 className="mt-4 text-base font-bold text-white">{t('bots.emptyTitle')}</h3>
          <p className="mt-1 max-w-md text-xs text-slate-400">{t('bots.emptyBody')}</p>
          <button
            type="button"
            onClick={() => setIsCreating(true)}
            className="mt-5 flex items-center gap-2 rounded-xl bg-amber-400 px-4 py-2 text-xs font-bold text-black transition hover:bg-amber-300"
          >
            <Plus className="h-4 w-4" />
            <span>{t('bots.newBot')}</span>
          </button>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {bots.map((bot) => {
            const isRunning = runningBotId === bot.id;
            return (
              <div
                key={bot.id}
                className={cn(
                  'relative flex flex-col justify-between rounded-2xl border bg-[#0d1017]/80 p-5 backdrop-blur-xl transition duration-200',
                  bot.enabled
                    ? 'border-white/10 shadow-lg hover:border-amber-400/30'
                    : 'border-white/5 opacity-60 hover:opacity-100',
                )}
              >
                <div>
                  {/* Top status bar */}
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          'h-2 w-2 rounded-full',
                          bot.enabled ? 'bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.8)]' : 'bg-slate-600',
                        )}
                      />
                      <Badge tone={bot.enabled ? 'emerald' : 'neutral'}>
                        {bot.enabled ? t('bots.statusActive') : t('bots.statusPaused')}
                      </Badge>
                    </div>

                    <div className="flex items-center gap-1.5 text-[11px] text-slate-400">
                      <Clock className="h-3 w-3 text-slate-500" />
                      <span>
                        {bot.interval_mins < 60
                          ? `${bot.interval_mins}m`
                          : bot.interval_mins === 60
                            ? '1h'
                            : bot.interval_mins === 360
                              ? '6h'
                              : bot.interval_mins === 1440
                                ? '24h'
                                : `${Math.round(bot.interval_mins / 1440)}d`}
                      </span>
                    </div>
                  </div>

                  {/* Bot details */}
                  <div className="mt-3">
                    <h3 className="text-base font-bold text-white">{bot.name}</h3>
                    <div className="mt-1 flex items-center gap-1.5 rounded-lg border border-white/5 bg-black/30 px-2.5 py-1.5 text-xs text-amber-200/90 font-mono">
                      <Search className="h-3 w-3 shrink-0 text-amber-400/60" />
                      <span className="truncate">{bot.query}</span>
                    </div>
                  </div>

                  {/* Metadata grid */}
                  <div className="mt-4 grid grid-cols-2 gap-2 rounded-xl border border-white/5 bg-white/[0.02] p-3 text-[11px]">
                    <div>
                      <span className="text-slate-500">{t('bots.runCount')}</span>
                      <p className="font-semibold text-slate-200">{bot.run_count}</p>
                    </div>
                    <div>
                      <span className="text-slate-500">{t('bots.claimsFound')}</span>
                      <p className="flex items-center gap-1 font-semibold text-amber-300">
                        <Sparkles className="h-3 w-3" />
                        {bot.claims_found}
                      </p>
                    </div>
                    <div className="col-span-2 border-t border-white/5 pt-2">
                      <span className="text-slate-500">{t('bots.lastRun')}</span>
                      <p className="truncate text-slate-300">
                        {bot.last_run_at ? formatTimestamp(bot.last_run_at) : t('bots.neverRun')}
                      </p>
                    </div>
                  </div>
                </div>

                {/* Actions footer */}
                <div className="mt-4 flex items-center justify-between gap-2 border-t border-white/10 pt-3">
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void handleRunNow(bot)}
                      disabled={isRunning}
                      className={cn(
                        'flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs font-semibold transition',
                        isRunning
                          ? 'bg-amber-400/20 text-amber-200 cursor-not-allowed'
                          : 'bg-amber-400/10 text-amber-300 hover:bg-amber-400/20 active:scale-95',
                      )}
                      title={t('bots.runNow')}
                    >
                      {isRunning ? (
                        <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Zap className="h-3.5 w-3.5" />
                      )}
                      <span>{isRunning ? t('bots.running') : t('bots.runNow')}</span>
                    </button>

                    <button
                      type="button"
                      onClick={() => void openHistory(bot)}
                      className="flex items-center gap-1 rounded-lg border border-white/5 bg-white/[0.02] px-2.5 py-1.5 text-xs font-medium text-slate-400 hover:bg-white/[0.06] hover:text-slate-200 transition"
                      title={t('bots.history')}
                    >
                      <History className="h-3.5 w-3.5" />
                      <span className="hidden sm:inline">{t('bots.history')}</span>
                    </button>
                  </div>

                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void handleToggleEnabled(bot)}
                      className={cn(
                        'rounded-lg p-1.5 text-xs transition',
                        bot.enabled
                          ? 'text-slate-400 hover:bg-white/10 hover:text-slate-200'
                          : 'text-amber-400 hover:bg-amber-400/10',
                      )}
                      title={bot.enabled ? t('bots.pause') : t('bots.resume')}
                    >
                      {bot.enabled ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                    </button>

                    <button
                      type="button"
                      onClick={() => void handleDelete(bot)}
                      className="rounded-lg p-1.5 text-xs text-slate-500 hover:bg-rose-500/10 hover:text-rose-400 transition"
                      title={t('bots.delete')}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* History Drawer */}
      {historyBot && (
        <div className="fixed inset-0 z-50 flex justify-end bg-black/60 backdrop-blur-sm animate-in fade-in duration-200">
          <div className="relative flex h-full w-full max-w-lg flex-col border-l border-white/10 bg-[#0a0d14] p-6 shadow-2xl">
            <div className="flex items-center justify-between border-b border-white/10 pb-4">
              <div>
                <span className="text-[10px] font-bold uppercase tracking-wider text-amber-400">
                  {t('bots.historyTitle')}
                </span>
                <h3 className="text-base font-bold text-white">{historyBot.name}</h3>
              </div>
              <button
                type="button"
                onClick={() => setHistoryBot(null)}
                className="rounded-lg p-1.5 text-slate-400 hover:bg-white/10 hover:text-white"
              >
                <X className="h-5 w-5" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto py-4 space-y-3">
              {loadingRuns ? (
                <div className="flex h-32 items-center justify-center">
                  <LoaderCircle className="h-6 w-6 animate-spin text-amber-400" />
                </div>
              ) : runs.length === 0 ? (
                <p className="text-center text-xs text-slate-500 py-8">{t('bots.noHistory')}</p>
              ) : (
                runs.map((r) => {
                  const isSuccess = r.status === 'success';
                  const isPartial = r.status === 'partial';
                  return (
                    <div
                      key={r.id}
                      className="rounded-xl border border-white/5 bg-white/[0.02] p-3 text-xs space-y-2"
                    >
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-1.5">
                          {isSuccess ? (
                            <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
                          ) : isPartial ? (
                            <AlertCircle className="h-3.5 w-3.5 text-amber-400" />
                          ) : (
                            <AlertCircle className="h-3.5 w-3.5 text-rose-400" />
                          )}
                          <span
                            className={cn(
                              'font-semibold uppercase tracking-wider text-[10px]',
                              isSuccess ? 'text-emerald-300' : isPartial ? 'text-amber-300' : 'text-rose-300',
                            )}
                          >
                            {r.status}
                          </span>
                        </div>
                        <span className="text-[10px] text-slate-500">{formatTimestamp(r.started_at)}</span>
                      </div>

                      <div className="grid grid-cols-2 gap-2 text-[11px] text-slate-400">
                        <div>
                          <span>Zbadano URL: </span>
                          <strong className="text-slate-200">{r.urls_checked}</strong>
                        </div>
                        <div>
                          <span>Nowe twierdzenia: </span>
                          <strong className="text-amber-300">+{r.claims_added}</strong>
                        </div>
                      </div>

                      {r.error_msg && (
                        <p className="rounded bg-rose-500/10 p-2 text-[11px] text-rose-300 border border-rose-500/20">
                          {r.error_msg}
                        </p>
                      )}
                    </div>
                  );
                })
              )}
            </div>

            <div className="border-t border-white/10 pt-4">
              <button
                type="button"
                onClick={() => setHistoryBot(null)}
                className="w-full rounded-xl border border-white/10 bg-white/[0.03] py-2 text-xs font-semibold text-slate-300 hover:bg-white/[0.08]"
              >
                Zamknij
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
