import { useMemo, useState } from 'react';
import {
  AlertTriangle,
  ArrowDownWideNarrow,
  BookOpenText,
  Check,
  Copy,
  FilterX,
  RefreshCw,
  Search,
  ShieldCheck,
  X,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { copyText, kindMeta, stanceMeta, stanceOrder } from '../lib';
import type { ClaimKind, ClaimSummary, Stance } from '../types';
import { ClaimCard } from './ClaimCard';
import { Badge, Spinner } from './ui';

const kinds: ClaimKind[] = [
  'fact',
  'causal',
  'concealment',
  'conspiracy',
  'prophecy',
  'other',
];

function SkeletonCard() {
  return (
    <div className="animate-pulse rounded-2xl border border-white/[0.06] bg-white/[0.02] p-5">
      <div className="h-5 w-24 rounded-full bg-white/[0.06]" />
      <div className="mt-4 space-y-2">
        <div className="h-4 w-full rounded bg-white/[0.05]" />
        <div className="h-4 w-11/12 rounded bg-white/[0.05]" />
        <div className="h-4 w-2/3 rounded bg-white/[0.04]" />
      </div>
      <div className="mt-5 h-1.5 w-full rounded-full bg-white/[0.05]" />
      <div className="mt-5 flex gap-3">
        <div className="h-3 w-20 rounded bg-white/[0.04]" />
        <div className="h-3 w-20 rounded bg-white/[0.04]" />
      </div>
    </div>
  );
}

function EmptyLedger() {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const command = t('claims.emptyCommand');

  async function copy() {
    if (await copyText(command)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    }
  }

  return (
    <div className="animate-rise relative overflow-hidden rounded-3xl border border-dashed border-white/10 bg-white/[0.018] px-6 py-14 text-center">
      <div className="pointer-events-none absolute left-1/2 top-0 h-64 w-64 -translate-x-1/2 -translate-y-1/2 rounded-full bg-amber-400/10 blur-3xl" />
      <div className="relative mx-auto flex h-14 w-14 items-center justify-center rounded-2xl border border-amber-300/15 bg-amber-400/[0.06]">
        <BookOpenText className="h-6 w-6 text-amber-300/70" />
      </div>
      <h3 className="relative mt-5 text-lg font-semibold text-slate-200">
        {t('claims.emptyTitle')}
      </h3>
      <p className="relative mx-auto mt-2 max-w-lg text-sm leading-relaxed text-slate-500">
        {t('claims.emptyBody')}
      </p>
      <div className="relative mx-auto mt-6 flex max-w-xl flex-col items-stretch gap-2 sm:flex-row">
        <code className="scrollbar-thin flex-1 overflow-x-auto rounded-xl border border-white/10 bg-black/35 px-4 py-3 text-left font-mono text-[11px] text-amber-200/80">
          {command}
        </code>
        <button
          type="button"
          onClick={copy}
          className="inline-flex items-center justify-center gap-2 rounded-xl border border-white/10 bg-white/[0.04] px-4 py-3 text-xs font-semibold text-slate-300 transition hover:border-white/20 hover:bg-white/[0.07]"
        >
          {copied ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
          {copied ? t('claims.copied') : t('claims.copy')}
        </button>
      </div>
      <p className="relative mt-4 inline-flex items-center gap-1.5 text-[10px] text-slate-600">
        <ShieldCheck className="h-3 w-3 text-emerald-500/60" />
        CitationGate · robots.txt · SSRF policy
      </p>
    </div>
  );
}

export function ClaimsView({
  claims,
  loading,
  error,
  onRefresh,
  onOpen,
}: {
  claims: ClaimSummary[];
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onOpen: (claim: ClaimSummary) => void;
}) {
  const { t, lang } = useI18n();
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState<ClaimKind | 'all'>('all');
  const [stance, setStance] = useState<Stance | 'all'>('all');
  const [sort, setSort] = useState<'sources' | 'recent' | 'newest'>('sources');

  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase(lang === 'pl' ? 'pl-PL' : 'en-GB');
    const rows = claims.filter((claim) => {
      if (kind !== 'all' && claim.kind !== kind) return false;
      if (stance !== 'all' && !claim.stances.some((item) => item.stance === stance)) {
        return false;
      }
      if (!needle) return true;
      return (
        claim.text.toLocaleLowerCase().includes(needle) ||
        claim.id.toLocaleLowerCase().includes(needle)
      );
    });

    return rows.sort((a, b) => {
      if (sort === 'sources') {
        return b.document_count - a.document_count || b.evidence_count - a.evidence_count;
      }
      if (sort === 'newest') {
        return b.first_seen.localeCompare(a.first_seen);
      }
      return b.last_seen.localeCompare(a.last_seen);
    });
  }, [claims, kind, lang, query, sort, stance]);

  const hasFilters = query.trim() !== '' || kind !== 'all' || stance !== 'all';
  const showEmpty = !loading && !error && claims.length === 0;
  const showNoMatch = !loading && !error && claims.length > 0 && filtered.length === 0;

  function clearFilters() {
    setQuery('');
    setKind('all');
    setStance('all');
  }

  return (
    <div className="space-y-6">
      <div className="grid gap-6 lg:grid-cols-[1fr_auto] lg:items-end">
        <div className="animate-rise">
          <div className="mb-3 flex items-center gap-2">
            <span className="h-px w-6 bg-amber-400/50" />
            <span className="text-[10px] font-bold uppercase tracking-[0.22em] text-amber-300/80">
              {t('claims.eyebrow')}
            </span>
          </div>
          <h1 className="text-balance text-3xl font-semibold tracking-[-0.03em] text-slate-100 sm:text-4xl">
            {t('claims.title')}
          </h1>
          <p className="mt-3 max-w-2xl text-sm leading-relaxed text-slate-500">
            {t('claims.lede')}
          </p>
        </div>

        <button
          type="button"
          onClick={onRefresh}
          disabled={loading}
          className="inline-flex w-fit items-center gap-2 rounded-xl border border-white/10 bg-white/[0.035] px-4 py-2.5 text-xs font-semibold text-slate-300 transition hover:border-amber-300/25 hover:bg-amber-400/[0.06] hover:text-amber-100 disabled:cursor-wait disabled:opacity-50"
        >
          {loading ? <Spinner className="text-amber-300" /> : <RefreshCw className="h-3.5 w-3.5" />}
          {t('claims.refresh')}
        </button>
      </div>

      {error && (
        <div className="flex items-start gap-3 rounded-2xl border border-rose-400/20 bg-rose-400/[0.06] px-4 py-3.5">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-rose-300" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold text-rose-200">{t('error.title')}</p>
            <p className="mt-1 break-words font-mono text-[11px] leading-relaxed text-rose-300/70">
              {error}
            </p>
          </div>
          <button
            type="button"
            onClick={onRefresh}
            className="shrink-0 rounded-lg border border-rose-300/20 px-3 py-1.5 text-[11px] font-semibold text-rose-200 transition hover:bg-rose-400/10"
          >
            {t('error.retry')}
          </button>
        </div>
      )}

      {showEmpty ? (
        <EmptyLedger />
      ) : (
        <>
          <div className="animate-rise rounded-2xl border border-white/[0.07] bg-white/[0.018] p-3 [animation-delay:40ms]">
            <div className="grid gap-2 lg:grid-cols-[minmax(0,1fr)_170px_170px_190px]">
              <label className="relative block">
                <span className="sr-only">{t('claims.search')}</span>
                <Search className="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-600" />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t('claims.searchPlaceholder')}
                  className="w-full rounded-xl border border-white/[0.08] bg-black/25 py-2.5 pl-10 pr-9 text-sm text-slate-200 placeholder:text-slate-600 transition focus:border-amber-400/35 focus:outline-none focus:ring-2 focus:ring-amber-400/10"
                />
                {query && (
                  <button
                    type="button"
                    onClick={() => setQuery('')}
                    aria-label={t('claims.clearFilters')}
                    className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-slate-600 transition hover:bg-white/5 hover:text-slate-300"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </label>

              <label className="relative block">
                <span className="sr-only">{t('claims.kind')}</span>
                <select
                  value={kind}
                  onChange={(event) => setKind(event.target.value as ClaimKind | 'all')}
                  className="w-full appearance-none rounded-xl border border-white/[0.08] bg-[#0a0e15] px-3 py-2.5 text-xs text-slate-400 transition focus:border-amber-400/35 focus:outline-none"
                >
                  <option value="all">{t('claims.allKinds')}</option>
                  {kinds.map((value) => (
                    <option key={value} value={value}>
                      {t(kindMeta[value].key)}
                    </option>
                  ))}
                </select>
              </label>

              <label className="relative block">
                <span className="sr-only">{t('claims.stance')}</span>
                <select
                  value={stance}
                  onChange={(event) => setStance(event.target.value as Stance | 'all')}
                  className="w-full appearance-none rounded-xl border border-white/[0.08] bg-[#0a0e15] px-3 py-2.5 text-xs text-slate-400 transition focus:border-amber-400/35 focus:outline-none"
                >
                  <option value="all">{t('claims.allStances')}</option>
                  {stanceOrder.map((value) => (
                    <option key={value} value={value}>
                      {t(stanceMeta[value].key)}
                    </option>
                  ))}
                </select>
              </label>

              <label className="relative block">
                <span className="sr-only">{t('claims.sort')}</span>
                <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate-600">
                  <ArrowDownWideNarrow className="h-3.5 w-3.5" />
                </span>
                <select
                  value={sort}
                  onChange={(event) => setSort(event.target.value as typeof sort)}
                  className="w-full appearance-none rounded-xl border border-white/[0.08] bg-[#0a0e15] py-2.5 pl-9 pr-3 text-xs text-slate-400 transition focus:border-amber-400/35 focus:outline-none"
                >
                  <option value="sources">{t('claims.sort.sources')}</option>
                  <option value="recent">{t('claims.sort.recent')}</option>
                  <option value="newest">{t('claims.sort.newest')}</option>
                </select>
              </label>
            </div>
          </div>

          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-medium text-slate-500">
                {t('claims.results', { count: filtered.length })}
              </span>
              {hasFilters && (
                <button
                  type="button"
                  onClick={clearFilters}
                  className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[10px] font-semibold text-slate-600 transition hover:bg-white/5 hover:text-slate-300"
                >
                  <FilterX className="h-3 w-3" />
                  {t('claims.clearFilters')}
                </button>
              )}
            </div>
            {hasFilters && (
              <Badge tone="neutral">
                {kind !== 'all' ? t(kindMeta[kind].key) : t('claims.allKinds')}
                {stance !== 'all' ? ` · ${t(stanceMeta[stance].key)}` : ''}
              </Badge>
            )}
          </div>

          {loading && filtered.length === 0 ? (
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
              {Array.from({ length: 6 }).map((_, index) => (
                <SkeletonCard key={index} />
              ))}
            </div>
          ) : showNoMatch ? (
            <div className="rounded-2xl border border-dashed border-white/10 px-6 py-14 text-center">
              <FilterX className="mx-auto h-6 w-6 text-slate-600" />
              <p className="mt-3 text-sm text-slate-400">{t('claims.noMatch')}</p>
              <button
                type="button"
                onClick={clearFilters}
                className="mt-4 rounded-lg border border-white/10 px-3 py-2 text-xs font-semibold text-slate-300 transition hover:bg-white/5"
              >
                {t('claims.clearFilters')}
              </button>
            </div>
          ) : (
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
              {filtered.map((claim, index) => (
                <ClaimCard key={claim.id} claim={claim} index={index} onOpen={onOpen} />
              ))}
            </div>
          )}

          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-white/[0.05] pt-4 text-[10px] text-slate-700">
            <span>{t('footer.localOnly')}</span>
            <span className="inline-flex items-center gap-1.5">
              <ShieldCheck className="h-3 w-3 text-emerald-600/70" />
              {t('footer.guard')}
            </span>
            <span className="font-mono">{t('footer.version')}</span>
          </div>
        </>
      )}
    </div>
  );
}
