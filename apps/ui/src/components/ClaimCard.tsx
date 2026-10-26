import {
  ArrowUpRight,
  CalendarDays,
  CircleHelp,
  EyeOff,
  FileText,
  Link2,
  Network,
  Quote,
  Sparkles,
  Trash2,
  Waypoints,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { cn, formatDate, kindMeta, stanceMeta, stanceOrder } from '../lib';
import type { ClaimKind, ClaimSummary } from '../types';

const kindIcons: Record<ClaimKind, typeof FileText> = {
  fact: FileText,
  causal: Waypoints,
  concealment: EyeOff,
  conspiracy: Network,
  prophecy: Sparkles,
  other: CircleHelp,
};

function StanceMix({ claim }: { claim: ClaimSummary }) {
  const { t } = useI18n();
  const total = claim.stances.reduce((sum, item) => sum + item.count, 0);
  const present = stanceOrder
    .map((stance) => ({
      stance,
      count: claim.stances.find((item) => item.stance === stance)?.count ?? 0,
    }))
    .filter((item) => item.count > 0);

  return (
    <div>
      <div className="flex h-1.5 w-full gap-0.5 overflow-hidden rounded-full bg-white/[0.05]">
        {present.length === 0 ? (
          <span className="h-full w-full bg-slate-600/50" />
        ) : (
          present.map((item) => (
            <span
              key={item.stance}
              className={cn('h-full rounded-full', stanceMeta[item.stance].bar)}
              style={{ width: `${total === 0 ? 0 : (item.count / total) * 100}%` }}
            />
          ))
        )}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1">
        {present.map((item) => (
          <span
            key={item.stance}
            className={cn('inline-flex items-center gap-1.5 text-[10px]', stanceMeta[item.stance].text)}
          >
            <span className={cn('h-1.5 w-1.5 rounded-full', stanceMeta[item.stance].dot)} />
            {t(stanceMeta[item.stance].key)}
            <span className="font-mono text-slate-600">{item.count}</span>
          </span>
        ))}
        {present.length === 0 && (
          <span className="text-[10px] text-slate-600">{t('stance.mentions')}</span>
        )}
      </div>
    </div>
  );
}

export function ClaimCard({
  claim,
  index,
  onOpen,
  onDelete,
}: {
  claim: ClaimSummary;
  index: number;
  onOpen: (claim: ClaimSummary) => void;
  onDelete?: (claimId: string) => void;
}) {
  const { t, lang } = useI18n();
  const KindIcon = kindIcons[claim.kind] ?? CircleHelp;
  const kind = kindMeta[claim.kind] ?? kindMeta.other;
  const dominant = stanceOrder
    .map((stance) => ({
      stance,
      count: claim.stances.find((item) => item.stance === stance)?.count ?? 0,
    }))
    .sort((a, b) => b.count - a.count)[0];
  const accent = dominant && dominant.count > 0 ? stanceMeta[dominant.stance].bar : 'bg-slate-500';

  return (
    <button
      type="button"
      onClick={() => onOpen(claim)}
      aria-label={`${t('card.open')}: ${claim.text}`}
      className="group relative flex h-full animate-rise flex-col overflow-hidden rounded-2xl border border-white/[0.075] bg-gradient-to-b from-white/[0.035] to-white/[0.015] text-left shadow-[0_16px_50px_rgba(0,0,0,0.16)] transition duration-300 hover:-translate-y-0.5 hover:border-white/15 hover:shadow-[0_24px_60px_rgba(0,0,0,0.28)]"
      style={{ animationDelay: `${Math.min(index, 9) * 35}ms` }}
    >
      <span className={cn('absolute inset-x-0 top-0 h-px opacity-70', accent)} />
      <span
        className={cn(
          'pointer-events-none absolute -right-16 -top-16 h-40 w-40 rounded-full opacity-0 blur-3xl transition duration-500 group-hover:opacity-20',
          accent,
        )}
      />

      <div className="flex flex-1 flex-col p-5">
        <div className="mb-3 flex items-center justify-between gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <span
              className={cn(
                'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.11em]',
                kind.tone,
              )}
            >
              <KindIcon className="h-3 w-3" />
              {t(kind.key)}
            </span>
            {claim.lang && (
              <span className="rounded-full border border-white/[0.07] px-2 py-1 font-mono text-[9px] uppercase tracking-[0.14em] text-slate-600">
                {claim.lang}
              </span>
            )}
          </div>
          {onDelete && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onDelete(claim.id);
              }}
              title={t('claims.delete')}
              aria-label={t('claims.delete')}
              className="rounded-lg p-1.5 text-slate-500 opacity-60 transition hover:bg-rose-500/20 hover:text-rose-300 hover:opacity-100"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          )}
        </div>

        <h3 className="text-balance text-[15px] font-medium leading-relaxed text-slate-200 transition group-hover:text-slate-50">
          {claim.text}
        </h3>

        <div className="mt-4">
          <StanceMix claim={claim} />
        </div>

        <div className="mt-auto pt-5">
          <div className="flex flex-wrap items-center gap-2 text-[10px] text-slate-600">
            <span className="inline-flex items-center gap-1">
              <Link2 className="h-3 w-3" />
              {t('card.sources', { count: claim.document_count })}
            </span>
            <span className="text-slate-800">·</span>
            <span className="inline-flex items-center gap-1">
              <Quote className="h-3 w-3" />
              {t('card.citations', { count: claim.evidence_count })}
            </span>
            <span className="ml-auto inline-flex items-center gap-1">
              <CalendarDays className="h-3 w-3" />
              {formatDate(claim.last_seen, lang)}
            </span>
          </div>

          <div className="mt-3 flex items-center justify-between border-t border-white/[0.06] pt-3">
            <span className="font-mono text-[9px] uppercase tracking-[0.12em] text-slate-700">
              {claim.id.replace(/^sha256:/, '').slice(0, 16)}
            </span>
            <span className="inline-flex items-center gap-1 text-[10px] font-semibold text-amber-300/70 transition group-hover:text-amber-200">
              {t('card.open')}
              <ArrowUpRight className="h-3.5 w-3.5 transition group-hover:-translate-y-0.5 group-hover:translate-x-0.5" />
            </span>
          </div>
        </div>
      </div>
    </button>
  );
}
