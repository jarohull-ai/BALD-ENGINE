import type { ReactNode } from 'react';
import { KeyRound, LoaderCircle } from 'lucide-react';
import { useI18n } from '../i18n';
import { cn } from '../lib';

export const inputClass =
  'w-full rounded-xl border border-white/10 bg-black/25 px-3.5 py-2.5 text-sm text-slate-100 shadow-inner shadow-black/20 placeholder:text-slate-600 transition focus:border-amber-400/40 focus:bg-black/35 focus:ring-2 focus:ring-amber-400/10 disabled:cursor-not-allowed disabled:opacity-45';

export const selectClass =
  'w-full appearance-none rounded-xl border border-white/10 bg-[#0b0f16] px-3.5 py-2.5 text-sm text-slate-100 transition focus:border-amber-400/40 focus:ring-2 focus:ring-amber-400/10 disabled:cursor-not-allowed disabled:opacity-45';

export const labelClass =
  'mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.14em] text-slate-500';

export function Spinner({ className }: { className?: string }) {
  return <LoaderCircle className={cn('h-4 w-4 animate-spin', className)} aria-hidden="true" />;
}

export function Badge({
  children,
  tone = 'neutral',
  className,
}: {
  children: ReactNode;
  tone?: 'neutral' | 'amber' | 'emerald' | 'cyan' | 'rose';
  className?: string;
}) {
  const tones = {
    neutral: 'border-white/10 bg-white/[0.04] text-slate-400',
    amber: 'border-amber-400/20 bg-amber-400/10 text-amber-200',
    emerald: 'border-emerald-400/20 bg-emerald-400/10 text-emerald-200',
    cyan: 'border-cyan-400/20 bg-cyan-400/10 text-cyan-200',
    rose: 'border-rose-400/20 bg-rose-400/10 text-rose-200',
  };
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em]',
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

export function KeySourceBadge({ source }: { source: string }) {
  const { t } = useI18n();
  if (source === 'env') {
    return (
      <Badge tone="cyan">
        <KeyRound className="h-3 w-3" />
        {t('settings.keyFromEnv')}
      </Badge>
    );
  }
  if (source === 'config') {
    return (
      <Badge tone="emerald">
        <KeyRound className="h-3 w-3" />
        {t('settings.keyFromConfig')}
      </Badge>
    );
  }
  return <Badge tone="neutral">{t('settings.keyMissing')}</Badge>;
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        'relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border transition',
        checked
          ? 'border-amber-300/30 bg-amber-400/80 shadow-[0_0_18px_rgba(251,191,36,0.22)]'
          : 'border-white/10 bg-white/10',
        disabled && 'cursor-not-allowed opacity-40',
      )}
    >
      <span
        className={cn(
          'inline-block h-4 w-4 transform rounded-full bg-white shadow-sm transition',
          checked ? 'translate-x-6' : 'translate-x-1',
        )}
      />
    </button>
  );
}

export function SectionHeading({
  eyebrow,
  title,
  body,
  action,
}: {
  eyebrow?: string;
  title: string;
  body?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-4">
      <div className="max-w-3xl">
        {eyebrow && (
          <p className="mb-2 text-[10px] font-bold uppercase tracking-[0.22em] text-amber-300/80">
            {eyebrow}
          </p>
        )}
        <h2 className="text-lg font-semibold tracking-tight text-slate-100">{title}</h2>
        {body && <p className="mt-2 text-sm leading-relaxed text-slate-500">{body}</p>}
      </div>
      {action}
    </div>
  );
}

export function CardShell({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      className={cn(
        'rounded-2xl border border-white/[0.08] bg-white/[0.025] shadow-[0_20px_70px_rgba(0,0,0,0.18)] backdrop-blur-sm',
        className,
      )}
    >
      {children}
    </section>
  );
}
