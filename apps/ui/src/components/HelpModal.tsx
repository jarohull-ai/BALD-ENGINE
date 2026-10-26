import { useState } from 'react';
import {
  BookOpen,
  CheckCircle2,
  Command,
  Lightbulb,
  Scale,
  ShieldAlert,
  Sparkles,
  X,
} from 'lucide-react';
import { cn } from '../lib';

interface HelpModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function HelpModal({ isOpen, onClose }: HelpModalProps) {
  const [activeTab, setActiveTab] = useState<'cli' | 'credibility' | 'ai' | 'guard' | 'bots'>('cli');

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-200">
      <div className="relative w-full max-w-3xl rounded-2xl border border-amber-500/30 bg-neutral-950/95 shadow-2xl text-neutral-100 overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-white/10 px-6 py-4 bg-white/[0.02]">
          <div className="flex items-center gap-3">
            <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-amber-500/10 text-amber-400 border border-amber-500/25">
              <BookOpen className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold text-white flex items-center gap-2">
                Instrukcja obsługi & Przewodnik BALD
                <span className="rounded-md bg-amber-500/15 px-2 py-0.5 text-[11px] font-mono text-amber-300">
                  v0.1
                </span>
              </h2>
              <p className="text-xs text-neutral-400">
                Krótki, merytoryczny opis funkcji silnika badawczego i weryfikacji faktów
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1.5 text-neutral-400 hover:text-white hover:bg-white/10 transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Navigation Tabs */}
        <div className="flex border-b border-white/10 bg-neutral-900/50 px-6 gap-2 overflow-x-auto py-2">
          <button
            onClick={() => setActiveTab('cli')}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all whitespace-nowrap',
              activeTab === 'cli'
                ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                : 'text-neutral-400 hover:text-neutral-200 hover:bg-white/5',
            )}
          >
            <Command className="h-3.5 w-3.5" />
            Komendy & Skróty
          </button>
          <button
            onClick={() => setActiveTab('credibility')}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all whitespace-nowrap',
              activeTab === 'credibility'
                ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                : 'text-neutral-400 hover:text-neutral-200 hover:bg-white/5',
            )}
          >
            <Scale className="h-3.5 w-3.5" />
            Sędzia Wiarygodności
          </button>
          <button
            onClick={() => setActiveTab('ai')}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all whitespace-nowrap',
              activeTab === 'ai'
                ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                : 'text-neutral-400 hover:text-neutral-200 hover:bg-white/5',
            )}
          >
            <Sparkles className="h-3.5 w-3.5" />
            Silnik AI & Jazda Próbna
          </button>
          <button
            onClick={() => setActiveTab('guard')}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all whitespace-nowrap',
              activeTab === 'guard'
                ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                : 'text-neutral-400 hover:text-neutral-200 hover:bg-white/5',
            )}
          >
            <ShieldAlert className="h-3.5 w-3.5" />
            Person Guard (Zasady)
          </button>
        </div>

        {/* Tab Content */}
        <div className="p-6 overflow-y-auto space-y-4 text-xs leading-relaxed text-neutral-300">
          {activeTab === 'cli' && (
            <div className="space-y-4">
              <div className="rounded-xl border border-white/10 bg-white/[0.02] p-4 space-y-3">
                <h3 className="text-sm font-semibold text-amber-400 flex items-center gap-2">
                  <Command className="h-4 w-4" />
                  Szybkie komendy wpisywane w czacie / wyszukiwarce:
                </h3>
                <div className="grid gap-2.5 font-mono">
                  <div className="flex items-start gap-3 rounded-lg bg-neutral-900/80 p-2.5 border border-white/5">
                    <span className="text-amber-400 font-bold whitespace-nowrap">/help</span>
                    <span className="text-neutral-300 font-sans">
                      Otwiera niniejszy samouczek i instrukcję obsługi skrótów.
                    </span>
                  </div>
                  <div className="flex items-start gap-3 rounded-lg bg-neutral-900/80 p-2.5 border border-white/5">
                    <span className="text-amber-400 font-bold whitespace-nowrap">https://...</span>
                    <span className="text-neutral-300 font-sans">
                      Wklejenie bezpośredniego linku uruchamia automatyczny Ingest artykułu: ekstrakcję
                      faktów, rygorystyczne sprawdzanie cytatów dosłownych (CitationGate) i zapis do
                      Rejestru Dowodowego.
                    </span>
                  </div>
                  <div className="flex items-start gap-3 rounded-lg bg-neutral-900/80 p-2.5 border border-white/5">
                    <span className="text-amber-400 font-bold whitespace-nowrap">Enter / Shift+Enter</span>
                    <span className="text-neutral-300 font-sans">
                      <strong className="text-white">Enter</strong> wysyła zapytanie lub badanie sieci;{' '}
                      <strong className="text-white">Shift + Enter</strong> przechodzi do nowej linii.
                    </span>
                  </div>
                </div>
              </div>

              <div className="rounded-xl border border-white/10 bg-white/[0.02] p-4 space-y-2">
                <h3 className="text-sm font-semibold text-neutral-200">Krótki opis zakładek:</h3>
                <ul className="space-y-1.5 list-disc list-inside text-neutral-400">
                  <li>
                    <strong className="text-neutral-200">Asystent & Sieć:</strong> Konwersacyjny czat
                    z badaniem sieci na żywo (Perplexity OSINT), odnośnikami [1], [2] i weryfikacją faktów.
                  </li>
                  <li>
                    <strong className="text-neutral-200">Rejestr Dowodowy (Ledger):</strong> Baza wszystkich
                    potwierdzonych twierdzeń, cytatów źródłowych, osób publicznych i instytucji.
                  </li>
                  <li>
                    <strong className="text-neutral-200">Roboty Badawcze (Bots):</strong> Autonomiczne
                    roboty cyklicznie badające wybrane zagadnienia w tle bez Twojej obecności.
                  </li>
                  <li>
                    <strong className="text-neutral-200">Ustawienia:</strong> Zarządzanie silnikiem AI,
                    szperaczem modeli, SearXNG, Brave Search oraz ochrona kluczy API.
                  </li>
                </ul>
              </div>
            </div>
          )}

          {activeTab === 'credibility' && (
            <div className="space-y-4">
              <div className="rounded-xl border border-emerald-500/20 bg-emerald-950/15 p-4 space-y-2">
                <h3 className="text-sm font-semibold text-emerald-400 flex items-center gap-2">
                  <CheckCircle2 className="h-4 w-4" />
                  Jak działa Sędzia Wiarygodności (Evidence & Credibility Judge)?
                </h3>
                <p className="text-neutral-300">
                  W odróżnieniu od komercyjnych wyszukiwarek, BALD nie ocenia źródeł na podstawie
                  budżetów reklamowych czy pozycjonowania SEO. Każde źródło i odpowiedź otrzymuje
                  matematyczny wskaźnik wiarygodności <strong>0–100%</strong> oparty na:
                </p>
              </div>

              <div className="grid gap-3 sm:grid-cols-2">
                <div className="rounded-xl border border-white/10 bg-neutral-900/60 p-3.5 space-y-1.5">
                  <div className="font-semibold text-amber-300 flex items-center gap-1.5">
                    <span>1. Klasyfikacja autorytetu domeny (Tier T1–T5)</span>
                  </div>
                  <p className="text-neutral-400 text-[11px]">
                    <strong>T1 (95 pkt):</strong> Źródła instytucjonalne, rządowe i unijne (.gov, .gov.pl, sejm, .edu, un.org).<br />
                    <strong>T2 (85 pkt):</strong> Recenzowane encyklopedie, repozytoria kodu i dokumentacje (Wikipedia, ArXiv, GitHub).<br />
                    <strong>T3 (70 pkt):</strong> Uznane media prasowe i branżowe.<br />
                    <strong>T4/T5 (35-55 pkt):</strong> Portale ogólne, fora i social media.
                  </p>
                </div>

                <div className="rounded-xl border border-white/10 bg-neutral-900/60 p-3.5 space-y-1.5">
                  <div className="font-semibold text-amber-300 flex items-center gap-1.5">
                    <span>2. Gęstość faktograficzna & cytowania</span>
                  </div>
                  <p className="text-neutral-400 text-[11px]">
                    Algorytm nagradza obecność konkretnych dat (lat), liczb, wartości procentowych,
                    walut oraz dosłownych cytatów w cudzysłowach, odrzucając ogólnikowy marketing.
                  </p>
                </div>

                <div className="rounded-xl border border-white/10 bg-neutral-900/60 p-3.5 space-y-1.5">
                  <div className="font-semibold text-amber-300 flex items-center gap-1.5">
                    <span>3. Niezależna korelacja dowodowa</span>
                  </div>
                  <p className="text-neutral-400 text-[11px]">
                    Gdy wiele niezależnych domen potwierdza te same fakty, wskaźnik rośnie.
                    Pojedyncze, niepotwierdzone doniesienia otrzymują status &quot;Wymaga weryfikacji&quot;.
                  </p>
                </div>

                <div className="rounded-xl border border-white/10 bg-neutral-900/60 p-3.5 space-y-1.5">
                  <div className="font-semibold text-amber-300 flex items-center gap-1.5">
                    <span>4. Zgodność z Rejestrem BALD</span>
                  </div>
                  <p className="text-neutral-400 text-[11px]">
                    Informacje zgodne z wcześniej zweryfikowanymi i zapisanymi w lokalnym ledgerze
                    twierdzeniami otrzymują dodatkowy bonus wiarygodności.
                  </p>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'ai' && (
            <div className="space-y-4">
              <div className="rounded-xl border border-amber-500/25 bg-amber-950/15 p-4 space-y-2">
                <h3 className="text-sm font-semibold text-amber-400 flex items-center gap-2">
                  <Sparkles className="h-4 w-4" />
                  Wbudowany silnik AI (Candle / Pure Rust)
                </h3>
                <p className="text-neutral-300">
                  BALD posiada wbudowany, natywny silnik inferencji LLM napisany w czystym Rust.
                  Działa od razu po instalacji — bez potrzeby instalowania Pythona, C++ cmake czy
                  uruchamiania zewnętrznej Ollamy. Modele (np. Qwen 2.5) działają lokalnie na CPU.
                </p>
              </div>

              <div className="rounded-xl border border-white/10 bg-neutral-900/60 p-4 space-y-2.5">
                <h4 className="font-semibold text-white">Szperacz Nowych Modeli AI (Scout Bot)</h4>
                <p className="text-neutral-400 text-[11px]">
                  Rynek modeli AI rozwija się w zawrotnym tempie. Wbudowany w BALD Szperacz cyklicznie
                  (co 7, 14 lub 30 dni) sprawdza repozytoria pod kątem nowszych, zoptymalizowanych modeli
                  poniżej 3 miliardów parametrów, które oferują jeszcze lepszą polszczyznę i mniejsze zużycie RAM.
                </p>
              </div>

              <div className="rounded-xl border border-white/10 bg-neutral-900/60 p-4 space-y-2.5">
                <h4 className="font-semibold text-white">Tryb Jazdy Próbnej (A/B Sandbox & Rollback)</h4>
                <p className="text-neutral-400 text-[11px]">
                  Gdy pojawi się nowy model, nie musisz zmieniać go w ciemno. Kliknięcie{' '}
                  <strong className="text-amber-300">&quot;Wypróbuj model (Jazda próbna)&quot;</strong> uruchamia
                  go w piaskownicy czatu. Pod każdą wygenerowaną odpowiedzią masz dwa przyciski:
                </p>
                <div className="flex gap-2 pt-1">
                  <span className="rounded-md bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 px-2 py-1 text-[11px] font-medium">
                    💾 Zachowaj ten model jako domyślny
                  </span>
                  <span className="rounded-md bg-neutral-800 text-neutral-300 border border-white/10 px-2 py-1 text-[11px] font-medium">
                    ⏪ Przywróć poprzedni model
                  </span>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'guard' && (
            <div className="space-y-4">
              <div className="rounded-xl border border-amber-500/20 bg-amber-950/15 p-4 space-y-2">
                <h3 className="text-sm font-semibold text-amber-300 flex items-center gap-2">
                  <ShieldAlert className="h-4 w-4" />
                  Filozofia Person Guard: „Temat tak, osoba nie”
                </h3>
                <p className="text-neutral-300">
                  BALD to silnik badania tematów, faktów, korporacji, zjawisk i instytucji — a nie
                  narzędzie do inwigilacji lub doxxingu osób prywatnych.
                </p>
              </div>

              <div className="space-y-2.5 text-neutral-300">
                <div className="flex items-start gap-2.5">
                  <span className="text-emerald-400 font-bold">✓ Co badamy:</span>
                  <span>
                    Zjawiska historyczne, afery, technologie, organizacje, korporacje, ustawy, oficjalne
                    raporty oraz <strong>postacie publiczne</strong> (polityków, autorów książek, sygnatariuszy
                    ustaw, prezesów instytucji).
                  </span>
                </div>
                <div className="flex items-start gap-2.5">
                  <span className="text-rose-400 font-bold">✗ Czego nie badamy:</span>
                  <span>
                    Osób prywatnych, danych wrażliwych (PESEL, prywatne numery telefonów, adresy domowe,
                    hasła). Wyszukiwanie zogniskowane na osobie prywatnej jest natychmiast blokowane przez
                    Person Guard.
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-white/10 px-6 py-3 bg-white/[0.02]">
          <div className="text-[11px] text-neutral-400 flex items-center gap-1.5">
            <Lightbulb className="h-3.5 w-3.5 text-amber-400" />
            Wskazówka: Wpisz <code className="text-amber-300 font-mono">/help</code> w czacie w dowolnym momencie.
          </div>
          <button
            onClick={onClose}
            className="rounded-lg bg-amber-500/20 px-4 py-1.5 text-xs font-medium text-amber-300 border border-amber-500/30 hover:bg-amber-500/30 transition-colors"
          >
            Zamknij
          </button>
        </div>
      </div>
    </div>
  );
}
