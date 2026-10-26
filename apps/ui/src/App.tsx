import { useCallback, useEffect, useState } from 'react';
import { fetchClaims } from './api';
import { BotsView } from './components/BotsView';
import { ChatView } from './components/ChatView';
import { ClaimDrawer } from './components/ClaimDrawer';
import { ClaimsView } from './components/ClaimsView';
import { Header, type Screen } from './components/Header';
import { HelpModal } from './components/HelpModal';
import { SettingsView } from './components/SettingsView';
import { StatsBar } from './components/StatsBar';
import type { ClaimSummary, LedgerStats } from './types';

const emptyStats: LedgerStats = {
  claims: 0,
  evidence: 0,
  entities: 0,
  documents_with_evidence: 0,
};

export default function App() {
  const [screen, setScreen] = useState<Screen>('chat');
  const [claims, setClaims] = useState<ClaimSummary[]>([]);
  const [stats, setStats] = useState<LedgerStats>(emptyStats);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ClaimSummary | null>(null);
  const [isHelpOpen, setIsHelpOpen] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetchClaims(200);
      setClaims(response.claims);
      setStats(response.stats);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const navigate = useCallback((next: Screen) => {
    setScreen(next);
    setSelected(null);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }, []);

  return (
    <div className="relative min-h-screen overflow-x-hidden">
      <div className="bg-grid pointer-events-none fixed inset-0 opacity-60" aria-hidden="true" />
      <div
        className="pointer-events-none fixed left-1/2 top-0 h-[30rem] w-[70rem] -translate-x-1/2 rounded-full bg-amber-400/[0.035] blur-3xl"
        aria-hidden="true"
      />

      <Header screen={screen} onNavigate={navigate} onOpenHelp={() => setIsHelpOpen(true)} />

      <main className="relative mx-auto max-w-7xl px-4 py-7 sm:px-6 sm:py-9 lg:px-8">
        {screen === 'chat' ? (
          <ChatView
            onOpenClaim={setSelected}
            onOpenHelp={() => setIsHelpOpen(true)}
            onSwitchToLedger={() => {
              void refresh();
              navigate('claims');
            }}
          />
        ) : screen === 'claims' ? (
          <div className="space-y-6">
            <StatsBar stats={stats} />
            <ClaimsView
              claims={claims}
              loading={loading}
              error={error}
              onRefresh={() => void refresh()}
              onOpen={setSelected}
            />
          </div>
        ) : screen === 'bots' ? (
          <BotsView
            onNavigateToLedger={() => {
              void refresh();
              navigate('claims');
            }}
          />
        ) : (
          <SettingsView />
        )}
      </main>

      {selected && <ClaimDrawer summary={selected} onClose={() => setSelected(null)} />}
      <HelpModal isOpen={isHelpOpen} onClose={() => setIsHelpOpen(false)} />
    </div>
  );
}
