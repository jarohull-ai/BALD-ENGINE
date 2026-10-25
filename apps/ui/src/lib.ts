import type { MessageKey } from './i18n';
import type { ClaimKind, Stance } from './types';

export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(' ');
}

export function formatDate(iso: string, lang: 'pl' | 'en'): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  try {
    return new Intl.DateTimeFormat(lang === 'pl' ? 'pl-PL' : 'en-GB', {
      day: '2-digit',
      month: 'short',
      year: 'numeric',
    }).format(date);
  } catch {
    return date.toISOString().slice(0, 10);
  }
}

export function shortId(id: string): string {
  const raw = id.replace(/^sha256:/, '');
  return raw.slice(0, 12);
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const area = document.createElement('textarea');
      area.value = text;
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      const ok = document.execCommand('copy');
      area.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

interface BadgeMeta {
  key: MessageKey;
  dot: string;
  text: string;
  bg: string;
  border: string;
  bar: string;
}

export const stanceMeta: Record<Stance, BadgeMeta> = {
  reports: {
    key: 'stance.reports',
    dot: 'bg-cyan-400',
    text: 'text-cyan-200',
    bg: 'bg-cyan-400/10',
    border: 'border-cyan-400/20',
    bar: 'bg-cyan-400',
  },
  endorses: {
    key: 'stance.endorses',
    dot: 'bg-amber-400',
    text: 'text-amber-200',
    bg: 'bg-amber-400/10',
    border: 'border-amber-400/20',
    bar: 'bg-amber-400',
  },
  debunks: {
    key: 'stance.debunks',
    dot: 'bg-emerald-400',
    text: 'text-emerald-200',
    bg: 'bg-emerald-400/10',
    border: 'border-emerald-400/20',
    bar: 'bg-emerald-400',
  },
  mentions: {
    key: 'stance.mentions',
    dot: 'bg-slate-400',
    text: 'text-slate-300',
    bg: 'bg-slate-400/10',
    border: 'border-slate-400/20',
    bar: 'bg-slate-400',
  },
};

export const stanceOrder: Stance[] = ['reports', 'endorses', 'debunks', 'mentions'];

export const kindMeta: Record<ClaimKind, { key: MessageKey; tone: string }> = {
  fact: { key: 'kind.fact', tone: 'text-sky-300 border-sky-400/20 bg-sky-400/10' },
  causal: { key: 'kind.causal', tone: 'text-violet-300 border-violet-400/20 bg-violet-400/10' },
  concealment: {
    key: 'kind.concealment',
    tone: 'text-rose-300 border-rose-400/20 bg-rose-400/10',
  },
  conspiracy: {
    key: 'kind.conspiracy',
    tone: 'text-orange-300 border-orange-400/20 bg-orange-400/10',
  },
  prophecy: {
    key: 'kind.prophecy',
    tone: 'text-fuchsia-300 border-fuchsia-400/20 bg-fuchsia-400/10',
  },
  other: { key: 'kind.other', tone: 'text-slate-300 border-slate-400/20 bg-slate-400/10' },
};
