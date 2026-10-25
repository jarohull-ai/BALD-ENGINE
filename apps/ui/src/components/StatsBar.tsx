import { BookOpenText, Database, FileText, Network } from 'lucide-react';
import { useI18n } from '../i18n';
import type { LedgerStats } from '../types';

export function StatsBar({ stats }: { stats: LedgerStats }) {
  const { t, lang } = useI18n();
  const nf = new Intl.NumberFormat(lang === 'pl' ? 'pl-PL' : 'en-GB');
  const items = [
    { key: 'stats.claims', value: stats.claims, icon: BookOpenText, tone: 'text-amber-300' },
    { key: 'stats.evidence', value: stats.evidence, icon: FileText, tone: 'text-cyan-300' },
    {
      key: 'stats.sources',
      value: stats.documents_with_evidence,
      icon: Network,
      tone: 'text-emerald-300',
    },
    { key: 'stats.entities', value: stats.entities, icon: Database, tone: 'text-violet-300' },
  ] as const;

  return (
    <div className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-white/[0.07] bg-white/[0.07] sm:grid-cols-4">
      {items.map((item) => {
        const Icon = item.icon;
        return (
          <div key={item.key} className="bg-[#090c12]/95 px-4 py-3.5">
            <div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-slate-600">
              <Icon className={`h-3.5 w-3.5 ${item.tone}`} />
              {t(item.key)}
            </div>
            <div className="mt-1.5 font-mono text-xl font-semibold tracking-tight text-slate-200">
              {nf.format(item.value)}
            </div>
          </div>
        );
      })}
    </div>
  );
}
