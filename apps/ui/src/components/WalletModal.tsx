import { useState } from 'react';
import {
  Coins,
  CheckCircle2,
  KeyRound,
  ShieldCheck,
  Sparkles,
  AlertTriangle,
  X,
} from 'lucide-react';
import { redeemVoucher } from '../api';
import type { WalletInfo } from '../types';

interface WalletModalProps {
  isOpen: boolean;
  onClose: () => void;
  wallet: WalletInfo | null;
  onWalletUpdated: (wallet: WalletInfo) => void;
}

export function WalletModal({ isOpen, onClose, wallet, onWalletUpdated }: WalletModalProps) {
  const [code, setCode] = useState('');
  const [loading, setLoading] = useState(false);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!isOpen) return null;

  async function handleRedeem(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = code.trim();
    if (!trimmed || loading) return;

    setLoading(true);
    setErrorMsg(null);
    setSuccessMsg(null);

    try {
      const updated = await redeemVoucher(trimmed);
      onWalletUpdated(updated);
      setSuccessMsg(`Pomyślnie aktywowano kod! Do Twojego salda dodano +10 000.00 Bald$ (aktualny stan: ${updated.formatted}).`);
      setCode('');
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  const balance = wallet?.balance ?? 1000.0;
  const queriesRemaining = Math.floor(balance / 0.25);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-md animate-in fade-in duration-200">
      <div className="relative w-full max-w-lg rounded-2xl border border-amber-500/35 bg-neutral-950/95 shadow-2xl text-neutral-100 overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-white/10 px-6 py-4 bg-gradient-to-r from-amber-500/10 via-transparent to-transparent">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-amber-500/15 text-amber-400 border border-amber-500/30 shadow-[0_0_20px_rgba(245,158,11,0.2)]">
              <Coins className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold text-white flex items-center gap-2">
                Portfel Bald$ & Kredyty
                <span className="rounded-md bg-amber-500/20 px-2 py-0.5 text-[10px] font-mono text-amber-300 border border-amber-500/30">
                  VIP Sovereign
                </span>
              </h2>
              <p className="text-xs text-neutral-400">
                Pula kredytów analitycznych silnika badawczego
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1.5 text-neutral-400 hover:text-white hover:bg-white/10 transition"
            aria-label="Zamknij"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Balance Card */}
          <div className="rounded-xl border border-amber-400/25 bg-amber-500/[0.04] p-5 relative overflow-hidden">
            <div className="absolute top-0 right-0 p-4 opacity-10 pointer-events-none">
              <Coins className="h-28 w-28 text-amber-300" />
            </div>

            <div className="flex flex-col sm:flex-row sm:items-baseline justify-between gap-2 relative z-10">
              <div>
                <span className="text-[11px] font-semibold uppercase tracking-wider text-amber-300/80">
                  Dostępne saldo analityczne:
                </span>
                <div className="mt-1 flex items-baseline gap-2">
                  <span className="font-mono text-3xl font-extrabold text-white tracking-tight">
                    {balance.toFixed(2)}
                  </span>
                  <span className="font-semibold text-amber-400 text-lg">Bald$</span>
                </div>
              </div>
              <div className="text-left sm:text-right">
                <span className="text-[11px] text-neutral-400">Pozostało badań:</span>
                <p className="font-mono text-sm font-semibold text-emerald-400">
                  ~{queriesRemaining.toLocaleString()} zapytań
                </p>
              </div>
            </div>

            <div className="mt-4 pt-3 border-t border-amber-500/15 flex flex-wrap items-center justify-between text-[11px] text-neutral-400 gap-2">
              <span className="flex items-center gap-1.5">
                <Sparkles className="h-3.5 w-3.5 text-amber-400/70" />
                Koszt: <strong className="text-neutral-200">0.25 Bald$</strong> / zapytanie OSINT
              </span>
              <span>
                Wykonano zapytań: <strong className="text-neutral-200">{wallet?.queries_made ?? 0}</strong>
              </span>
            </div>
          </div>

          {/* Code Redeem Form */}
          <div className="space-y-3">
            <label className="block text-xs font-semibold text-neutral-300 flex items-center gap-1.5">
              <KeyRound className="h-3.5 w-3.5 text-amber-400" />
              Doładuj konto jednorazowym kodem licencyjnym (Voucher VIP):
            </label>

            <form onSubmit={handleRedeem} className="flex gap-2">
              <input
                type="text"
                value={code}
                onChange={(e) => setCode(e.target.value)}
                placeholder="np. BALD-VIP-C2CF-C158"
                disabled={loading}
                className="flex-1 rounded-xl border border-white/15 bg-black/60 px-3.5 py-2.5 font-mono text-xs text-white placeholder:text-neutral-600 focus:border-amber-400 focus:outline-none focus:ring-1 focus:ring-amber-400 uppercase tracking-wider disabled:opacity-50"
              />
              <button
                type="submit"
                disabled={loading || !code.trim()}
                className="rounded-xl border border-amber-400/35 bg-amber-400 px-4 py-2.5 text-xs font-bold text-amber-950 hover:bg-amber-300 transition shadow-[0_4px_16px_rgba(251,191,36,0.2)] disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {loading ? 'Weryfikacja...' : 'Zrealizuj'}
              </button>
            </form>

            {successMsg && (
              <div className="flex items-start gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/10 p-3 text-xs text-emerald-300 animate-in fade-in">
                <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400 mt-0.5" />
                <span>{successMsg}</span>
              </div>
            )}

            {errorMsg && (
              <div className="flex items-start gap-2 rounded-xl border border-rose-500/30 bg-rose-500/10 p-3 text-xs text-rose-300 animate-in fade-in">
                <AlertTriangle className="h-4 w-4 shrink-0 text-rose-400 mt-0.5" />
                <span>{errorMsg}</span>
              </div>
            )}
          </div>

          {/* Info note */}
          <div className="rounded-xl border border-white/5 bg-white/[0.02] p-3.5 text-[11px] leading-relaxed text-neutral-400 space-y-1.5">
            <p className="flex items-center gap-1.5 font-semibold text-neutral-300">
              <ShieldCheck className="h-3.5 w-3.5 text-amber-400" />
              Czym są kredyty Bald$?
            </p>
            <p>
              Kredyty Bald$ reprezentują jednostki obliczeniowe dla zapytań śledczych, weryfikacji cytatów (CitationGate) oraz analiz Sędziego Wiarygodności.
              Wersja VIP dla twórców i testerów zawiera nielimitowane doładowania kodami licencyjnymi.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
