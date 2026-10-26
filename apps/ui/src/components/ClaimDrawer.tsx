import { useCallback, useEffect, useState } from 'react';
import {
  AlertTriangle,
  BookOpen,
  Building2,
  CalendarDays,
  CalendarRange,
  Check,
  CircleHelp,
  Copy,
  Cpu,
  Download,
  ExternalLink,
  FileText,
  Hash,
  Link2,
  MapPin,
  Quote,
  Scale,
  ShieldCheck,
  Sparkles,
  Trash2,
  UserCheck,
  X,
} from 'lucide-react';
import { fetchClaim } from '../api';
import { useI18n } from '../i18n';
import { copyText, cn, formatDate, kindMeta, shortId, stanceMeta } from '../lib';
import type { ClaimDetail, ClaimSummary, EntityKind, EvidenceItem } from '../types';
import { Badge, Spinner } from './ui';

const entityIcons: Record<EntityKind, typeof Building2> = {
  public_actor: UserCheck,
  organization: Building2,
  location: MapPin,
  event: CalendarRange,
  document: FileText,
  technology: Cpu,
  law: Scale,
  work: BookOpen,
  other: CircleHelp,
};

function EvidenceCard({
  evidence,
  index,
  copied,
  onCopy,
}: {
  evidence: EvidenceItem;
  index: number;
  copied: boolean;
  onCopy: (evidence: EvidenceItem) => void;
}) {
  const { t, lang } = useI18n();
  const stance = stanceMeta[evidence.stance] ?? stanceMeta.mentions;

  return (
    <article
      className="animate-rise relative overflow-hidden rounded-2xl border border-white/[0.08] bg-white/[0.022] p-4 sm:p-5"
      style={{ animationDelay: `${Math.min(index, 8) * 40}ms` }}
    >
      <span className={cn('absolute inset-y-0 left-0 w-px', stance.bar)} />
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <Badge className={cn(stance.bg, stance.border, stance.text)}>
              <span className={cn('h-1.5 w-1.5 rounded-full', stance.dot)} />
              {t(stance.key)}
            </Badge>
            <span className="inline-flex items-center gap-1 font-mono text-[10px] text-slate-600">
              <Link2 className="h-3 w-3" />
              {evidence.domain}
            </span>
            <span className="text-[10px] text-slate-700">
              {t('detail.strength')} {Math.round(evidence.strength * 100)}%
            </span>
          </div>
          <a
            href={evidence.canonical_url}
            target="_blank"
            rel="noreferrer noopener"
            className="mt-2 block max-w-xl truncate text-[13px] font-medium text-slate-300 underline-offset-4 transition hover:text-amber-200 hover:underline"
            title={evidence.title}
          >
            {evidence.title || evidence.canonical_url}
          </a>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <button
            type="button"
            onClick={() => onCopy(evidence)}
            className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.08] px-2.5 py-1.5 text-[10px] font-semibold text-slate-500 transition hover:border-white/15 hover:text-slate-200"
          >
            {copied ? (
              <Check className="h-3 w-3 text-emerald-400" />
            ) : (
              <Copy className="h-3 w-3" />
            )}
            {copied ? t('detail.copied') : t('detail.copyQuote')}
          </button>
          <a
            href={evidence.canonical_url}
            target="_blank"
            rel="noreferrer noopener"
            className="grid h-7 w-7 place-items-center rounded-lg border border-white/[0.08] text-slate-500 transition hover:border-amber-300/25 hover:text-amber-200"
            aria-label={t('detail.openSource')}
            title={t('detail.openSource')}
          >
            <ExternalLink className="h-3 w-3" />
          </a>
        </div>
      </div>

      <blockquote className="quote-serif relative mt-4 rounded-xl border-l-2 border-amber-400/30 bg-black/20 px-4 py-3 text-[15px] leading-relaxed text-slate-300/90">
        <Quote className="mb-1.5 h-3.5 w-3.5 text-amber-400/40" />
        <span className="whitespace-pre-wrap">{evidence.quote}</span>
      </blockquote>

      <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[9px] text-slate-700">
        <span className="inline-flex items-center gap-1">
          <Cpu className="h-3 w-3" />
          {evidence.model_version}
        </span>
        <span className="inline-flex items-center gap-1">
          <Hash className="h-3 w-3" />
          {evidence.quote_hash.replace(/^sha256:/, '').slice(0, 12)}
        </span>
        <span className="ml-auto inline-flex items-center gap-1">
          <CalendarDays className="h-3 w-3" />
          {formatDate(evidence.created_at, lang)}
        </span>
      </div>
    </article>
  );
}

export function ClaimDrawer({
  summary,
  onClose,
  onDelete,
}: {
  summary: ClaimSummary;
  onClose: () => void;
  onDelete?: (claimId: string) => void;
}) {
  const { t, lang } = useI18n();
  const [detail, setDetail] = useState<ClaimDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copiedQuote, setCopiedQuote] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const value = await fetchClaim(summary.id);
      setDetail(value);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, [summary.id]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const previous = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose();
    }
    window.addEventListener('keydown', onKeyDown);
    return () => {
      document.body.style.overflow = previous;
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [onClose]);

  const kinds = kindMeta[summary.kind] ?? kindMeta.other;
  const claim = detail?.claim ?? summary;
  const evidence = detail?.evidence ?? [];
  const entities = detail?.entities ?? [];

  async function copyQuote(item: EvidenceItem) {
    if (await copyText(item.quote)) {
      setCopiedQuote(item.quote_hash);
      window.setTimeout(() => setCopiedQuote(null), 1800);
    }
  }

  async function copyClaimId() {
    if (await copyText(claim.id)) {
      setCopiedId(true);
      window.setTimeout(() => setCopiedId(false), 1800);
    }
  }

  function exportMarkdown() {
    let md = `# BALD Evidence Ledger Report\n\n`;
    md += `**Twierdzenie:** ${claim.text}\n`;
    md += `**ID Twierdzenia:** \`${claim.id}\`\n`;
    md += `**Rodzaj:** ${claim.kind}\n`;
    md += `**Liczba źródeł:** ${claim.document_count}\n`;
    md += `**Liczba cytatów (CitationGate):** ${claim.evidence_count}\n`;
    md += `**Pierwsze pojawienie:** ${claim.first_seen}\n`;
    md += `**Ostatnie pojawienie:** ${claim.last_seen}\n\n`;
    md += `---\n\n## Zweryfikowane cytaty źródłowe\n\n`;

    for (const [i, e] of evidence.entries()) {
      md += `### ${i + 1}. [${e.title || e.canonical_url}](${e.canonical_url})\n`;
      md += `- **Domena:** \`${e.domain}\`\n`;
      md += `- **Stance:** \`${e.stance}\` (siła: ${Math.round(e.strength * 100)}%)\n`;
      md += `- **Dosłowny cytat:** „${e.quote}”\n`;
      md += `- **Hash cytatu (SHA-256):** \`${e.quote_hash}\`\n\n`;
    }

    if (entities.length > 0) {
      md += `---\n\n## Powiązane podmioty i encje (Public Actors & Entities)\n\n`;
      for (const ent of entities) {
        md += `- **${ent.name}** (${ent.kind})\n`;
      }
      md += `\n`;
    }

    md += `---\n*Wygenerowano przez BALD Search Engine (VIPER STUDIO) · Temat tak, osoba nie.*`;

    const blob = new Blob([md], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `bald-raport-twierdzenia-${shortId(claim.id)}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      <div
        aria-hidden="true"
        onClick={onClose}
        className="animate-fade absolute inset-0 bg-[#020305]/75 backdrop-blur-[2px]"
      />
      <aside
        role="dialog"
        aria-modal="true"
        aria-label={t('detail.title')}
        className="animate-drawer relative flex h-full w-full max-w-3xl flex-col border-l border-white/[0.09] bg-[#080b11] shadow-[-30px_0_90px_rgba(0,0,0,0.55)]"
      >
        <header className="flex items-center gap-3 border-b border-white/[0.07] px-5 py-4 sm:px-7">
          <span className="grid h-9 w-9 place-items-center rounded-xl border border-amber-300/15 bg-amber-400/[0.06]">
            <Quote className="h-4 w-4 text-amber-300/80" />
          </span>
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-slate-200">{t('detail.title')}</h2>
            <p className="mt-0.5 truncate text-[10px] text-slate-600">{t('detail.subtitle')}</p>
          </div>
          <button
            type="button"
            onClick={exportMarkdown}
            title={t('chat.exportReport')}
            className="ml-auto inline-flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-white/[0.025] px-2.5 py-1.5 text-xs font-semibold text-slate-300 transition hover:border-amber-400/40 hover:bg-amber-400/10 hover:text-amber-200"
          >
            <Download className="h-3.5 w-3.5 text-amber-300" />
            <span className="hidden sm:inline">Eksportuj .md</span>
          </button>
          <button
            type="button"
            autoFocus
            onClick={onClose}
            className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-white/[0.08] text-slate-500 transition hover:border-white/15 hover:bg-white/[0.04] hover:text-slate-200"
            aria-label={t('detail.close')}
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div className="scrollbar-thin flex-1 overflow-y-auto overscroll-contain">
          <div className="px-5 py-6 sm:px-7">
            <div className="flex flex-wrap items-center gap-2">
              <span
                className={cn(
                  'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em]',
                  kinds.tone,
                )}
              >
                {t(kinds.key)}
              </span>
              {claim.lang && (
                <Badge tone="neutral">
                  <span className="font-mono">{claim.lang}</span>
                </Badge>
              )}
              <Badge tone="emerald">
                <ShieldCheck className="h-3 w-3" />
                CitationGate
              </Badge>
              <span className="ml-auto text-[10px] text-slate-600">
                {t('detail.evidenceCount', { count: claim.evidence_count })}
              </span>
            </div>

            <h1 className="mt-4 text-balance text-xl font-semibold leading-relaxed tracking-[-0.01em] text-slate-100 sm:text-2xl">
              {claim.text}
            </h1>

            <div className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 text-[10px] text-slate-600">
              <span className="inline-flex items-center gap-1.5">
                <Link2 className="h-3 w-3" />
                {claim.document_count} {t('stats.sources')}
              </span>
              <span className="inline-flex items-center gap-1.5">
                <FileText className="h-3 w-3" />
                {claim.evidence_count} {t('stats.evidence')}
              </span>
              <span className="inline-flex items-center gap-1.5">
                <CalendarDays className="h-3 w-3" />
                {formatDate(claim.last_seen, lang)}
              </span>
            </div>

            <div className="mt-5 flex items-start gap-3 rounded-xl border border-emerald-400/15 bg-emerald-400/[0.045] px-4 py-3">
              <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" />
              <div>
                <p className="text-xs font-semibold text-emerald-200/90">
                  {t('detail.verified')}
                </p>
                <p className="mt-1 text-[11px] leading-relaxed text-emerald-200/45">
                  {t('detail.verifiedBody')}
                </p>
              </div>
            </div>

            <section className="mt-7">
              <div className="mb-3 flex items-center justify-between">
                <h3 className="text-[11px] font-bold uppercase tracking-[0.18em] text-slate-500">
                  {t('detail.quote')}
                </h3>
                <span className="font-mono text-[9px] text-slate-700">
                  {evidence.length} / {claim.evidence_count}
                </span>
              </div>

              {loading && (
                <div className="flex items-center gap-2 rounded-2xl border border-white/[0.07] px-4 py-8 text-xs text-slate-500">
                  <Spinner className="text-amber-300" />
                  {t('detail.loading')}
                </div>
              )}

              {error && !loading && (
                <div className="rounded-2xl border border-rose-400/20 bg-rose-400/[0.05] px-4 py-4">
                  <div className="flex items-center gap-2 text-xs font-semibold text-rose-200">
                    <AlertTriangle className="h-4 w-4" />
                    {t('detail.error')}
                  </div>
                  <p className="mt-1 font-mono text-[10px] leading-relaxed text-rose-300/60">
                    {error}
                  </p>
                  <button
                    type="button"
                    onClick={() => void load()}
                    className="mt-3 rounded-lg border border-rose-300/20 px-3 py-1.5 text-[10px] font-semibold text-rose-200 transition hover:bg-rose-400/10"
                  >
                    {t('error.retry')}
                  </button>
                </div>
              )}

              {!loading && !error && evidence.length === 0 && (
                <p className="rounded-2xl border border-dashed border-white/10 px-4 py-8 text-center text-xs text-slate-600">
                  {t('detail.emptyEvidence')}
                </p>
              )}

              <div className="space-y-3">
                {evidence.map((item, index) => (
                  <EvidenceCard
                    key={`${item.quote_hash}:${item.document_id}`}
                    evidence={item}
                    index={index}
                    copied={copiedQuote === item.quote_hash}
                    onCopy={copyQuote}
                  />
                ))}
              </div>
            </section>

            <section className="mt-8">
              <h3 className="mb-3 text-[11px] font-bold uppercase tracking-[0.18em] text-slate-500">
                {t('detail.entities')}
              </h3>
              {entities.length === 0 ? (
                <p className="text-[11px] text-slate-600">{t('detail.noEntities')}</p>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {entities.map((entity) => {
                    const Icon = entityIcons[entity.kind] ?? CircleHelp;
                    return (
                      <span
                        key={entity.id}
                        className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.08] bg-white/[0.025] px-2.5 py-1.5 text-[11px] text-slate-400"
                      >
                        <Icon className="h-3 w-3 text-violet-300/70" />
                        {entity.name}
                        <span className="text-[9px] uppercase tracking-[0.1em] text-slate-700">
                          {entity.kind}
                        </span>
                      </span>
                    );
                  })}
                </div>
              )}
            </section>

            <div className="mt-8 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-white/[0.06] bg-black/20 px-4 py-3">
              <div className="min-w-0">
                <p className="text-[9px] font-semibold uppercase tracking-[0.16em] text-slate-600">
                  {t('detail.claimId')}
                </p>
                <p className="mt-1 truncate font-mono text-[10px] text-slate-500" title={claim.id}>
                  {shortId(claim.id)}
                  <span className="text-slate-700"> · sha256</span>
                </p>
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => void copyClaimId()}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.08] px-3 py-1.5 text-[10px] font-semibold text-slate-500 transition hover:border-white/15 hover:text-slate-200"
                >
                  {copiedId ? (
                    <Check className="h-3 w-3 text-emerald-400" />
                  ) : (
                    <Copy className="h-3 w-3" />
                  )}
                  {copiedId ? t('detail.copied') : t('claims.copy')}
                </button>
                {onDelete && (
                  <button
                    type="button"
                    onClick={() => {
                      onDelete(claim.id);
                      onClose();
                    }}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-rose-500/25 bg-rose-500/10 px-3 py-1.5 text-[10px] font-semibold text-rose-300 transition hover:bg-rose-500/20"
                  >
                    <Trash2 className="h-3 w-3" />
                    <span>{t('claims.delete')}</span>
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>

        <footer className="flex items-center gap-2 border-t border-white/[0.07] px-5 py-3 text-[9px] text-slate-700 sm:px-7">
          <Sparkles className="h-3 w-3 text-amber-500/50" />
          {t('footer.guard')}
          <span className="ml-auto font-mono">{t('footer.version')}</span>
        </footer>
      </aside>
    </div>
  );
}
