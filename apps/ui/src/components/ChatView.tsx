import { useCallback, useEffect, useRef, useState } from 'react';
import {
  AlertTriangle,
  ArrowUp,
  Bot,
  Compass,
  Download,
  ExternalLink,
  Globe,
  LoaderCircle,
  MessageSquare,
  Quote,
  RotateCcw,
  Search,
  ShieldCheck,
  Sparkles,
  User,
} from 'lucide-react';
import { ingestEvidence, searchWeb, sendChatMessage } from '../api';
import { useI18n } from '../i18n';
import { cn } from '../lib';
import type { ChatMessageItem, ClaimSummary, IngestResponse, SearchHit } from '../types';
import { Badge, CardShell } from './ui';

interface ChatViewProps {
  onOpenClaim?: (claim: ClaimSummary) => void;
  onSwitchToLedger?: () => void;
}

const STORAGE_KEY = 'bald.chat.history.v1';

function loadStoredMessages(): ChatMessageItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

export function ChatView({ onOpenClaim, onSwitchToLedger }: ChatViewProps) {
  const { t } = useI18n();
  const [mode, setMode] = useState<'chat' | 'search'>('chat');
  const [messages, setMessages] = useState<ChatMessageItem[]>(loadStoredMessages);
  const [input, setInput] = useState('');
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Web search state
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchGuardRefused, setSearchGuardRefused] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [ingestingUrl, setIngestingUrl] = useState<string | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(messages));
    } catch {
      // Ignore localStorage quotas
    }
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSendChat = useCallback(
    async (textToSend?: string) => {
      const messageText = (textToSend ?? input).trim();
      if (!messageText || sending) return;

      const userMessage: ChatMessageItem = {
        id: `u-${Date.now()}`,
        role: 'user',
        content: messageText,
        createdAt: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      };

      setMessages((prev) => [...prev, userMessage]);
      setInput('');
      setSending(true);
      setError(null);

      try {
        const history = messages.slice(-6).map((m) => ({
          role: m.role,
          content: m.content,
        }));

        const response = await sendChatMessage({
          message: messageText,
          history,
        });

        const assistantMessage: ChatMessageItem = {
          id: `a-${Date.now()}`,
          role: 'assistant',
          content: response.content,
          isUrl: response.is_url,
          guardRefused: response.guard_refused,
          ingest: response.ingest,
          matchingClaims: response.matching_claims,
          sources: response.sources,
          autoResearched: response.auto_researched,
          newClaimsCount: response.new_claims_count,
          followUps: response.follow_ups,
          createdAt: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
        };

        setMessages((prev) => [...prev, assistantMessage]);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setSending(false);
        window.setTimeout(() => {
          textareaRef.current?.focus();
        }, 100);
      }
    },
    [input, messages, sending],
  );

  const handleSearchWeb = useCallback(
    async (queryOverride?: string) => {
      const q = (queryOverride ?? searchQuery).trim();
      if (!q || searching) return;

      setSearching(true);
      setSearchError(null);
      setSearchGuardRefused(false);

      try {
        const resp = await searchWeb(q, 10);
        if (resp.guard_refused) {
          setSearchGuardRefused(true);
          setSearchResults([]);
        } else {
          setSearchResults(resp.hits);
        }
      } catch (err) {
        setSearchError(err instanceof Error ? err.message : String(err));
      } finally {
        setSearching(false);
      }
    },
    [searchQuery, searching],
  );

  const handleInvestigateUrl = async (url: string) => {
    setIngestingUrl(url);
    setError(null);
    try {
      const ingestResp = await ingestEvidence(url);
      setMode('chat');
      const assistantMessage: ChatMessageItem = {
        id: `a-${Date.now()}`,
        role: 'assistant',
        content: `Zanalizowałem artykuł **${ingestResp.title}** (\`${ingestResp.domain}\`).\n\n• Wyekstrahowano sprawdzalnych twierdzeń: **${ingestResp.claims_found}**\n• Potwierdzonych dosłownym cytatem (CitationGate): **${ingestResp.claims_accepted}**\n• Odrzuconych (brak pełnego cytatu): **${ingestResp.claims_rejected}**\n\nZweryfikowane twierdzenia zostały zapisane w Twoim lokalnym Rejestrze Twierdzeń.`,
        isUrl: true,
        guardRefused: false,
        ingest: ingestResp,
        createdAt: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      };
      setMessages((prev) => [...prev, assistantMessage]);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIngestingUrl(null);
    }
  };

  const exportDocumentReport = (ingest: IngestResponse) => {
    let md = `# BALD Evidence Ledger Report\n\n`;
    md += `**Tytuł:** ${ingest.title}\n`;
    md += `**URL:** ${ingest.url}\n`;
    md += `**Domena:** ${ingest.domain}\n`;
    md += `**Data analizy:** ${ingest.generated_at}\n`;
    md += `**Status CitationGate:** Zweryfikowano ${ingest.claims_accepted} cytatów, odrzucono ${ingest.claims_rejected}\n\n`;
    md += `---\n\n## Wyekstrahowane twierdzenia i dosłowne cytaty\n\n`;

    for (const [i, acc] of ingest.accepted.entries()) {
      md += `### ${i + 1}. ${acc.text}\n`;
      md += `- **Stance:** \`${acc.stance}\`\n`;
      md += `- **Rodzaj:** \`${acc.kind}\`\n`;
      md += `- **Pewność:** ${Math.round(acc.confidence * 100)}%\n`;
      md += `- **Dosłowny cytat ze źródła:** „${acc.quote}”\n`;
      md += `- **Hash cytatu (SHA-256):** \`${acc.quote_hash}\`\n\n`;
    }

    md += `---\n*Wygenerowano przez BALD Search Engine (VIPER STUDIO) · Temat tak, osoba nie.*`;

    const blob = new Blob([md], { type: 'text/markdown;charset=utf-8' });
    const u = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = u;
    a.download = `bald-raport-${ingest.domain}-${Date.now()}.md`;
    a.click();
    URL.revokeObjectURL(u);
  };

  const handleKeyDownChat = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSendChat();
    }
  };

  const handleKeyDownSearch = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      void handleSearchWeb();
    }
  };

  const handleClear = () => {
    setMessages([]);
    localStorage.removeItem(STORAGE_KEY);
    setError(null);
  };

  const quickPrompts = [
    { label: t('chat.prompt1'), value: 'The Great Reset — co mówią źródła?' },
    { label: t('chat.prompt2'), value: 'https://example.com' },
    { label: t('chat.prompt3'), value: 'Jakie twierdzenia mamy w bazie?' },
    { label: t('chat.prompt4'), value: 'Jak CitationGate weryfikuje cytaty?' },
  ];

  return (
    <div className="mx-auto flex h-[calc(100vh-8.5rem)] max-w-4xl flex-col space-y-4">
      {/* Top action bar with Mode Toggle */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/[0.06] pb-3">
        <div className="flex items-center gap-2.5">
          <span className="grid h-8 w-8 place-items-center rounded-xl border border-amber-300/20 bg-amber-400/10 text-amber-300">
            {mode === 'chat' ? <Bot className="h-4 w-4" /> : <Globe className="h-4 w-4" />}
          </span>
          <div>
            <h1 className="text-sm font-semibold text-slate-100">
              {mode === 'chat' ? t('chat.title') : t('chat.modeSearch')}
            </h1>
            <p className="text-[11px] text-slate-500">
              {mode === 'chat' ? t('chat.subtitle') : t('chat.searchPlaceholder')}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* Mode Switcher */}
          <div className="flex items-center rounded-xl border border-white/[0.08] bg-white/[0.025] p-1">
            <button
              type="button"
              onClick={() => setMode('chat')}
              className={cn(
                'inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-semibold transition',
                mode === 'chat'
                  ? 'bg-amber-400/15 text-amber-200 shadow-sm'
                  : 'text-slate-500 hover:text-slate-300',
              )}
            >
              <MessageSquare className="h-3 w-3" />
              <span>{t('chat.modeChat')}</span>
            </button>
            <button
              type="button"
              onClick={() => setMode('search')}
              className={cn(
                'inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-semibold transition',
                mode === 'search'
                  ? 'bg-cyan-400/15 text-cyan-200 shadow-sm'
                  : 'text-slate-500 hover:text-slate-300',
              )}
            >
              <Search className="h-3 w-3" />
              <span>{t('chat.modeSearch')}</span>
            </button>
          </div>

          {mode === 'chat' && messages.length > 0 && (
            <button
              type="button"
              onClick={handleClear}
              className="inline-flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-1.5 text-xs text-slate-400 transition hover:border-white/20 hover:text-slate-200"
            >
              <RotateCcw className="h-3 w-3" />
              {t('chat.clear')}
            </button>
          )}
        </div>
      </div>

      {/* MODE 1: CHAT & INVESTIGATE */}
      {mode === 'chat' && (
        <>
          <div className="flex-1 space-y-4 overflow-y-auto pr-1">
            {messages.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center p-6 text-center">
                <div className="grid h-16 w-16 place-items-center rounded-2xl border border-amber-300/20 bg-amber-400/[0.08] shadow-[0_0_35px_rgba(251,191,36,0.12)]">
                  <img src="/logo.png" alt="" className="h-10 w-10 object-contain" />
                </div>
                <h2 className="mt-4 text-lg font-bold tracking-tight text-slate-100">
                  {t('chat.emptyTitle')}
                </h2>
                <p className="mt-2 max-w-md text-xs leading-relaxed text-slate-400">
                  {t('chat.emptyLede')}
                </p>

                <div className="mt-6 flex flex-wrap justify-center gap-2">
                  {quickPrompts.map((p, idx) => (
                    <button
                      key={idx}
                      type="button"
                      onClick={() => void handleSendChat(p.value)}
                      className="inline-flex items-center gap-1.5 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3 py-1.5 text-xs text-slate-300 transition hover:border-amber-400/40 hover:bg-amber-400/10 hover:text-amber-200"
                    >
                      <Sparkles className="h-3 w-3 text-amber-300/70" />
                      {p.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              messages.map((msg) => (
                <div
                  key={msg.id}
                  className={cn(
                    'flex gap-3',
                    msg.role === 'user' ? 'justify-end' : 'justify-start',
                  )}
                >
                  {msg.role === 'assistant' && (
                    <div className="grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-amber-300/20 bg-amber-400/10 text-amber-300 shadow-sm">
                      <Bot className="h-4 w-4" />
                    </div>
                  )}

                  <div
                    className={cn(
                      'max-w-[85%] rounded-2xl p-4 text-xs leading-relaxed shadow-sm',
                      msg.role === 'user'
                        ? 'border border-amber-400/20 bg-amber-400/[0.12] text-slate-100'
                        : 'border border-white/[0.08] bg-[#0c1017] text-slate-200',
                    )}
                  >
                    {/* Person Guard banner */}
                    {msg.guardRefused && (
                      <div className="mb-3 inline-flex items-center gap-1.5 rounded-lg border border-emerald-400/20 bg-emerald-400/10 px-2.5 py-1 text-[11px] font-medium text-emerald-300">
                        <ShieldCheck className="h-3.5 w-3.5" />
                        {t('chat.guardNotice')}
                      </div>
                    )}

                    {/* Perplexity-style Live Research Sources Shelf */}
                    {msg.sources && msg.sources.length > 0 && (
                      <div className="mb-3.5 space-y-2 border-b border-white/[0.06] pb-3">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <div className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-cyan-300">
                            <Globe className="h-3.5 w-3.5 text-cyan-400" />
                            <span>Zbadano na żywo w sieci ({msg.sources.length} źródeł)</span>
                            <span className="text-[10px] font-normal text-slate-500">· od najnowszych</span>
                          </div>
                          {msg.newClaimsCount && msg.newClaimsCount > 0 ? (
                            <span className="inline-flex items-center gap-1 rounded-full border border-amber-400/20 bg-amber-400/10 px-2 py-0.5 text-[10px] font-medium text-amber-300/90">
                              <Sparkles className="h-3 w-3" />
                              +{msg.newClaimsCount} twierdzeń w Ledgerze
                            </span>
                          ) : null}
                        </div>

                        <div className="grid grid-cols-1 gap-2 pt-1 sm:grid-cols-2">
                          {msg.sources.map((src, sIdx) => (
                            <a
                              key={sIdx}
                              href={src.url}
                              target="_blank"
                              rel="noreferrer"
                              className="group flex flex-col justify-between rounded-xl border border-white/[0.08] bg-white/[0.025] p-2.5 transition hover:border-cyan-400/30 hover:bg-white/[0.05]"
                            >
                              <div className="flex items-center gap-1.5">
                                <span className="grid h-4 w-4 shrink-0 place-items-center rounded-full bg-cyan-400/15 text-[10px] font-bold text-cyan-300">
                                  {sIdx + 1}
                                </span>
                                <span className="truncate text-[10px] font-medium text-slate-400 group-hover:text-cyan-300">
                                  {src.domain}
                                </span>
                                {src.published && (
                                  <span className="rounded bg-white/[0.06] px-1.5 py-0.5 text-[9px] font-mono text-cyan-200/80">
                                    {src.published}
                                  </span>
                                )}
                                <ExternalLink className="ml-auto h-2.5 w-2.5 shrink-0 text-slate-500 group-hover:text-cyan-300" />
                              </div>
                              <p className="mt-1 line-clamp-2 text-[11px] font-semibold text-slate-200 group-hover:text-white">
                                {src.title}
                              </p>
                            </a>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Main message text */}
                    <div className="whitespace-pre-wrap font-sans">{msg.content}</div>

                    {/* If document URL was analyzed */}
                    {msg.ingest && (
                      <div className="mt-4 rounded-xl border border-white/[0.08] bg-black/30 p-3.5">
                        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/[0.06] pb-2">
                          <div className="flex items-center gap-1.5 font-semibold text-slate-200">
                            <Globe className="h-3.5 w-3.5 text-cyan-400" />
                            <span className="truncate">{msg.ingest.title}</span>
                          </div>
                          <div className="flex items-center gap-2">
                            <a
                              href={msg.ingest.url}
                              target="_blank"
                              rel="noreferrer"
                              className="inline-flex items-center gap-1 text-[10px] text-cyan-400 hover:text-cyan-300"
                            >
                              {msg.ingest.domain}
                              <ExternalLink className="h-2.5 w-2.5" />
                            </a>
                            <button
                              type="button"
                              onClick={() => exportDocumentReport(msg.ingest!)}
                              className="inline-flex items-center gap-1 rounded border border-white/10 bg-white/[0.04] px-2 py-0.5 text-[10px] text-slate-300 hover:border-amber-400/40 hover:text-amber-200"
                              title={t('chat.exportReport')}
                            >
                              <Download className="h-2.5 w-2.5 text-amber-300" />
                              <span>Eksportuj .md</span>
                            </button>
                          </div>
                        </div>

                        <div className="mt-2.5 flex items-center gap-2 text-[10px]">
                          <Badge tone="emerald">
                            {t('chat.quotesAccepted', { count: msg.ingest.claims_accepted })}
                          </Badge>
                          {msg.ingest.claims_rejected > 0 && (
                            <Badge tone="rose">
                              {t('chat.quotesRejected', { count: msg.ingest.claims_rejected })}
                            </Badge>
                          )}
                        </div>

                        {/* Claims list */}
                        {msg.ingest.accepted.length > 0 && (
                          <div className="mt-3 space-y-2">
                            <p className="text-[11px] font-semibold text-slate-400">
                              {t('chat.claimsFound')}
                            </p>
                            {msg.ingest.accepted.map((acc, i) => (
                              <div
                                key={i}
                                className="rounded-lg border border-white/[0.06] bg-white/[0.02] p-2.5 text-[11px]"
                              >
                                <div className="flex items-center justify-between gap-2">
                                  <span className="font-semibold text-slate-200">{acc.text}</span>
                                  <Badge tone="amber">{acc.stance}</Badge>
                                </div>
                                <div className="mt-1.5 flex items-start gap-1.5 text-slate-400">
                                  <Quote className="mt-0.5 h-3 w-3 shrink-0 text-amber-300/70" />
                                  <span className="italic text-slate-300">„{acc.quote}”</span>
                                </div>
                              </div>
                            ))}
                          </div>
                        )}

                        {onSwitchToLedger && (
                          <button
                            type="button"
                            onClick={onSwitchToLedger}
                            className="mt-3 inline-flex items-center gap-1.5 text-[11px] font-semibold text-amber-300 hover:text-amber-200"
                          >
                            {t('chat.openInLedger')}
                          </button>
                        )}
                      </div>
                    )}

                    {/* If matching claims were found */}
                    {msg.matchingClaims && msg.matchingClaims.length > 0 && (
                      <div className="mt-3 space-y-2 border-t border-white/[0.06] pt-2.5">
                        <p className="text-[11px] font-semibold text-slate-400">
                          Powiązane twierdzenia w Rejestrze:
                        </p>
                        <div className="grid gap-2 sm:grid-cols-2">
                          {msg.matchingClaims.map((c) => (
                            <div
                              key={c.id}
                              onClick={() => onOpenClaim?.(c)}
                              className="cursor-pointer rounded-lg border border-white/[0.06] bg-white/[0.02] p-2.5 transition hover:border-amber-400/30 hover:bg-white/[0.04]"
                            >
                              <div className="flex items-center justify-between gap-1">
                                <span className="truncate font-semibold text-slate-200">
                                  {c.text}
                                </span>
                                <Badge tone="neutral">{c.kind}</Badge>
                              </div>
                              <p className="mt-1 text-[10px] text-slate-500">
                                {c.document_count} źródeł · {c.evidence_count} cytatów
                              </p>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Follow-up suggestions (Drąż temat) */}
                    {msg.followUps && msg.followUps.length > 0 && (
                      <div className="mt-3.5 space-y-1.5 border-t border-white/[0.06] pt-3">
                        <div className="flex items-center gap-1.5 text-[11px] font-semibold text-amber-300/90">
                          <Compass className="h-3 w-3 text-amber-300" />
                          <span>Proponowane kierunki śledztwa (Drąż temat):</span>
                        </div>
                        <div className="flex flex-wrap gap-1.5 pt-0.5">
                          {msg.followUps.map((q, qIdx) => (
                            <button
                              key={qIdx}
                              type="button"
                              onClick={() => void handleSendChat(q)}
                              className="inline-flex items-center gap-1 rounded-xl border border-white/10 bg-white/[0.03] px-2.5 py-1 text-left text-[11px] text-slate-300 transition hover:border-amber-400/35 hover:bg-amber-400/10 hover:text-amber-200"
                            >
                              <Sparkles className="h-2.5 w-2.5 shrink-0 text-amber-300/70" />
                              <span>{q}</span>
                            </button>
                          ))}
                        </div>
                      </div>
                    )}

                    <div className="mt-2 text-right text-[9px] text-slate-500">{msg.createdAt}</div>
                  </div>

                  {msg.role === 'user' && (
                    <div className="grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-white/10 bg-white/[0.04] text-slate-400 shadow-sm">
                      <User className="h-4 w-4" />
                    </div>
                  )}
                </div>
              ))
            )}

            {/* Loading bubble */}
            {sending && (
              <div className="flex items-center gap-3">
                <div className="grid h-8 w-8 place-items-center rounded-xl border border-amber-300/20 bg-amber-400/10 text-amber-300">
                  <Bot className="h-4 w-4" />
                </div>
                <div className="inline-flex items-center gap-2 rounded-2xl border border-white/[0.08] bg-[#0c1017] px-4 py-3 text-xs text-slate-400">
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin text-amber-400" />
                  <span>{t('chat.analyzing')}</span>
                </div>
              </div>
            )}

            {error && (
              <div className="flex items-center gap-2 rounded-xl border border-rose-400/20 bg-rose-400/[0.06] p-3 text-xs text-rose-300">
                <AlertTriangle className="h-4 w-4 shrink-0 text-rose-400" />
                <span>{error}</span>
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>

          {/* Input area */}
          <CardShell className="p-2 sm:p-2.5">
            <div className="flex items-end gap-2">
              <textarea
                ref={textareaRef}
                rows={2}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDownChat}
                disabled={sending}
                placeholder={t('chat.placeholder')}
                className="w-full resize-none rounded-xl border-none bg-transparent px-3 py-2 text-xs text-slate-100 placeholder:text-slate-500 focus:outline-none focus:ring-0 disabled:opacity-50"
              />
              <button
                type="button"
                onClick={() => void handleSendChat()}
                disabled={sending || !input.trim()}
                className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-amber-400 text-amber-950 shadow-[0_4px_16px_rgba(251,191,36,0.2)] transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-40"
                title={t('chat.send')}
              >
                {sending ? (
                  <LoaderCircle className="h-4 w-4 animate-spin" />
                ) : (
                  <ArrowUp className="h-4 w-4 stroke-[2.5]" />
                )}
              </button>
            </div>
          </CardShell>
        </>
      )}

      {/* MODE 2: WEB SEARCH & 1-CLICK INVESTIGATE */}
      {mode === 'search' && (
        <div className="flex flex-1 flex-col space-y-4 overflow-hidden">
          {/* Search bar */}
          <CardShell className="p-2 sm:p-2.5">
            <div className="flex items-center gap-2">
              <Search className="ml-2 h-4 w-4 text-slate-500" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={handleKeyDownSearch}
                disabled={searching}
                placeholder={t('chat.searchPlaceholder')}
                className="w-full border-none bg-transparent px-2 py-1.5 text-xs text-slate-100 placeholder:text-slate-500 focus:outline-none focus:ring-0 disabled:opacity-50"
              />
              <button
                type="button"
                onClick={() => void handleSearchWeb()}
                disabled={searching || !searchQuery.trim()}
                className="inline-flex shrink-0 items-center gap-1.5 rounded-xl bg-cyan-500 px-4 py-2 text-xs font-semibold text-cyan-950 shadow-md transition hover:bg-cyan-400 disabled:opacity-40"
              >
                {searching ? (
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Search className="h-3.5 w-3.5" />
                )}
                <span>{searching ? t('chat.searching') : t('chat.modeSearch')}</span>
              </button>
            </div>
          </CardShell>

          {/* Search Results list */}
          <div className="flex-1 space-y-3 overflow-y-auto pr-1">
            {searchGuardRefused && (
              <div className="flex items-center gap-2 rounded-xl border border-emerald-400/20 bg-emerald-400/10 p-4 text-xs text-emerald-300">
                <ShieldCheck className="h-5 w-5 shrink-0" />
                <span>{t('chat.guardNotice')}</span>
              </div>
            )}

            {searchError && (
              <div className="flex items-center gap-2 rounded-xl border border-rose-400/20 bg-rose-400/[0.06] p-3 text-xs text-rose-300">
                <AlertTriangle className="h-4 w-4 shrink-0 text-rose-400" />
                <span>{searchError}</span>
              </div>
            )}

            {searchResults.length > 0 ? (
              <div className="space-y-3">
                <p className="text-[11px] font-semibold text-slate-400">
                  {t('chat.searchResults', { count: searchResults.length })}
                </p>
                {searchResults.map((hit, idx) => {
                  const isIngesting = ingestingUrl === hit.url;
                  return (
                    <div
                      key={idx}
                      className="rounded-2xl border border-white/[0.08] bg-[#0c1017] p-4 shadow-sm transition hover:border-white/15"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="font-mono text-[10px] text-cyan-400">
                              {hit.domain}
                            </span>
                            <span className="text-[9px] text-slate-600">
                              {hit.providers.join(', ')}
                            </span>
                          </div>
                          <h3 className="mt-1 font-semibold text-slate-100">{hit.title}</h3>
                          <p className="mt-1 text-xs leading-relaxed text-slate-400">
                            {hit.snippet}
                          </p>
                        </div>

                        {/* 1-Click Investigation Button */}
                        <button
                          type="button"
                          onClick={() => void handleInvestigateUrl(hit.url)}
                          disabled={Boolean(ingestingUrl)}
                          className="inline-flex shrink-0 items-center gap-1.5 rounded-xl border border-amber-400/30 bg-amber-400/10 px-3 py-2 text-xs font-semibold text-amber-200 transition hover:border-amber-400/60 hover:bg-amber-400/20 disabled:opacity-50"
                        >
                          {isIngesting ? (
                            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            <Compass className="h-3.5 w-3.5 text-amber-300" />
                          )}
                          <span>
                            {isIngesting ? t('chat.investigating') : t('chat.investigateNow')}
                          </span>
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              !searching && (
                <div className="flex h-64 flex-col items-center justify-center rounded-2xl border border-dashed border-white/10 p-6 text-center text-xs text-slate-500">
                  <Globe className="h-8 w-8 text-slate-600" />
                  <p className="mt-2 font-medium">Wpisz temat, aby wyszukać artykuły w internecie</p>
                  <p className="mt-1 text-[11px] text-slate-600">
                    BALD przeszuka sieć i pozwoli Ci zbadać każdy artykuł jednym kliknięciem.
                  </p>
                </div>
              )
            )}
          </div>
        </div>
      )}
    </div>
  );
}
