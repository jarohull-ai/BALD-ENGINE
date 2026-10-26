import { Bot, Coins, HelpCircle, Languages, MessageSquareText, Settings2, ShieldCheck, Sparkles } from 'lucide-react';
import { useI18n } from '../i18n';
import { cn } from '../lib';
import type { WalletInfo } from '../types';

export type Screen = 'chat' | 'claims' | 'bots' | 'settings';

export function Header({
  screen,
  onNavigate,
  onOpenHelp,
  wallet,
  onOpenWallet,
}: {
  screen: Screen;
  onNavigate: (screen: Screen) => void;
  onOpenHelp?: () => void;
  wallet?: WalletInfo | null;
  onOpenWallet?: () => void;
}) {
  const { lang, setLang, t } = useI18n();

  return (
    <header className="sticky top-0 z-30 border-b border-white/[0.07] bg-[#07090d]/85 backdrop-blur-xl">
      <div className="mx-auto flex h-16 max-w-7xl items-center gap-4 px-4 sm:px-6 lg:px-8">
        <button
          type="button"
          onClick={() => onNavigate('claims')}
          className="group flex min-w-0 items-center gap-3 text-left"
          aria-label="BALD"
        >
          <span className="relative grid h-9 w-9 shrink-0 place-items-center overflow-hidden rounded-xl border border-amber-300/20 bg-amber-400/[0.07] shadow-[0_0_24px_rgba(251,191,36,0.1)]">
            <img
              src="/logo.png"
              alt=""
              className="h-7 w-7 object-contain transition duration-300 group-hover:scale-105"
            />
          </span>
          <span className="min-w-0">
            <span className="flex items-baseline gap-2">
              <span className="text-[17px] font-bold tracking-[0.18em] text-slate-100">BALD</span>
              <span className="hidden text-[10px] font-medium uppercase tracking-[0.18em] text-slate-600 sm:inline">
                {t('brand.tagline')}
              </span>
            </span>
            <span className="mt-0.5 hidden items-center gap-1.5 text-[10px] text-slate-600 sm:flex">
              <ShieldCheck className="h-3 w-3 text-emerald-400/70" />
              {t('shell.guard')}
            </span>
          </span>
        </button>

        <nav className="ml-auto flex items-center gap-1 rounded-xl border border-white/[0.07] bg-white/[0.025] p-1">
          <button
            type="button"
            onClick={() => onNavigate('chat')}
            className={cn(
              'flex items-center gap-2 rounded-lg px-3 py-2 text-xs font-semibold transition',
              screen === 'chat'
                ? 'bg-amber-400/10 text-amber-200'
                : 'text-slate-500 hover:bg-white/[0.04] hover:text-slate-300',
            )}
          >
            <MessageSquareText className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">{t('nav.chat')}</span>
          </button>
          <button
            type="button"
            onClick={() => onNavigate('claims')}
            className={cn(
              'flex items-center gap-2 rounded-lg px-3 py-2 text-xs font-semibold transition',
              screen === 'claims'
                ? 'bg-amber-400/10 text-amber-200'
                : 'text-slate-500 hover:bg-white/[0.04] hover:text-slate-300',
            )}
          >
            <Sparkles className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">{t('nav.claims')}</span>
          </button>
          <button
            type="button"
            onClick={() => onNavigate('bots')}
            className={cn(
              'flex items-center gap-2 rounded-lg px-3 py-2 text-xs font-semibold transition',
              screen === 'bots'
                ? 'bg-amber-400/10 text-amber-200'
                : 'text-slate-500 hover:bg-white/[0.04] hover:text-slate-300',
            )}
          >
            <Bot className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">{t('nav.bots')}</span>
          </button>
          <button
            type="button"
            onClick={() => onNavigate('settings')}
            className={cn(
              'flex items-center gap-2 rounded-lg px-3 py-2 text-xs font-semibold transition',
              screen === 'settings'
                ? 'bg-amber-400/10 text-amber-200'
                : 'text-slate-500 hover:bg-white/[0.04] hover:text-slate-300',
            )}
          >
            <Settings2 className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">{t('nav.settings')}</span>
          </button>
        </nav>

        <div className="hidden items-center gap-2 xl:flex">
          <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-400/15 bg-emerald-400/[0.06] px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-emerald-300/80">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.8)]" />
            {t('shell.local')}
          </span>
          <span className="rounded-full border border-white/[0.07] px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-slate-600">
            {t('shell.phase')}
          </span>
        </div>

        {onOpenWallet && (
          <button
            type="button"
            onClick={onOpenWallet}
            aria-label="Portfel Bald$"
            title={`Stan portfela: ${wallet?.formatted ?? '1 000.00 Bald$'}. Kliknij, aby otworzyć portfel.`}
            className={cn(
              'flex items-center gap-1.5 rounded-lg border px-2.5 py-2 text-[11px] font-semibold transition shadow-sm',
              (wallet?.balance ?? 1000) < 0.25
                ? 'border-rose-500/35 bg-rose-500/15 text-rose-300 hover:bg-rose-500/25 animate-pulse'
                : 'border-amber-400/30 bg-amber-400/[0.08] text-amber-300 hover:bg-amber-400/15 hover:border-amber-400/50',
            )}
          >
            <Coins className="h-3.5 w-3.5 text-amber-400" />
            <span className="font-mono font-bold tracking-tight">
              {wallet ? `${wallet.balance.toFixed(2)} Bald$` : '1 000.00 Bald$'}
            </span>
          </button>
        )}

        <button
          type="button"
          onClick={onOpenHelp}
          aria-label="Pomoc & Skróty (/help)"
          title="Pomoc & Skróty (/help)"
          className="flex items-center gap-1.5 rounded-lg border border-amber-500/25 bg-amber-500/10 px-2.5 py-2 text-[11px] font-semibold text-amber-300 transition hover:bg-amber-500/20"
        >
          <HelpCircle className="h-3.5 w-3.5" />
          <span className="hidden sm:inline">Pomoc</span>
        </button>

        <button
          type="button"
          onClick={() => setLang(lang === 'pl' ? 'en' : 'pl')}
          aria-label={t('shell.language')}
          title={t('shell.language')}
          className="flex items-center gap-1.5 rounded-lg border border-white/[0.07] bg-white/[0.02] px-2.5 py-2 text-[11px] font-semibold text-slate-500 transition hover:border-white/15 hover:text-slate-200"
        >
          <Languages className="h-3.5 w-3.5" />
          {lang.toUpperCase()}
        </button>
      </div>
    </header>
  );
}
