# BALD SEARCH ENGINE — plan architektury

**Wersja:** 0.8 (plan + fazy 0–4)
**Data:** 2026-10-26
**Stack:** Rust, desktop-first (Tauri), web-first (`bald serve`), lokalny-first, chmura opcjonalna
**Status:** fazy 0–3 zrealizowane; silnik, crawler, ledger, API Axum i frontend React działają w pełni;
faza 4 (wbudowany Mini-LLM z Tool-Callingiem Out-of-the-Box + Model Decyzyjno-Scoringowy) w planie architektonicznym;
szczegóły decyzji w §16, stan implementacji w §19a

**Decyzje zamykające v0.1:** Tauri · Leptos · bez własnego crawlera w v1 · PL+EN ·
presety chmurowe + lokalne + „dodaj providera”.

**Nowe w 0.8:** Faza 4 — architektura dwusilnikowa:
1) **Wbudowany Mini-LLM (Out-of-the-Box)**: Qwen 2.5 1.5B Instruct (GGUF Q4, ~950 MB) osadzony bezpośrednio w Rust (`candle`/`llama-cpp-rs`) z obsługą natywnego *Function / Tool Calling* (narzędzia `search_web`, `ingest_and_verify`, `query_ledger`) oraz *Grammar-Constrained Decoding* (100% gwarancja poprawnego JSON). Działa natychmiast bez instalacji zewnętrznych serwisów (Ollama/Python).
2) **Model Decyzyjno-Scoringowy (Evidence & Credibility Judge)**: lekki silnik tabelaryczny (ONNX / Kumo-Tabular / PFN, 50–120 MB) pełniący rolę obiektywnego sędziego. Wylicza twardy wskaźnik wiarygodności (0–100%), chroni Rejestr Twierdzeń przed farmami botów, wykrywa skoordynowane kampanie narracyjne i wykonuje faktograficzny re-ranking.
3) **UX "Zero-Config"**: podział ustawień na profil domyślny („po prostu działa”) oraz rozwijany panel „Zaawansowane” (zewnętrzne API, Ollama, własny SearXNG/Tor).

**Nowe w 0.3:** D13 Tor/onion poza v1 · D14 seed-core jako action gate, **nie** klasyfikator
zapytań · D15 edge-radar jako ingress w trybie daemon, **nie** egress · D16 komponenty JFP
za feature flag (izolacja licencyjna).

**Nowe w 0.4:** realizacja ADR-005 i fazy 0 — workspace Rust, `bse-guard` (40/40 testów
person-targeted i benign), `bse-store` SQLite, providerzy SearXNG/Brave/Demo, CLI `bald`,
audit hash chain oraz odłączony shell Tauri. Świadome odstępstwo: `rusqlite` zamiast `sqlx`.

**Nowe w 0.5:** realizacja fazy 1 — `bse-fetch` (polityka SSRF, `robots.txt`, cache,
ekstrakcja HTML/plain), `bse-index` (Tantivy/BM25, upsert), near-duplicate MinHash/LSH,
metryka różnorodności domen, CLI `fetch`/`index`, ADR-006. MSRV podniesiony do 1.88
(Tantivy 0.27).

**Nowe w 0.6:** realizacja fazy 2 — `bse-llm` (Ollama/OpenAI-compatible, endpoint policy),
`bse-classify` (twierdzenia, stance, encje nie-osobowe, PII, fallback `heuristic-v1`),
evidence ledger w SQLite, `CitationGate` + minimalna długość cytatu, CLI
`evidence add/list/show/stats`, ADR-007. Osoby nie są encjami; `noindex` jest pomijany
przed LLM.

**Nowe w 0.7:** realizacja fazy 3 — `bse-api` (serwis Axum na loopback `127.0.0.1:7333`, session token
wstrzykiwany do HTML, CSP), `bald serve` (działa bez WebKitGTK bezpośrednio w przeglądarce),
hybrydowy frontend React + TypeScript + Tailwind + Lucide w `apps/ui` (siatka claims, drawer dowodów,
ustawienia), Tauri IPC bridge, atomowy zapis konfiguracji z uprawnieniami `0600`, ADR-008.

---

## 0. Streszczenie

BALD to **wyszukiwarka twierdzeń, narracji i materiałów źródłowych** (ang. *Claim, Narrative & Source Evidence Engine*) — nie narzędzie do namierzania ludzi.
Silnik zbiera **twierdzenia (claims)**, a do każdego z nich **dowody i ich status**: źródła pierwotne,
archiwa, fact-checki, kontrdowody, daty, ślad usunięć i deplatformizacji.

Rdzeń produktu to **nie wyszukiwarka linków**, a **rejestr dowodowy (evidence ledger)**:
użytkownik dostaje odpowiedź z cytowaniami, oceną wiarygodności źródeł i jawnym rozdzieleniem
„co twierdzi źródło” od „co jest udokumentowane”.

Cztery filary v0.2:

1. **Metasearch + archiwum** — wiele providerów wyszukiwania i archiwów, każdy za tym samym
   interfejsem (chmurowy, lokalny, własny).
2. **Claim-first** — LLM nie streszcza „na wiarę”, tylko wyciąga twierdzenia i przypisuje im źródła.
   Każde zdanie w odpowiedzi ma cytację albo trafia do sekcji „brak pokrycia”.
3. **Graf narracji** — twierdzenia, byty i narracje jako sieć powiązań; oddzielenie „dokumentu”
   od „interpretacji” jest widoczne w interfejsie, nie ukryte w rankingu.
4. **Prostota obsługi** — jeden ekran providerów: dodaj klucz / wykryj lokalny LLM / testuj / włącz.
   Zero Dockera, zero edycji plików konfiguracyjnych w codziennej pracy.

---

## 1. Definicja produktu

### 1.1 Czym to jest

- Wyszukiwarka **tematów, twierdzeń i narracji** („kto/co/kiedy twierdzi”), nie osób.
- Indeks i archiwum **treści usuniętych, ocenzurowanych lub zdeplatformowanych** — z oznaczeniem
  statusu dostępności i historią zmian.
- Narzędzie **analityczne**: timeline twierdzenia, źródła pierwotne, korekty, fact-checki, graf aktorów.
- Działa **w 100% lokalnie** (SearXNG + Ollama + SQLite/Tantivy) albo hybrydowo z chmurą.
- **Dyferencjacja źródeł jest mierzona** — wynik pokazuje, czy pochodzi z jednej bańki, czy z wielu
  niezależnych domen (metryka `source_diversity`).

### 1.2 Czym to NIE jest

- Nie jest to „wyszukiwarka informacji o osobach”, profilowanie, doxxing, dossier.
- Nie orzeka „prawda/fałsz” — pokazuje **status dokumentacyjny** i **rozdźwięk** między twierdzeniem
  a dowodami.
- Nie obchodzi płatnych ścian, nie łamie ToS, nie scrapuje wbrew robots.txt.
- Nie jest narzędziem do operacjonalizacji przemocy — zapytania o „jak” są odcinane na poziomie planera.
- Nie jest „agregatorem niesprawdzonych treści” — indeksuje **dokumenty o twierdzeniach i tematach**, w tym ich obalenia i weryfikację,
  i zawsze oznacza stance.

### 1.3 Zasady projektowe (twarde)

| Zasada | Konsekwencja techniczna |
|---|---|
| Dokumentacja > opinia | Każdy wynik ma typ źródła i tier; brak źródła = jawny warning |
| Człowiek nie jest celem | Klasyfikator zapytań blokuje zapytania person-targeted (imię + adres/pracodawca/rodzina) |
| Cytowanie obowiązkowe | Synteza LLM odrzucana, jeśli zdanie nie mapuje się na źródło (walidacja post-hoc) |
| Wzmianka ≠ poparcie | Każdy dokument ma `stance`: relacjonuje / popiera / obala / wspomina |
| Jedno źródło = ostrzeżenie | Badge „single-source” dopóki brak niezależnego potwierdzenia |
| Archiwum zawsze | Dla każdego dokumentu próbujemy snapshot (Wayback/CDX), zapisujemy hash treści |
| Lokalność domyślnie | Domyślny profil: tylko lokalne providery; chmura wymaga świadomego włączenia |
| Desktop first | Tauri 2 + Leptos; brak Dockera i JVM w ścieżce użytkownika |
| Przejrzystość rankingu | API zwraca rozkład punktów (RRF, trust, świeżość, korroboracja) |
| Budżet pod kontrolą | Twardy limit $/zapytanie i $/dzień, egzekwowany przed wywołaniem providera |

### 1.4 Definicja „udokumentowane”

Dokument jest **udokumentowany**, gdy spełnia minimum: ma **autora/instytucję**, **datę**,
**trwały link lub snapshot** i **można go sklasyfikować** do jednego z tierów.

**Oś 1 — typ źródła (T1–T6 + ARC):**

| Tier | Opis | Przykład |
|---|---|---|
| T1 | Źródło pierwotne / akt oficjalny | wyrok, raport sądowy, dokument rządowy, publikacja |
| T2 | Recenzowane / instytucjonalne | OpenAlex, PubMed, raport NGO/think-tanku |
| T3 | Fact-check / ClaimReview | Google Fact Check, Demagog, AFP Fact Check |
| T4 | Dzienniki o sprawdzonej procedurze | AP, Reuters, PAP, BBC |
| T5 | Media niszowe / branżowe / blogi | Marginalia, Substack, fora |
| T6 | Treść anonimowa / niezweryfikowana | 4chan, niepodpisane mirror'y |
| ARC | Archiwum (niezależne od tieru) | Wayback, archive.today, Common Crawl WARC |

**Oś 2 — autorytet (A–D)**, hierarchia z praktyki fact-checkingu:

| Klasa | Opis |
|---|---|
| A | Komunikat oficjalny instytucji |
| B | Interakcja oficjalna (odpowiedź urzędu, oświadczenie, protokół) |
| C | Weryfikacja krzyżowa społeczności / organizacji |
| D | Medium nieznane, brak procedury |

**Oś 3 — niezależność (I0–I3):** czy źródło jest niezależne od pierwotnego twierdzenia
(I0 = to samo źródło / ten sam właściciel, I3 = niezależne medium lub instytucja).

> **A ≠ prawda.** Komunikat państwowy jest źródłem pierwotnym *narracji państwowej*, nie neutralnym
> arbitrem. Dlatego autorytet, tier i niezależność liczymy osobno i pokazujemy osobno — bez jednej
> „oceny prawdy”.

---

## 2. Model domenowy

### 2.1 Encje

```
Claim (twierdzenie)
 ├─ id, tekst znormalizowany, język, wektory (embedding)
 ├─ typ: fakt | przyczynowość | ukrycie | spisek | przepowiednia
 ├─ aktorzy[] → Entity (kto ma działać w tle)
 ├─ pierwsze_wystąpienie, ostatnie_wystąpienie
 ├─ status: niesprawdzone | częściowo_poparte | obalone | nierozstrzygalne
 └─ Evidence[] ──> Document

Document (dokument)
 ├─ url kanoniczny, hash treści, hash HTML
 ├─ tytuł, autor, data publikacji, język
 ├─ source_id → Source (domena/instytucja)
 ├─ stance: relacjonuje | popiera | obala | wspomina
 ├─ availability: live | archived | removed | paywalled | geoblocked | unknown
 ├─ snapshots[] (Wayback, archive.today, własny WARC)
 └─ extracted_text, sekcje, embedding

Source (źródło)
 ├─ domena / instytucja / kanał
 ├─ tier (T1..T6), authority (A..D), independence (I0..I3)
 ├─ rubryka oceny, historia błędów
 └─ polityka: dozwolone / ograniczone / zablokowane

Evidence (powiązanie)
 ├─ claim_id, document_id
 ├─ relacja: popiera | przeczy | kontekst | powtórzenie
 ├─ siła (0..1) + uzasadnienie (cytat ze źródła)
 └─ data przypisania, model, wersja promptu

Entity (byt w narracji)
 ├─ id, kind: instytucja | wydarzenie | dokument | aktor publiczny | grupa
 ├─ nazwa, aliasy, wikidata_id
 ├─ is_public_figure, is_private_individual
 │    └─ private = true → blokada agregacji, brak strony bytu, tylko wzmianka w cytacie
 └─ wystąpienia[] → claim_id (bez pól osobowych: adres, rodzina, kontakt)

Narrative (narracja)
 ├─ id, etykieta, embedding, okno czasowe
 ├─ claims[] (klaster podobnych twierdzeń)
 └─ entities[] (wspólny zestaw aktorów)

SearchRun (zapytanie)
 ├─ query, plan, providers[] (kto zapytany, koszt, liczba wyników)
 ├─ tokeny, koszt_usd, latencja
 └─ wynik: claims[] + documents[] + trace
```

### 2.2 Cykl życia zapytania

1. Użytkownik pyta: „co wiadomo o teorii X”.
2. Planer rozbija na **podzapytania** (frazy źródłowe, synonimy, nazwy własne, język oryginału).
3. Guard sprawdza, czy zapytanie nie jest person-targeted / harmful.
4. Fan-out do providerów (równolegle, z budżetem i timeoutami).
5. Deduplikacja i kanonizacja URL.
6. Pobranie treści + ekstrakcja + snapshot w archiwum.
7. Ekstrakcja twierdzeń + klasyfikacja + stance (LLM, structured output).
8. Grading dowodów + ekstrakcja bytów.
9. Ranking i fuzja (RRF + trust + świeżość + korroboracja + różnorodność źródeł).
10. Zapis do grafu (encje, krawędzie, klastry narracji).
11. Synteza z cytowaniami + walidacja „każde zdanie ma źródło”.
12. Zapis runu (reuse przy kolejnych zapytaniach) + zwrot evidence ledger.

### 2.3 Graf narracji

**Po co:** teoria spiskowa to nie pojedynczy tekst, tylko **sieć powiązań** między „dowodami”.
Dokument bez kontekstu kłamie przez wycięcie; graf pokazuje, że ten sam „dowód” krąży w 40 wariantach
i kto go powtarza.

**Węzły:** `Claim`, `Entity`, `Document`, `Narrative`.
**Krawędzie:** `mentions`, `supports`, `contradicts`, `repeats`, `precedes`, `co_occurs`.

Typowe zapytania:

- sąsiedztwo twierdzenia do głębokości 2–3 (co je popiera, co obala),
- timeline narracji (pierwsze wystąpienie → mutacje → szczyt → spadek),
- współwystępowanie bytów („kto pojawia się razem z kim” — na poziomie narracji, nie osób prywatnych),
- mosty między narracjami (jeden dokument łączy dwa wątki).

**Przechowywanie (decyzja):**

| Opcja | Werdykt | Dlaczego |
|---|---|---|
| Neo4j | ✗ w v1 | JVM + osobny serwis — zabija „jeden instalator” Tauri |
| Postgres + Apache AGE | ○ tryb serwerowy | openCypher, dobre przy >1M krawędzi |
| SQLite (tabele krawędzi) + `petgraph` | ✓ domyślnie | zero dodatkowych usług, rekurencyjne CTE do depth 3, analityka w pamięci |

Skala: do ~200 tys. krawędzi SQLite + indeksy wystarcza z zapasem. `petgraph` służy do centralności
i wykrywania społeczności (label propagation / Louvain) — liczonym offline, nie w ścieżce zapytania.

**Etyka grafu:** węzeł `Entity` dla osoby publicznej jest dozwolony **tylko jako aktor narracji**
(„twierdzenie przypisuje X rolę w spisku”) i nigdy nie agreguje danych osobowych. Osoby prywatne
mają `is_private_individual = true` → brak strony bytu, brak agregacji, tylko cytat w dokumencie.

---

## 3. Architektura

### 3.1 Przepływ

```
            ┌──────────────────────┐
            │  Tauri UI (Leptos)   │
            └──────────┬───────────┘
                       ▼  typed IPC
        ┌────────────────────────────┐
        │  Query Planner             │  (LLM opcjonalnie, reguły zawsze)
        │  + Person Guard            │
        └──────────────┬─────────────┘
                       ▼
   ┌───────────────────────────────────────┐
   │  Fan-out (tower::Service)             │
   │  search · archive · academic          │
   │  factcheck · decentralized · render   │
   └──────────────────┬────────────────────┘
                      ▼
   ┌───────────────────────────────────────┐
   │ Canonicalize → Dedupe → Fetch/Extract │
   │ snapshot · hash · availability        │
   └──────────────────┬────────────────────┘
                      ▼
   ┌───────────────────────────────────────┐
   │ Claim Extraction (LLM)                │
   │ Classify · Stance · Entities          │
   └──────────────────┬────────────────────┘
                      ▼
   ┌───────────────────────────────────────┐
   │ Rank: RRF + trust + recency           │
   │       + corroboration + diversity     │
   └──────────────────┬────────────────────┘
                      ▼
   ┌───────────────────────────────────────┐
   │ Synthesis with citations              │
   │ + citation validator                  │
   └──────────────────┬────────────────────┘
                      ▼
      Response + Evidence Ledger + Graph
```

### 3.2 Workspace (crates)

```
bald/
├── Cargo.toml                 # [workspace], profil release (lto, codegen-units=1)
├── crates/
│   ├── bse-core/              # typy domenowe, błędy, trait Provider, Capabilities
│   ├── bse-config/            # figment, sekrety, katalog presetów, walidacja
│   ├── bse-store/             # trait RunStore, SQLite (domyślnie), Postgres (feature)
│   ├── bse-providers/         # searxng, brave, tavily, exa, wayback, yacy,
│   │                          #   openalex, factcheck, render, custom...
│   ├── bse-llm/               # OpenAI-compatible, Anthropic, Gemini, Ollama, budżety
│   ├── bse-fetch/             # HTTP (reqwest+rustls), SSRF policy, cache, robots.txt
│   ├── bse-index/             # Tantivy (BM25), HNSW, dedupe (MinHash/LSH), ranking
│   ├── bse-classify/          # reguły + LLM + opcjonalny ONNX (conspiracy/stance)
│   ├── bse-guard/             # RulesGate (person/PII) · CitationGate · adapter seed-core
│   ├── bse-graph/             # encje, krawędzie, klastry narracji, petgraph
│   ├── bse-transport/         # Direct | TorSocks | Arti (feature), polityka .onion
│   ├── bse-gateway/           # [feature "server"] adapter jfp-edge-radar (ingress)
│   ├── bse-pipeline/          # orkiestracja zapytania jako tower::Service
│   ├── bse-api/               # axum (tryb daemon), OpenAPI, SSE, auth
│   ├── bse-cli/               # clap: search, providers, keys, index, eval
│   ├── bse-desktop/           # Tauri 2 shell, komendy IPC, updater
│   └── bse-ui/                # Leptos 0.7+ (CSR/WASM), wspólne typy z bse-core
├── migrations/sqlite/         # sqlx
├── migrations/postgres/       # opcjonalnie (feature "server")
├── eval/                      # golden set + harness
└── docs/adr/                  # decyzje architektoniczne
```

Zależności orientacyjne (pinowane w `Cargo.lock`): `tokio 1`, `axum 0.8`, `tauri 2`,
`leptos 0.7`, `tower 0.5`, `reqwest 0.12` (rustls), `serde 1` + `schemars`, `sqlx 0.8` (sqlite),
`tantivy 0.2x`, `hnsw_rs` (lub `usearch`), `moka`, `governor`, `backon`, `secrecy` + `zeroize`
+ `keyring`, `figment`, `clap 4`, `tracing`, `thiserror`, `async-trait`, `url`,
`scraper`/`dom_smoothie`, `feed-rs`, `jsonpath-rust`, `petgraph`, `fastembed` / `ort`
(opcjonalnie), `utoipa`, `insta`, `wiremock`, `proptest`.

**Opcjonalne, za feature flagami (licencje JFP — patrz §3.6):** `seed-core = "0.1.6"`
(`feature = "seed-gate"`), `jfp-edge-radar` (path/git, `feature = "server-gateway"`),
`arti-client` (`feature = "tor"`). Domyślny build BALD nie zawiera żadnego z nich.

### 3.3 Zasady wewnętrzne

- **Jeden kontrakt providera**, brak „specjalnych ścieżek” w pipeline.
- **Wszystko przez `tower`**: retry, timeout, rate limit, circuit breaker jako warstwy.
- **Fail-open na źródłach, fail-closed na budżecie**: brak jednego providera nie psuje wyniku,
  przekroczenie budżetu zatrzymuje wywołania.
- **Cache trójwarstwowy**: HTTP (ETag/Last-Modified), zapytania (klucz = znormalizowane query + profil),
  semantyczny (embedding + próg podobieństwa).
- **Wersjonowanie promptów i modeli** w bazie — każdy evidence ma `model_version` i `prompt_version`.
- **Sieć wyłącznie w Rust** — WebView nigdy nie dostaje kluczy ani nie wykonuje requestów.

### 3.4 Aplikacja desktopowa (Tauri)

**Model procesu (v1): jeden proces.**

```
┌─────────────────────────────────────────────┐
│ Tauri 2                                     │
│  ├─ WebView: Leptos CSR (WASM)              │
│  └─ Rust core: tokio runtime                │
│       ├─ pipeline / providers / store       │
│       ├─ SQLite + Tantivy + HNSW (lokalnie) │
│       └─ IPC commands (typowane)            │
└─────────────────────────────────────────────┘
```

- **Komendy IPC:** `search`, `cancel_search`, `get_claim`, `get_document`, `list_providers`,
  `test_provider`, `set_key`, `delete_key`, `discover_models`, `usage`, `entity_graph`.
- **Streaming:** eventy `search:progress` (provider po providerze), `index:status`, `search:done`.
- **Klucze** żyją tylko w procesie Rust (keyring), WebView dostaje `••••last4`.
- **Bezpieczeństwo Tauri:** CSP bez `unsafe-inline`, brak zdalnych źródeł w WebView, minimalne
  uprawnienia w `capabilities`, `dangerousRemoteDomainIpcAccess` wyłączone.
- **Tryb headless (opcjonalny):** `bse-daemon` — ten sam kod, lokalne HTTP `127.0.0.1:7788`
  dla CLI/automatyzacji. Tauri i daemon dzielą `bse-pipeline`, więc nie ma dwóch implementacji.
- **Dystrybucja:** `.deb`/AppImage (Linux), `.dmg` (macOS), `.msi` (Windows), podpis + updater.
- **Brak Dockera w ścieżce użytkownika.** Docker pojawia się tylko w dokumentacji „jak postawić
  SearXNG/YaCy”, nie w instalatorze.

**Dlaczego Leptos, a nie askama + HTMX:** Tauri nie ma serwera renderującego HTML — askama+HTMX
wymagałaby wbudowania serwera HTTP i drugiego okna przeglądarki (gorszy UX, dwa procesy, IPC przez
sieć). Leptos daje typowane komponenty współdzielące `bse-core` z backendem, WASM w WebView i jedno
źródło prawdy dla typów. Koszt: dłuższa kompilacja i ~1–2 MB WASM — akceptowalne dla desktopu.

### 3.5 Transport i sieci alternatywne (Tor / onion)

**Pytanie „jaka darmowa przeglądarka?” ma dwie różne odpowiedzi** — zależnie od tego, czy chodzi
o ręczną weryfikację, czy o warstwę zbierania danych w BALD.

**Do ręcznej weryfikacji linków (darmowe):**

| Narzędzie | Co daje | Uwaga |
|---|---|---|
| **Tor Browser** | pełny Tor, najlepsza izolacja i fingerprinting protection | domyślny wybór do .onion |
| Brave (Private Window with Tor) | Tor w Chromium, wygodne | słabsza izolacja niż Tor Browser |
| Mullvad Browser | anty-fingerprint klasy Tor Browser, **bez** sieci Tor | Surface Web, łączyć z VPN |
| LibreWolf | hardening Firefoksa | codzienne przeglądanie, bez Tor |
| Onion Browser (iOS) / Orbot (Android) | Tor na mobile | tylko weryfikacja |

**Do BALD nie potrzebujesz przeglądarki, a transportu.** Przeglądarka to UI; silnik potrzebuje
klienta HTTP, który umie wyjść przez Tor. Trzy opcje:

1. **Systemowy Tor** (najprościej): daemon na `127.0.0.1:9050`, `reqwest` z proxy `socks5h://`
   (DNS przez Tor, bez wycieku).
2. **`arti`** (czysty Rust): wbudowany klient Tor, bez instalowania demona — dobra opcja dla Tauri,
   ale większy binarny i wolniejszy start.
3. **`tor` + `tor-rtcompat`**: biblioteki z ekosystemu ArTi, gdy potrzebna kontrola nad circuits.

```rust
pub enum Transport {
    Direct,
    TorSocks { addr: SocketAddr },   // socks5h = remote DNS
    Arti,                            // feature = "tor"
}
```

Zasady:

- `.onion` **tylko** przez Tor; nigdy przez Direct (nie ma DNS).
- Adresy v3 są self-authenticating (brak CA), ale to nie chroni przed scamem — dlatego hash treści
  i archiwizacja zostają obowiązkowe.
- **Nie routujemy przez Tor zapytań do Brave/Tavily/OpenAI** — ich edge blokuje lub flaguje exits;
  Tor dotyczy wyłącznie pobierania onionów.
- Timeouty 20–30 s, osobny limit równoległości (2–3), agresywny cache.
- Osobna przestrzeń indeksu: `documents.onion = 1`, żeby nie mieszać Surface Web z onionem.

**Wyszukiwarki onion (darmowe, jakość bardzo nierówna):** **Ahmia** (najbardziej sensowna, filtruje
treści nielegalne, open source, indeks clearnet + onion), Torch, Haystak, OnionLand. Nie mają
stabilnych API — to HTML scraping, więc preset musi być „best effort” i łatwo się psuje. Wiele
wyników to scam/martwe linki.

**Alternatywy dla „ocenzurowanych informacji” (często lepsze niż onion):**

| Sieć | Co daje | Werdykt dla BALD |
|---|---|---|
| IPFS / Arweave | trwałość treści, content-addressing | ○ źródło dokumentów, nie anonimowość |
| Nostr | relaye odporne na deplatforming | ○ kanał narracji, nie anonymity |
| I2P (eepsites) | anonymizing overlay, Java | ✗ v1 (ciężki sidecar, niszowy) |
| Telegram (eksporty) | tam żyje wiele narracji (Schwurbelarchiv) | ○ import offline, nie live crawl |
| Wayback / Common Crawl | archiwalna „prawda” o usuniętych treściach | ✓ rdzeń |

**Prawo i etyka:** Tor jest legalny w większości krajów, ale nie wszędzie; treść nielegalna pozostaje
nielegalna niezależnie od sieci. Obowiązuje blocklist kategorii (CSAM, rynki narkotykowe, materiały
operacyjne), zakaz masowego crawlowania onionów i ostrzeżenie przy pierwszym włączeniu.

**Decyzja:** Tor/onion **poza v1**. Wchodzi jako opt-in w v1.1 (`bse-transport`, feature `tor`),
po M4, z własnym namespace indeksu i testami. Nie bundlujemy Tor Browsera; linki zewnętrzne
otwieramy w systemowej przeglądarce.

### 3.6 Warstwy JFP: seed-core i jfp-edge-radar

Oba komponenty są realne i dobre — ale **nie robią tego, do czego zostały przypisane** w propozycji
integracji. Poniżej stan faktyczny z kodu (seed-core 0.1.6, edge-radar 0.5.0):

| Komponent | Co naprawdę robi | Gdzie pasuje | Gdzie **nie** pasuje |
|---|---|---|---|
| **seed-core 0.1.6** | Gate akcji agenta: `CandidateAction` (FileRead/Write, CommandExec, NetworkFetch, BrowserAction, PlanStep) → `DeciderVerdict` (Approved/Veto/Escalate); LID, ETM, streak-guard; łańcuch SHA-256; reguły **wbudowane** | action gate dla własnych akcji BALD: fetch, eksport, otwarcie linku, instalacja modelu; audit chain; veto SSRF dla `BrowserAction` | **nie jest** klasyfikatorem zapytań o osoby; **nie ma** citation validatora; **brak publicznego API reguł** — nowe reguły wymagają upstreamu lub forka |
| **jfp-edge-radar 0.5.0** | **Ingress** L7 reverse proxy: kanonizacja URL, session tracker, ASA (boty), tarpit, nftables, proof log, Prometheus, HUD | tryb daemon/serwer: ochrona `bse-api`, rate limit, HUD dla ops | **nie jest egress SSRF shield** (README: „HTTP L7 reverse proxy and sensor”); w Tauri nie ma ingressu; jako obowiązkowy hop = latencja + SPOF |

**Jak to wpiąć poprawnie:**

1. **`bse-guard`** — nasz cienki crate z traitami, nie „seeded” na siłę:
   ```rust
   pub trait Gate: Send + Sync {
       fn check(&self, ctx: &GateCtx) -> GateVerdict;   // synchroniczny, bez I/O
   }
   // RulesGate     — person/PII, wzorce PL/EN, normalizacja (µs)
   // CitationGate  — quote_span → hash → dokument (deterministyczny)
   // SeedGate      — adapter seed-core dla AKCJI (feature "seed-gate")
   // LlmGate       — semantyczna druga linia (ms), tylko dla ambiguous
   ```
   Łańcuch `Rules → Llm → Seed(action)`; każda decyzja trafia do **jednego** append-only logu
   z hash chain. Nie twierdzimy, że 15 µs dotyczy nowych reguł — dotyczy istniejącego zestawu.
2. **Person Guard** — deterministyka łapie wzorce („imię + adres”, „PESEL”, „gdzie mieszka”),
   ale nie intencję parafrazy. Dlatego `LlmGate` zostaje jako druga linia, a `RulesGate` jest
   **prefiltrem**, nie jedyną linią obrony. To różnica między „szybko” a „bezpiecznie”.
3. **Citation Gate** — seed-core nie zna pojęcia „claim” ani „cytat”, więc walidator piszemy
   natywnie: normalizacja cytatu → SHA-256 → porównanie z hashem fragmentu dokumentu; fuzzy tylko
   z progiem i flagą `approx`. Seed-core może podpisać **decyzję** (audit), nie treść.
4. **SSRF** — miejsce na to jest w `bse-fetch`, nie w gatewayu: resolve → odrzuć loopback,
   link-local, RFC1918, `169.254.169.254`, metadata IPv6; ponowna weryfikacja po każdym redirectcie
   (DNS rebinding); limit redirectów; allowlist schematów. seed-core może dodatkowo zawetować
   `BrowserAction` na poziomie narzędzi.
5. **`bse-gateway`** — tylko `feature = "server"`, tylko tryb daemon: `RadarServer` przed `axum`
   albo sidecar; HUD i Prometheus dla ops. W buildzie desktopowym crate nie istnieje.

**Licencje (blokujące dla decyzji o publikacji):**

- **seed-core**: darmowy wyłącznie do użytku osobistego, edukacyjnego i niekomercyjnego;
  użycie komercyjne wymaga płatnej licencji. To **nie jest** licencja OSI — nie można publikować
  BALD na MIT/Apache z twardą zależnością od seed-core.
- **jfp-edge-radar**: darmowy do 10 mln USD rocznego przychodu, z atrybucją; powyżej — licencja
  komercyjna.
- Dlatego oba są **opcjonalnymi adapterami za feature flagami**, a domyślny build BALD jest czysty
  licencyjnie. Użytkownik świadomie włącza JFP (i akceptuje jego warunki) albo nie.

**Wersjonowanie:** `seed-core = "0.1.6"` (crates.io), `jfp-edge-radar` z path/git — 0.5.0 nie jest
jeszcze opublikowane na crates.io (potwierdzone w release notes). Oba za traitem, żeby wymiana
implementacji nie dotykała pipeline'u.

**Korekta struktury workspace:** propozycja w pytaniu usuwa `bse-config`, `bse-llm`, `bse-fetch`,
`bse-index`, `bse-classify`, `bse-graph`, `bse-cli` i `bse-api` — bez nich projekt się nie zbuduje.
Docelowe drzewo jest w §3.2 (z `bse-guard`, `bse-transport`, `bse-gateway` dodanymi, nie zamiast).

---

## 4. Warstwa providerów (serce „prostoty”)

### 4.1 Kontrakt

```rust
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn kind(&self) -> ProviderKind;
    // Search | Archive | Academic | FactCheck | Decentralized | Render | Llm
    fn capabilities(&self) -> Capabilities;  // języki, max_results, koszt, rate limit, auth
    async fn health(&self) -> Health;         // tanie wywołanie testowe
    async fn search(&self, q: &SearchQuery, ctx: &CallCtx)
        -> Result<Vec<RawHit>, ProviderError>;
}

pub enum ProviderError {
    Auth, RateLimited { retry_after: Option<Duration> },
    Budget, Timeout, Upstream(String), Unsupported,
}
```

`CallCtx` niesie budżet pozostały w zapytaniu, deadline, korelację trace i licznik kosztu.
Każdy adapter **sam raportuje koszt** (na podstawie cennika z presetu), a nie pipeline.

`ProviderKind::Render` to osobna kategoria: nie „szuka”, tylko **pobiera i renderuje** stronę,
gdy zwykły fetch dostanie 403/JS-shell. `Decentralized` to sieci P2P (YaCy).

### 4.2 Katalog presetów (wbudowany, wersjonowany)

**Chmurowe znane:**

| Provider | Typ | Klucz | Uwagi |
|---|---|---|---|
| Brave Search API | search | tak | niezależny indeks, płatne progi |
| Tavily | search | tak | wyniki „AI-ready”, opcjonalny extract |
| Exa | search | tak | wyszukiwanie neuronowe, dobre do pojęć |
| SerpAPI | search | tak | drogi, fallback Google |
| Google CSE | search | tak + cx | 100 zapytań/dzień gratis |
| Mojeek | search | tak | niezależny indeks, pokrycie nisz |
| Marginalia | search | tak/limit | mały/niezależny web — kluczowy dla tematu |
| Oxylabs / Firecrawl / Browserless | render | tak | JS + anti-bot; tylko per-domena, drogie |
| Google Fact Check Tools | factcheck | tak | ClaimReview, darmowe |

**Lokalne / otwarte:**

| Provider | Typ | Klucz | Uwagi |
|---|---|---|---|
| SearXNG | search | brak | self-host, meta, domyślny profil lokalny |
| YaCy | decentralized | brak | P2P (Java sidecar), jakość zmienna — **opcja, nie rdzeń** |
| Wayback CDX/Save | archive | brak | snapshoty, treści usunięte |
| Common Crawl | archive | brak (S3) | masowe WARC, tryb batch |
| archive.today | archive | brak | tylko link/redirect, brak API |
| OpenAlex / Crossref | academic | brak (polite pool) | metadane publikacji |
| Semantic Scholar | academic | opcjonalny | abstrakty, cytowania |
| arXiv / PubMed | academic | brak | preprinty / medycyna |
| GDELT | news | brak | strumień wydarzeń, wielojęzyczny |
| Wikidata SPARQL | meta | brak | encje, daty, powiązania |
| Demagog / Konkret24 | factcheck | brak | PL fact-checki (także do eval) |
| Reddit / HN Algolia | social | OAuth / brak | ślad narracji (nie osoby) |
| RSS/Atom | feeds | brak | własne kanały użytkownika |

**Własny provider:** dowolny endpoint REST lub LLM zgodny z OpenAI — patrz §4.5.

**Bing Search API nie jest planowany** — Microsoft wycofał go w 2025 r.

### 4.3 Zarządzanie kluczami i providerami (UI + CLI)

- **Preset → dodaj → test → włącz.** Formularz generowany z `Capabilities` (auth: header/query/OAuth).
- **Klucze write-only**: API nigdy nie zwraca klucza; UI pokazuje `••••last4` + status.
- **Przechowywanie**: systemowy keyring (`keyring`) → fallback `age`-encrypted plik → `.env` w dev.
  W pamięci tylko `SecretString`, zeroizacja przy drop.
- **Auto-discovery lokalnych LLM** (opt-in): probe `127.0.0.1:11434` (Ollama), `:1234` (LM Studio),
  `:8080` (llama.cpp), `:8000` (vLLM) — jedno kliknięcie „użyj lokalnego modelu”.
- **Mierniki**: koszt dziś / limit, skuteczność (wyniki na zapytanie), p95 latencji, błędy 24h.
- **Priorytety i failover**: kolejność per kategoria; circuit breaker po 3 błędach; auto-degradacja.
- **Profile**: `local` (domyślny), `hybrid`, `cloud` — przełącznik w UI, nie konfiguracja w pliku.

### 4.4 Odporność

- Retry z jitterem tylko dla 429/5xx/timeout; nigdy dla 401/403.
- Rate limit per provider (`governor`) + globalny limit równoległości.
- Timeouty: search 8 s, fetch 15 s, render 25 s, LLM 60 s (konfigurowalne).
- Częściowe wyniki zawsze wracają — brak providera to ostrzeżenie, nie błąd zapytania.
- **Render provider** (Oxylabs/Firecrawl) jest wołany tylko, gdy: brak treści po fetchu **lub**
  status 403/429 **lub** domena jest na liście „wymaga JS”. Inaczej = palenie budżetu.

### 4.5 „Dodaj providera” — deklaratywna specyfikacja

Zamiast pisać kod dla każdego API, użytkownik dodaje providera **plikiem JSON** (lub formularzem
w UI, który ten JSON generuje). Zero rekompilacji, zero pluginów.

```json
{
  "id": "moj-news",
  "label": "Mój serwis",
  "kind": "search",
  "auth": { "type": "bearer", "secret_ref": "moj-news" },
  "request": {
    "method": "GET",
    "url": "https://api.example.com/search?q={query}&lang={lang}&limit={limit}",
    "headers": { "Accept": "application/json" }
  },
  "mapping": {
    "items": "$.results[*]",
    "url": "$.link",
    "title": "$.title",
    "snippet": "$.description",
    "published_at": "$.date",
    "author": "$.author"
  },
  "pagination": { "param": "page", "max_pages": 3 },
  "limits": { "rps": 2, "timeout_ms": 8000, "cost_per_call_usd": 0.001 }
}
```

Zasady:

- **Tylko HTTPS**, szablony `{query}`/`{lang}`/`{limit}` escapowane; brak lokalnych adresów
  w trybie chmurowym (SSRF).
- **Walidacja przed zapisem**: kompilacja JSONPath, testowe zapytanie, pokazanie zmapowanych wyników.
- **Sekrety przez referencję**, nigdy inline w JSON.
- **Limity twarde**: rozmiar odpowiedzi 5 MB, timeout, rps, koszt.
- **Escape hatch (post-MVP)**: plugin WASM (Extism) dla OAuth2 i nietypowej paginacji — sandbox,
  bez dostępu do sieci poza zadeklarowanymi hostami.
- **LLM „custom”** to ten sam mechanizm: `base_url` + `model` + klucz, protokół OpenAI-compatible.

---

## 5. Warstwa LLM

### 5.1 Adaptery

1. **OpenAI-compatible** (jeden adapter): OpenAI, OpenRouter, Groq, Mistral, DeepSeek, xAI,
   Together, vLLM, llama.cpp server, LM Studio.
2. **Anthropic** (Messages API) — natywnie, bo inny format.
3. **Gemini** — natywnie (structured output).
4. **Embedded** (opcjonalnie, feature flag): `candle` / `mistral.rs` — pełna lokalność bez serwera.

### 5.2 Structured output

- Wszystkie odpowiedzi jako JSON z `schemars` → walidacja `jsonschema`.
- Dwa poziomy naprawy: (a) re-prompt z błędem walidacji, (b) repair parser; po 2 próbach → fallback
  na model lokalny lub wynik bez LLM (same cytaty).
- `temperature 0.1–0.3`, `max_tokens` twardo ograniczony.

### 5.3 Prompt injection (krytyczne — treść z sieci jest wroga)

- Treść źródła zawsze w bloku `<untrusted_source>`, nigdy w instrukcjach.
- Model **nie ma narzędzi** w krokach ekstrakcji i syntezy.
- Strip: zero-width, HTML comments, „ignore previous instructions” → oznacz jako podejrzane.
- Wynik walidowany schematem; cytaty weryfikowane substringiem w treści źródła (hash + fuzzy 0.9).
- Logowanie incydentów injection do metryk.

### 5.4 Budżet

- Domyślnie: **$0,05/zapytanie**, **$2/dzień**, konfigurowalne.
- Lokalny model = koszt 0 → domyślny profil.
- Twarde zatrzymanie przed wywołaniem, nie po.

### 5.5 Klasyfikacja: teoria, wzmianka, obalenie

To najtrudniejsza część i **musi być zaprojektowana, nie „wyjdzie z promptu”**.

Definicja robocza (za badaniami nad TelConGBERT): **teoria spiskowa** = twierdzenie o **sekretnym
działaniu potężnych aktorów** zmierzającym do **ukrytego, złego celu**, przedstawiane jako fakt
mimo braku dowodów. Definicja jest wąska celowo — odsiewa „zwykłe” opinie i pojedyncze oskarżenia.

Trzy etapy (kaskada):

1. **Reguły + leksykon** (tanio, deterministycznie): frazy typu „ukrywają”, „elity”, „nie chcą,
   żebyś wiedział”, „plandemia”, „wielki reset”, „za tym stoi”. Wynik: `suspected` / `not`.
2. **LLM (structured output)**: zwraca `{is_conspiracy, actors[], secret, goal, evidence_cited[],
   stance}` + cytat uzasadniający.
3. **Encoder (opcjonalnie)**: mały model wielojęzyczny (MiniLM/XLM-R) dotrenowany na zebranych
   danych, eksport ONNX, uruchamiany przez `ort`. Tylko gdy zbiór treningowy jest wystarczający;
   do tego czasu etap 2 wystarcza.

**Stance jest obowiązkowy** — bez niego indeks miesza treści:

| Stance | Znaczenie | Przykład |
|---|---|---|
| `reports` | relacjonuje teorię bez oceny | artykuł „kto w to wierzy i dlaczego” |
| `endorses` | popiera teorię | post „oni naprawdę to ukrywają” |
| `debunks` | obala / wyjaśnia | fact-check, artykuł naukowy |
| `mentions` | tylko wzmianka | przegląd mediów, commentary |

Bez rozróżnienia `debunks` od `endorses` wyszukiwarka staje się agregatorem dezinformacji.
Dlatego stance jest polem w bazie, filtrem w UI i składnikiem rankingu.

**Dane referencyjne / ground truth:**

- **Schwurbelarchiv** — niemiecki zbiór treści z Telegrama (metoda kuli śnieżnej, walidacja
  TelConGBERT). Świetny do **ewaluacji klasyfikatora DE**, nie do runtime'u; sprawdzić licencję
  przed użyciem, nie redystrybuować.
- **QAnon / QMap / QProofs** — inspiracja dla modelu grafu („dowód → narracja”), nie źródło do ingestii.
- **PL:** Demagog, Konkret24, AFP Polska jako źródła fact-checków + ręcznie zbudowany zbiór
  100–200 twierdzeń do eval (PL i EN).

**Uwaga:** TelConGBERT jest niemiecki. Dla PL/EN potrzebny model wielojęzyczny lub własny zbiór
treningowy. Nie zakładamy, że „BERT z paperskiego datasetu” zadziała out-of-the-box.

---

## 6. Pipeline wyszukiwania — szczegóły kroków

### 6.1 Planer zapytań

- Wejście: zapytanie użytkownika + język + profil.
- Wyjście: 2–6 podzapytań (fraza oryginalna, synonimy, nazwy instytucji, tytuły dokumentów, język EN).
- Reguły zawsze; LLM tylko wzbogaca (i tylko gdy budżet pozwala).
- Cache planu per znormalizowane zapytanie.

### 6.2 Guard (ochrona przed person-targeting)

- Heurystyki: imię+nazwisko + (adres | pracodawca | PESEL | „gdzie mieszka” | „zdjęcia”).
- Klasyfikator LLM (mały, lokalny) jako druga linia.
- Wynik: `allow` | `refuse_with_explanation` | `rewrite` (na temat twierdzenia, bez osoby).
- Log bez treści zapytania (RODO): tylko hash + kategoria.

### 6.3 Fan-out i fuzja

- Równoległe zapytania, `join_all` z `buffer_unordered`.
- **RRF** (Reciprocal Rank Fusion) jako baza — odporna na różne skale score'ów.
- Dedupe: normalizacja URL (tracking params, AMP, http/https, trailing slash) → hash treści →
  MinHash + LSH (progi 0.85/0.92) → wybór „najlepszej” kopii (najwyższy tier, najstarsza data).
- **Diversity**: kara za wyniki z jednej domeny/rodziny serwisów; metryka `source_diversity` w trace.

### 6.4 Pobieranie i ekstrakcja

- `reqwest` + rustls, UA identyfikujący bota, robots.txt respektowany, limity per domena.
- HTML → `dom_smoothie`/`readability` → czysty tekst + struktura nagłówków.
- PDF → ekstrakcja tekstu (opcjonalny feature, bo ciężki).
- **Render fallback**: 403/JS-shell → provider `render` (jeśli włączony i dozwolony dla domeny).
- Zapis: treść + hash + metadane + snapshot (Wayback Save jeśli dozwolone; inaczej CDX lookup).
- Wykrywanie zmian: porównanie hashów snapshotów → „usunięto akapit”, „zmieniono datę”.

### 6.5 Ekstrakcja twierdzeń

- Wejście: dokument (sekcje) + zapytanie.
- Wyjście: `Claim { text, type, actors[], quote_span, confidence }`.
- Wymóg: `quote_span` musi istnieć w dokumencie (walidacja).
- Klasyfikacja (kaskada z §5.5) + `stance` dokumentu.
- Agregacja: to samo twierdzenie w N dokumentach → klaster (embedding + string similarity),
  licznik wystąpień + pierwsza/ostatnia data.

### 6.6 Grading dowodów

- Reguły: tier, authority, niezależność (I0–I3), typ dokumentu, autor/data.
- LLM: relacja `popiera/przeczy/kontekst` + siła, **tylko z cytatem**.
- Wynik zapisywany z `model_version`; przeliczanie możliwe po zmianie modelu.
- **Single-source detection**: korroboracja liczona po rodzinach źródeł (domena + właściciel),
  nie po liczbie linków.

### 6.7 Ranking

```
score = 0.40 * RRF_norm
      + 0.20 * source_trust(tier, authority, independence)
      + 0.15 * corroboration(niezależne rodziny źródeł)
      + 0.10 * recency(decay 365 dni)
      + 0.10 * archive_availability
      + 0.05 * source_diversity
      - spam_penalty
```

Wagi w konfiguracji, zwracane w `trace`, żeby użytkownik widział „dlaczego to jest wysoko”.

### 6.8 Synteza

- Model dostaje **tylko** wyselekcjonowane fragmenty z ID źródeł.
- Format: teza → „co twierdzą źródła” → „co mówią dowody/fact-checki” → „czego brakuje”.
- Walidator: każde zdanie twierdzące musi mieć `[S1]`; inaczej zdanie trafia do sekcji „bez pokrycia”.
- Odpowiedź zawsze zawiera sekcję **„Czego nie udało się ustalić”** — to feature, nie wstyd.

### 6.9 Zapis grafu

- Upsert encji (rozstrzyganie aliasów: normalizacja nazw + Wikidata ID, fuzzy dopasowanie).
- Krawędzie z wagą = liczba niezależnych dokumentów (nie wystąpień).
- Przypisanie claim → narrative (klaster: cosine ≥ 0.85 + wspólni aktorzy).
- Wszystko w jednej transakcji z zapisem runu; graf jest **produktem ubocznym** pipeline'u,
  nie osobnym crawlerem.

---

## 7. Warstwa „ocenzurowane” (availability & diff)

To wyróżnik produktu, więc osobny moduł:

- **Availability probe**: live / archived / removed / paywalled / geoblocked.
- **Snapshot chain**: Wayback (CDX + Save), archive.today (link only, bez API), własny WARC.
- **Diff**: porównanie kolejnych snapshotów → usunięte akapity, zmienione nagłówki, zniknięte PDF-y.
- **Deplatforming log**: dokument obecny w archiwum, nieobecny live → `removed`, z datą wykrycia.
- **Search po usuniętych**: zapytania kierowane najpierw do CDX/Common Crawl, potem live.
- Etykieta w UI: „treść usunięta 2024-03-11, ostatni snapshot 2024-02-28”.

---

## 8. Przechowywanie i indeks

### 8.1 Podział (decyzja: SQLite w v1)

| Warstwa | Technologia | Rola |
|---|---|---|
| Relacyjna | **SQLite** (`sqlx`, bundled) | encje, runy, evidence, koszty, konfiguracja |
| Full-text | Tantivy | BM25, facety, szybkie wyszukiwanie lokalne |
| Wektory | HNSW (`hnsw_rs`) lub brute-force < 50k | podobieństwo twierdzeń, cache semantyczny |
| Graf | tabele krawędzi + `petgraph` | sąsiedztwo, centrality, klastry |
| Pliki | katalog aplikacji | snapshoty, WARC, PDF-y |
| Cache | moka | plany, wyniki, HTTP |

**Dlaczego SQLite, a nie Postgres:** Tauri = jeden instalator. Postgres wymaga serwera, portu,
haseł i migracji — użytkownik tego nie zrobi. SQLite + WAL obsłuży setki tysięcy dokumentów
i zapytania jednego użytkownika z zapasem. Postgres + pgvector wchodzi jako **tryb serwerowy**
(feature `server`, te same repozytoria, osobne migracje) dopiero, gdy pojawi się potrzeba
wieloużytkownikowa lub >1M dokumentów.

**Dlaczego Tantivy, a nie Elasticsearch/OpenSearch:** ES to serwis w JVM, 1 GB+ RAM i osobna
instalacja. Tantivy jest embedded, w Rust, z BM25 — dokładnie to, czego potrzebuje desktop.
Gdyby kiedyś potrzebny był zdalny klaster, `bse-index` ma trait, więc podmiana jest lokalna.

Dlaczego nie tylko wektory: BM25 jest tańszy, deterministyczny i lepszy dla nazw własnych;
wektory służą do klastrowania twierdzeń i semantycznego cache.

### 8.2 Schemat (skrót, dialekt SQLite)

```sql
create table sources (
  id text primary key,
  domain text not null unique,
  tier integer not null check (tier between 1 and 6),
  authority text not null default 'D',       -- A | B | C | D
  independence integer not null default 0,   -- 0..3
  policy text not null default 'allowed',
  notes text
);

create table documents (
  id text primary key,
  url text not null,
  url_canonical text not null unique,
  content_hash blob not null,
  title text, author text,
  published_at text,                          -- ISO-8601 UTC
  lang text,
  source_id text references sources(id),
  stance text not null default 'mentions',    -- reports | endorses | debunks | mentions
  availability text not null default 'unknown',
  extracted_text text,
  embedding blob,                             -- f32[] jako bajty
  first_seen text not null default (datetime('now')),
  last_checked text
);

create table claims (
  id text primary key,
  text text not null,
  lang text,
  kind text,                                  -- fact | causal | concealment | conspiracy | prophecy
  status text not null default 'unverified',
  embedding blob,
  first_seen text, last_seen text
);

create table evidence (
  claim_id text references claims(id),
  document_id text references documents(id),
  relation text not null,                     -- supports | contradicts | context | repetition
  strength real not null,
  quote text not null,
  model_version text not null,
  prompt_version text not null,
  primary key (claim_id, document_id, relation)
);

create table entities (
  id text primary key,
  kind text not null,                         -- institution | event | document | public_actor | group
  name text not null,
  aliases text,
  wikidata_id text,
  is_public_figure integer not null default 0,
  is_private_individual integer not null default 0   -- 1 = blokada agregacji
);

create table edges (
  src text not null,
  dst text not null,
  relation text not null,                     -- mentions | supports | contradicts | repeats | precedes | co_occurs
  weight real not null default 1.0,
  claim_id text,
  document_id text,
  first_seen text,
  primary key (src, dst, relation)
);

create table runs (
  id text primary key,
  query_hash blob not null,
  profile text not null,
  cost_usd real not null default 0,
  latency_ms integer,
  trace text not null,                        -- JSON
  created_at text not null default (datetime('now'))
);

create index idx_documents_published on documents(published_at);
create index idx_documents_source on documents(source_id);
create index idx_edges_src on edges(src);
create index idx_edges_dst on edges(dst);
```

Indeksy wektorowe: HNSW w osobnym pliku (`vectors.hnsw`), przebudowa po batchu; do 50k wektorów
brute-force w pamięci jest szybszy i prostszy (bez ryzyka rozjazdu indeksu).

### 8.3 Retencja

- Surowy HTML: 90 dni (potem tylko tekst + hash + snapshot link).
- Zapytania: 30 dni (hash + metryki), treść tylko jeśli użytkownik włączy „historię”.
- Klucze: nigdy w bazie (keyring/plik zaszyfrowany).
- Baza: jeden plik `bald.db` + katalog `index/` + `snapshots/` — łatwy backup, łatwy export.

---

## 9. API

### 9.1 Endpointy

| Metoda | Ścieżka | Opis |
|---|---|---|
| POST | `/v1/search` | pełne zapytanie → claims + documents + trace |
| POST | `/v1/search/stream` | SSE: progres po providerach, wyniki częściowe |
| GET | `/v1/claims/{id}` | evidence ledger twierdzenia + timeline + aktorzy |
| GET | `/v1/documents/{id}` | metadane, snapshoty, diff |
| GET | `/v1/entities/{id}` | sąsiedztwo w grafie (bez osób prywatnych) |
| GET | `/v1/narratives/{id}` | klaster narracji, oś czasu, mutacje |
| GET | `/v1/providers` | katalog + status + koszty (bez kluczy) |
| POST | `/v1/providers` | dodaj providera (preset lub custom spec) |
| POST | `/v1/providers/{id}/test` | test połączenia |
| PUT | `/v1/providers/{id}` | włącz/wyłącz, priorytet, budżet |
| POST | `/v1/keys/{provider}` | zapis klucza (write-only) |
| DELETE | `/v1/keys/{provider}` | usunięcie klucza |
| GET | `/v1/models` | modele lokalne (discovery) + chmurowe (presety) |
| GET | `/v1/usage` | koszty, latencje, błędy |
| GET | `/v1/health` | status całości |

Auth API: token Bearer (lokalnie opcjonalny), CORS tylko `127.0.0.1`, rate limit globalny.
W trybie Tauri endpointy są dostępne przez IPC; HTTP istnieje dla `bse-daemon` i CLI.

### 9.2 Przykład

```http
POST /v1/search
Authorization: Bearer <token>
Content-Type: application/json

{
  "query": "teoria o ...",
  "lang": "pl",
  "profile": "hybrid",
  "depth": "standard",
  "budget_usd": 0.05,
  "include_removed": true
}
```

```json
{
  "run_id": "…",
  "answer": {
    "summary": "…",
    "sections": [
      { "kind": "claims", "sentences": [ { "text": "…", "citations": ["S1","S4"] } ] },
      { "kind": "counter_evidence", "sentences": [ … ] },
      { "kind": "gaps", "sentences": [ … ] }
    ],
    "unsupported": []
  },
  "claims": [
    { "id": "…", "text": "…", "status": "partially_supported",
      "evidence": [ { "document_id": "…", "relation": "supports",
                      "tier": 4, "independence": 2, "quote": "…" } ] }
  ],
  "documents": [
    { "id": "…", "title": "…", "url": "…", "tier": 3, "authority": "A",
      "stance": "debunks", "availability": "removed",
      "archived_at": "2024-02-28", "single_source": false,
      "why": { "rrf": 0.71, "trust": 0.8, "corroboration": 3, "diversity": 0.6 } }
  ],
  "trace": { "providers": [ { "id": "brave", "hits": 12, "cost_usd": 0.002 } ],
             "cost_usd": 0.011, "latency_ms": 4200, "source_diversity": 0.6 }
}
```

---

## 10. UI (Tauri + Leptos)

Pięć ekranów na start:

1. **Search** — pole, przełącznik profilu (local/hybrid/cloud), lista twierdzeń z rozwinięciem dowodów,
   filtry stance/availability/tier.
2. **Claim view** — timeline, źródła per relacja, fact-checki, aktorzy, „czego brakuje”.
3. **Providers** — kafelki z statusem, kosztem, testem, kluczem (maskowanym), discovery lokalnych LLM,
   przycisk **„Dodaj providera”** (preset lub własny JSON + test).
4. **Graph** — sąsiedztwo twierdzenia/byt (force-directed, limit 100 węzłów), oś czasu narracji.
5. **Runs/Usage** — koszty dzienne, latencje, błędy, zużycie per provider, top zapytania (hash).

Technologia: **Leptos 0.7+ (CSR/WASM)** w WebView Tauri; komponenty dzielą typy z `bse-core`
(ten sam crate, feature `ui-types`). Brak SSR w v1 (Tauri go nie potrzebuje), brak Node w toolchainie
(trunk + wasm-bindgen). Wykres grafu: `petgraph` → serializacja do JSON → render w SVG/canvas
(własny komponent, bez ciężkich bibliotek JS).

---

## 11. Bezpieczeństwo, etyka, prawo

### 11.1 Ochrona osób (produktowa, nie „postscriptum”)

- Brak możliwości wyszukania „Kto to jest X” — guard blokuje lub przepisuje zapytanie.
- Brak agregacji PII w wynikach; dane osobowe w źródłach nie są wyciągane do pól strukturalnych.
- Brak „listy wyznawców” / „kto w to wierzy” — to profilowanie.
- **Graf:** osoby publiczne tylko jako aktorzy narracji; osoby prywatne `is_private_individual`
  → brak strony bytu, brak agregacji, tylko cytat.
- Zasada: **temat tak, osoba nie**.

### 11.2 Etyka publikacji

- Każdy claim jawnie oznaczony jako claim, nie fakt.
- Fact-checki pokazywane razem z kontrdowodami, nie zamiast nich.
- Sekcja „gaps” obowiązkowa.
- Stance widoczny przy każdym dokumencie — bez niego „obalenie” wygląda jak „poparcie”.
- Brak „truth score” — tylko komponenty, tier, autorytet i niezależność (anty-overconfidence).

### 11.3 Prawo

- **Zniesławienie**: nie etykietujemy osób; oceniamy dokumenty i twierdzenia.
- **RODO**: minimalizacja, hash zapytań, retencja 30 dni, brak profilowania.
- **ToS providerów**: respektować limity i zakazy cache'owania; klucze per użytkownik.
- **Crawl**: robots.txt, rate limit, identyfikacja bota, preferencja API > scraping.
- **Render/proxy (Oxylabs itp.)**: tylko domeny dozwolone, świadome włączenie, budżet; to narzędzie
  do pokonywania blokad technicznych, nie do masowego kopiowania płatnych treści.
- **Datasety (Schwurbelarchiv itp.)**: sprawdzić licencję przed użyciem; domyślnie tylko eval,
  bez redystrybucji w aplikacji.
- **Treści nielegalne**: blocklist kategorii, brak indeksowania CSAM/ekstremizmu operacyjnego,
  procedura zgłoszeń.

### 11.4 Bezpieczeństwo systemu

- Sekrety poza logami (`secrecy` + filtr `tracing`).
- **SSRF (w `bse-fetch`, nie w gatewayu):** odrzucenie loopback, link-local, RFC1918, `::1`,
  `169.254.169.254`, metadata IPv6; ponowna weryfikacja po redirectcie (DNS rebinding); limit
  redirectów; allowlist schematów; custom provider bez localhost.
- **Action gate (opcjonalnie seed-core):** każda własna akcja BALD (fetch, eksport, otwarcie linku,
  instalacja modelu) przechodzi przez `Gate` przed wykonaniem; werdykt + SHA-256 do audit logu.
- **Ingress (tylko daemon):** `bse-gateway` przed `axum` — rate limit, bot/scanner detection,
  tarpit, nftables; w Tauri ingress nie istnieje.
- Sanityzacja HTML przed UI (Leptos auto-escape, brak `inner_html` na treści źródła).
- Rate limit API, rozmiar body, limity głębokości JSON.
- Tauri: minimalne capabilities, CSP, brak zdalnych źródeł w WebView.

---

## 12. Wydajność i koszty

| Metryka | Cel MVP |
|---|---|
| p50 latencja (local, bez syntezy) | < 1,5 s |
| p95 latencja (hybrid + synteza) | < 10 s |
| Koszt/zapytanie (hybrid, domyślny model) | < $0,03 |
| Trafność cytowań (walidacja) | ≥ 95% |
| Precyzja stance na eval | ≥ 90% |
| Pokrycie archiwum dla usuniętych | ≥ 70% |
| Duplikaty w wynikach | < 5% |
| Różnorodność źródeł (entropia domen) | ≥ 0,6 |
| Guard p99 (RulesGate, bez I/O) | < 1 ms |
| SSRF suite (znane wektory + redirecty) | 100% |
| Czas startu aplikacji (Tauri) | < 2 s |
| Rozmiar instalatora | < 40 MB (bez modeli) |

Optymalizacje: cache planów i wyników, `buffer_unordered` z limitem, wczesne odcinanie
providerów bez wyników, embeddingi liczone tylko dla nowych dokumentów, batch LLM,
leniwe ładowanie grafu (tylko widoczne węzły).

### 12.2 Model wartości i licencjonowania (Professional / Enterprise Grade)

Narzędzie pozycjonowane jest jako profesjonalna platforma wywiadu narracyjnego, fact-checkingu i śledztw dokumentacyjnych, konkurująca możliwościami z systemami klasy Maltego, Dataminr czy Social Links.

**Cennik profesjonalny (B2B / Investigator / Institutional):**
1. **Free Evaluation (100 zapytań badawczych):**
   - Wstępna wersja ewaluacyjna pozwalająca zweryfikować jakość ekstrakcji claimów, archiwów i dowodów.
2. **Pakiety profesjonalne (Investigator & Media):**
   - **Pakiet Analityczny (Investigator Pass - 500 zapytań):** **$99.00 (~399 zł)**
     - Pełna ekstrakcja twierdzeń, weryfikacja cytowań, eksport raportów dowodowych.
   - **Pakiet Dziennikarski / Śledczy (Pro Researcher - 2 500 zapytań):** **$250.00 (~990 zł)**
     - Generowanie grafu powiązań (petgraph SVG/PDF), zaawansowane archiwa (Wayback diffs), kryptograficzny stempel `seed-core` SHA-256 dla sądu/redakcji.
   - **Licencja Instytucjonalna / Kancelaria (Enterprise Daemon - 10 000 zapytań):** **$490.00 – $990.00 (~2 000 – 4 000 zł)**
     - Pełny tryb serwerowy daemon, tarcza ochronna `jfp-edge-radar` (ASA sniffer, anti-scraping), dedykowana baza SQLite/Tantivy.

3. **Dedykowany Partner VIP Pass (BaldTV & Kluczowi Partnerzy):**
   - Nielimitowana licencja VIP przekazana bezpłatnie jako narzędzie badawcze o wartości komercyjnej $1 000+.
   - Ekspozycja marki twórcy: logotyp autora (Jaro / Viper AIOS) w ekranie startowym i stopce raportów generowanych przez silnik.
   - Synergia wizerunkowa: narzędzie buduje prestiż badacza (unikalna technologia `seed-core` niedostępna na rynku konsumenckim) w zamian za publiczne wskazanie autora rozwiązania.

4. **Tryb BYOK (Bring Your Own Key):**
   - W przypadku pracy na w 100% lokalnym sprzęcie (własna Ollama, własny SearXNG) narzędzie nie nalicza opłat chmurowych.

5. **Architektura anty-tamper (Zabezpieczenie licznika):**
   - Stan licznika i Machine ID są chronione kryptograficznie (`seed-core` / HMAC SHA-256) przed ręczną manipulacją w plikach konfiguracyjnych lub SQLite.

---

## 13. Testy i ewaluacja

- **Jednostkowe**: normalizacja URL, dedupe, RRF, walidator cytowań, guard, mapowanie custom providera.
- **Integracyjne**: `wiremock` — nagrane odpowiedzi providerów (kasety), brak ruchu sieciowego w CI.
- **Property-based**: `proptest` dla parserów i kanonizacji.
- **Snapshot**: `insta` dla promptów i kształtu odpowiedzi.
- **Eval harness** (`eval/`):
  - 50 twierdzeń PL+EN: ≥1 źródło T1–T4, ≥1 fact-check, ≥1 archiwum, poprawny stance;
  - **Schwurbelarchiv** (DE, subset): precyzja klasyfikatora vs. etykiety referencyjne (licencja!);
  - **Demagog / Konkret24 / ClaimReview**: zgodność z verdictami fact-checków;
  - metryki liczone automatycznie przy każdym PR.
- **Red team**: prompt injection w treściach źródeł; test „czy model wykonał instrukcję z HTML”.
- **Regresje kosztowe**: budżet na PR (limit tokenów w testach).
- **UI smoke**: uruchomienie Tauri w CI (headless WebDriver) — tylko ścieżki krytyczne.
- **Guard**: 200 zapytań person-targeted (PL/EN, w tym obfuskacja) + 200 benign; zero false negatives
  na znanych wzorcach; `LlmGate` mierzony osobno (nie może być wąskim gardłem).
- **SSRF**: `127.0.0.1`, `::1`, `10/8`, `172.16/12`, `192.168/16`, `169.254.169.254`, metadata IPv6,
  `file://`, redirect na metadata, DNS rebinding.
- **Audit**: próba edycji logu wykryta przez hash chain; eksport do weryfikacji. 

---

## 14. Roadmapa

| Faza | Czas | Zakres | Kryterium akceptacji |
|---|---|---|---|
| 0. Fundament | 3–4 dni | workspace, `bse-core`, `bse-store` (SQLite), config+sekrety, trait Provider, CLI, SearXNG + Brave, szkielet Tauri, `bse-guard` (RulesGate) | `bald search "..."` zwraca wyniki z 2 źródeł; okno Tauri się otwiera; guard odrzuca testowy doxxing |
| 1. Indeks i dedupe | 1 tydz. | Tantivy, fetch+extract, **SSRF policy**, kanonizacja, MinHash/LSH, RRF, cache, diversity | te same wyniki z 3 providerów → 1 wpis, p95 < 3 s; SSRF suite zielony |
| 2. LLM + claims | 2 tydz. | OpenAI-compatible + Ollama, structured output, ekstrakcja, klasyfikacja, stance, grading, **CitationGate + kaskada Rules→Llm** | każde zdanie ma źródło albo trafia do „unsupported”; stance obecny; cytat bez pokrycia odrzucony |
| 3. UI + providerzy | 1,5 tydz. | Tauri + Leptos: search, claim view, panel providerów, klucze write-only, discovery LLM, custom provider + test, **action gate (seed-core, feature)** | dodanie klucza i własnego providera w UI bez terminala; akcja bez werdyktu nie wykonuje się |
| 4. Archiwum + diff | 1 tydz. | Wayback CDX/Save, availability, diff snapshotów, „removed” w wynikach | usunięta strona oznaczona z datą i snapshotem |
| 5. Graf narracji | 1 tydz. | encje, aliasy, krawędzie, entity page, timeline narracji, widok grafu | sąsiedztwo twierdzenia do depth 2 bez osób prywatnych |
| 6. Eval + pakowanie | 1 tydz. | golden set, metryki, injection testy, **gateway w trybie daemon**, instalatory, updater, ADR-y | eval ≥ progi z §12, CI zielone, instalator działa |
| 7. Tor/onion (v1.1) | 1 tydz. | `bse-transport`, system Tor / `arti`, Ahmia, namespace onion, blocklist | onion fetch działa, Surface Web nietknięty, zero wycieków DNS |

Razem: **8–9 tygodni** pracy jednej osoby bez fazy 7 (kalendarzowo więcej przy review i poprawkach).
Kolejność faz 2–5 można przestawić; faza 0 i 1 są nienegocjowalne. Faza 7 jest opcjonalna i wchodzi
po M4.

**Stan realizacji:** fazy 0–2 są zamknięte dla rdzenia i CLI. Faza 2 dostarcza Ollamę i
OpenAI-compatible, structured output, stance, `CitationGate` i ledger; pełny grading dowodów,
UI i kaskada Rules→Llm w interfejsie pozostają w fazie 3.

### Kamienie milowe

- **M1 (koniec fazy 1)**: „działa jak wyszukiwarka” — szybko, bez duplikatów, w pełni lokalnie.
- **M2 (koniec fazy 3)**: „działa jak produkt” — klucz, provider, wynik z cytowaniami, UI desktop.
- **M3 (koniec fazy 5)**: „widać narrację” — graf, timeline, stance, archiwum.
- **M4 (koniec fazy 6)**: „gotowe do pokazania” — eval, instalator, dokumentacja, etyka.
- **M5 (faza 7, opcjonalnie)**: „poza Surface Web” — onion przez Tor, bez wycieków i bez wpływu
  na resztę indeksu.

---

## 15. Ryzyka

| Ryzyko | Wpływ | Mitygacja |
|---|---|---|
| Halucynacje w syntezie | wysoki | walidator cytowań, sekcja gaps, model lokalny do weryfikacji |
| Prompt injection ze źródeł | wysoki | treść jako dane, brak narzędzi, walidacja schematem, red team |
| Mylenie obalenia z poparciem | wysoki | obowiązkowy `stance`, filtr w UI, eval na fact-checkach |
| Koszty API | średni | budżety twarde, local-first, cache, tanie modele do ekstrakcji |
| Link rot / znikanie treści | średni | archiwizacja przy pierwszym pobraniu, diff, CDX |
| ToS / blokady providerów | średni | wielu providerów, respektowanie limitów, SearXNG jako fallback |
| Render/proxy (Oxylabs) — koszt i etyka | średni | off by default, per-domena allowlist, budżet |
| YaCy: mały indeks, spam, Java | niski | tylko jako opcjonalny provider, nie rdzeń |
| Zniesławienie / RODO | wysoki | brak person-targeting, ocena dokumentów nie osób, ADR prawny |
| Prywatność w grafie | wysoki | `is_private_individual` blokuje agregację i strony bytów |
| „Truth score” i nadużycia | średni | tylko komponenty, jawny tier, brak jednej liczby |
| Licencje datasetów (Schwurbelarchiv) | średni | eval only, weryfikacja licencji, brak redystrybucji |
| Rozjazd SQLite ↔ Postgres | średni | v1 tylko SQLite; Postgres jako feature w v2 z testami |
| Scope creep (crawler, mobile, social) | wysoki | twarde „nie” w v1, backlog po M4 |
| Jakość źródeł PL | średni | ręczna kuratacja top domen, presety regionalne, Demagog |

---

## 16. Decyzje

### 16.1 Podjęte

| # | Decyzja | Uzasadnienie |
|---|---|---|
| D1 | **Tauri 2** (desktop) | jeden instalator, brak serwera, klucze w procesie Rust |
| D2 | **Leptos (CSR/WASM)** | Tauri nie ma SSR; typy współdzielone z core; jeden język |
| D3 | **v1 bez własnego crawlera** | metasearch + archiwa + opcjonalny render; crawler to inny produkt |
| D4 | **PL + EN** | dwujęzyczne zapytania, UI PL z i18n od początku |
| D5 | **Presety chmurowe + lokalne + „dodaj providera”** | katalog znanych + deklaratywny custom |
| D6 | **SQLite zamiast Postgres** (v1) | zero infrastruktury; Postgres jako tryb serwerowy w v2 |
| D7 | **Tantivy zamiast Elasticsearch** | embedded, Rust, BM25, brak JVM |
| D8 | **Graf w SQLite + petgraph**, nie Neo4j | brak osobnego serwisu; eksport do Cypher/AGE później |
| D9 | **Brak Pythona w runtime** | ONNX (`ort`) lub LLM; Python tylko offline do treningu/eksportu |
| D10 | **YaCy i Oxylabs jako opcjonalne providery** | użyteczne, ale nie mogą być rdzeniem (jakość / koszt / ToS) |
| D11 | **Stance obowiązkowy** | bez tego narzędzie staje się agregatorem dezinformacji |
| D12 | **Autorytet A–D osobno od tieru** | „oficjalne” ≠ „prawdziwe”; trzy osie zamiast jednej oceny |
| D13 | **Tor/onion poza v1** | zmienia threat model i support; v1.1 jako opt-in `bse-transport` |
| D14 | **seed-core = action gate + audit**, nie klasyfikator zapytań | z kodu: gate akcji, brak pojęcia claim/cytat, reguły wbudowane |
| D15 | **edge-radar = ingress w daemonie**, nie egress | z kodu: reverse proxy L7; SSRF należy do `bse-fetch` |
| D16 | **JFP za feature flagami** | licencje: seed-core non-commercial, edge-radar <$10M; core czysty licencyjnie |
| D17 | **Model wartości: Professional / Enterprise + Partner VIP dla BaldTV** | Cennik profesjonalny ($99 / $250 / $990) dla analityków i redakcji, nielimitowany VIP Pass dla BaldTV w zamian za ekspozycję logo/autora, tamper-proof z `seed-core`, BYOK bez opłat |

### 16.2 Otwarte (nie blokują fazy 0)

1. **Licencja / publikacja**: prywatne, open-source, czy komercyjne API?
2. **Telemetria**: brak (domyślnie), czy anonimowe metryki jakości?
3. **Kanał aktualizacji**: GitHub Releases, własny serwer, czy brak auto-update w v1?
4. **Model do syntezy**: domyślnie lokalny (jaki?) czy chmurowy z fallbackiem?

---

## 17. Załącznik: przykładowa konfiguracja

```toml
[app]
profile = "local"            # local | hybrid | cloud
lang = ["pl", "en"]
telemetry = false

[storage]
backend = "sqlite"           # sqlite | postgres (feature "server")
path = "~/.local/share/bald/bald.db"

[security]
action_gate = "none"         # none | seed-core   (feature "seed-gate")
gateway = false              # tylko tryb daemon (feature "server-gateway")
audit_log = "~/.local/share/bald/audit.jsonl"
ssrf_strict = true

[transport]
tor = false                  # v1.1, opt-in
tor_socks = "127.0.0.1:9050"
onion_index = false

[profile.local]
local_only = true
budget_usd_per_query = 0.00
budget_usd_per_day = 0.00

[profile.hybrid]
local_only = false
budget_usd_per_query = 0.05
budget_usd_per_day = 2.00

[providers.searxng]
enabled = true
kind = "search"
base_url = "http://127.0.0.1:8888"
priority = 10

[providers.brave]
enabled = false
kind = "search"
api_key = { env = "BRAVE_API_KEY" }
priority = 20

[providers.yacy]
enabled = false
kind = "decentralized"
base_url = "http://127.0.0.1:8090"
priority = 40

[providers.oxylabs]
enabled = false
kind = "render"
api_key = { keyring = "bald/oxylabs" }
domains_allow = ["example.gov", "example-media.org"]
priority = 90

[providers.wayback]
enabled = true
kind = "archive"
priority = 5

[llm.default]
provider = "ollama"
model = "qwen2.5:14b-instruct"
base_url = "http://127.0.0.1:11434"

[llm.fallback]
provider = "openrouter"
model = "<dowolny model z katalogu>"
api_key = { keyring = "bald/openrouter" }

[guard]
person_targeting = "block"   # block | rewrite | warn

[rank]
w_rrf = 0.40
w_trust = 0.20
w_corroboration = 0.15
w_recency = 0.10
w_archive = 0.10
w_diversity = 0.05
```

---

## 18. Ocena zaproponowanego stacku (z inputu Jaro)

| Element | Werdykt | Co robimy |
|---|---|---|
| SearXNG | ✓ rdzeń | self-host, provider `search`, domyślny profil lokalny |
| Oxylabs / Scrapy | ✓ opcjonalnie | `render` provider, per-domena, off by default; Scrapy wypada (Python) |
| Wayback | ✓ rdzeń | `archive` provider + snapshot chain + diff |
| YaCy | ○ opcjonalnie | `decentralized` provider, Java sidecar, jakość zmienna |
| Elasticsearch / OpenSearch | ✗ | Tantivy (embedded, Rust) |
| Neo4j | ✗ w v1 | SQLite + petgraph; AGE/Cypher dopiero w trybie serwerowym |
| Python (spaCy/HF) | ✗ w runtime | LLM structured output + opcjonalny ONNX (`ort`) |
| TelConGBERT | ○ referencja | definicja + baseline; model DE, nie PL/EN |
| Schwurbelarchiv | ✓ eval | ground truth do klasyfikatora DE, po sprawdzeniu licencji |
| QMap / QProofs | ✓ inspiracja | model grafu „dowód → narracja”, nie dane do ingestii |
| Hierarchia A–D | ✓ rozszerzona | osobna oś `authority` + `independence` + `tier` |
| seed-core 0.1.6 | ○ action gate | audit chain + veto akcji; **nie** klasyfikator zapytań, brak API reguł |
| jfp-edge-radar 0.5.0 | ○ tryb serwerowy | ingress + HUD; **nie** egress SSRF (to `bse-fetch`) |

**Najważniejsza korekta:** Elasticsearch + Neo4j + Python + Tauri to cztery runtime'y i trzy
języki. To nie jest „prosty w obsłudze” produkt — to infrastruktura do utrzymania. Wersja Rust-native
(SQLite + Tantivy + petgraph + Leptos) daje 90% funkcji przy jednym instalatorze.

**Druga korekta (JFP):** seed-core i edge-radar to dobre klocki, ale nie te, za które ich uznano
w propozycji. seed-core jest gate'em **akcji**, nie klasyfikatorem zapytań i nie walidatorem cytowań;
edge-radar jest **ingressem**, nie egressem. Wpięte zgodnie z przeznaczeniem (action gate + audit,
reverse proxy w daemonie) dają realną wartość; wpięte niezgodnie — dokładają runtime i iluzję
bezpieczeństwa. Do tego licencje: seed-core non-commercial, edge-radar próg $10M — stąd feature flagi.

---

## 19. Co dalej

1. **ADR-001**: Tauri + Leptos (struktura crate'ów i IPC).
2. **ADR-002**: SQLite schema + migracje + backup (bez Postgresa w v1).
3. **ADR-003**: kontrakt providera + spec custom providera (JSON Schema).
4. **ADR-004**: model grafu i zasady prywatności (`is_private_individual`).
5. ~~**ADR-005**: `bse-guard` — RulesGate/CitationGate/LlmGate + adapter seed-core (action gate,
   nie klasyfikator zapytań); audit log i izolacja licencyjna.~~ **Zrealizowane w fazie 0**
   (`docs/adr/ADR-005-bse-guard.md`); `LlmGate`/`SeedGate` pozostają na fazy 2–3.
6. ~~**Faza 0**: szkielet workspace, `bse-store` z SQLite, CLI `search`, adaptery SearXNG + Brave,
   okno Tauri z jednym ekranem „Search (dev)”.~~ **Zrealizowane dla rdzenia/CLI**; shell Tauri
   jest gotowy, ale build wymaga WebKitGTK — patrz §19a.
7. ~~**Faza 1**: `bse-fetch` z SSRF/robots/cache, `bse-index` z Tantivy, near-duplicate
   MinHash/LSH, metryka różnorodności i komendy `fetch`/`index`.~~ **Zrealizowane dla rdzenia/CLI**;
   render API i benchmark p95 na żywych providerach pozostają do fazy 2 — patrz §19a.
8. ~~**Faza 2**: `bse-llm` + `bse-classify`, ekstrakcja twierdzeń/stance/encji, evidence ledger,
   `CitationGate` dla każdego twierdzenia, CLI `evidence`, ADR-007.~~ **Zrealizowane dla
   rdzenia/CLI**; UI, graf narracji i embeddingi pozostają — patrz §19a.

Po fazach 0–2 wracamy do tego dokumentu z pierwszymi liczbami (latencja, koszt, jakość) i korygujemy
roadmapę — plan bez pomiarów to hipoteza.

### 19a. Stan faz 0–2 (2026-10-25)

- **Workspace:** `bse-core`, `bse-config`, `bse-guard`, `bse-store`, `bse-providers`,
  `bse-fetch`, `bse-index`, `bse-llm`, `bse-classify`, `bse-pipeline`, `bse-cli`;
  `seed-core` i `jfp-edge-radar` poza workspace. MSRV 1.88 (Tantivy 0.27).
- **CLI `bald`:** `search`, `guard check`, `citation check`, `audit verify`, `providers list`,
  `fetch`, `index add/search/stats`, `evidence add/list/show/stats`, `config init/path`;
  JSON, `--demo`, `--no-store`, `--lang`, `--no-cache`, `--offline`.
- **Guard:** deterministyczne reguły PL/EN + leetspeak; 40 person-targeted → 0 `Allow`,
  40 benign → 0 `Refuse`; `Review`/`Refuse` kończą CLI kodem 2. Guard działa też na
  `index search`.
- **Audit:** SHA-256 JSONL hash chain, tamper detection, tylko hashe/kody; brak surowych PII.
  `fetch`, `index` i `evidence` dopisują decyzje/refusal z hashem URL/cytatów.
- **Store:** SQLite (rusqlite), runy, dokumenty, dedupe po kanonicznym URL; zapisywane tylko `Allow`.
- **Providerzy:** SearXNG JSON, Brave, Demo offline; fuzja RRF k=60.
- **Fetch:** tylko HTTP/HTTPS na 80/443, bez credentiali i proxy; blokada loopback/private/
  link-local/metadata; własny resolver DNS i rewalidacja każdego redirectu; 2 MiB, 20 s,
  5 redirectów; allowlista HTML/XHTML/plain/Markdown; `robots.txt` per origin (fail-closed
  przy 5xx), `noindex` pomijany przy indeksowaniu; cache wyekstrahowanych dokumentów
  (SHA-256 URL, TTL 1 h, atomowy zapis, `0700/0600`).
- **Ekstrakcja:** `article`/`main`/`body`, meta/OG, canonical (tylko ta sama domena),
  `lang`, `published`, charset z nagłówka lub `<meta>`, normalizacja whitespace, limit 120k znaków.
- **Indeks:** Tantivy 0.27, BM25, schema id/URL/title/body/excerpt/domain/published/lang/
  fetched/hashe; upsert po SHA-256 kanonicznego URL; pełna treść indeksowana, przechowywany
  tylko excerpt; katalog `0700`, pliki `0600`.
- **Dedupe/„suwerenność”:** RRF → near-duplicate MinHash/LSH (64 hashe, 16 pasm, próg 0,90;
  krótkie snippety < 8 tokenów bez scalania) → metryka znormalizowanej entropii domen;
  ostrzeżenia `single-source`/`low-diversity`.
- **LLM:** `bse-llm` obsługuje Ollama `/api/chat` i OpenAI-compatible `/chat/completions`;
  zdalny endpoint tylko HTTPS, `http` tylko loopback; bez credentiali w URL, bez proxy
  i redirectów; limity 4 MiB/60 s; błędy redagowane i przycinane; klucz tylko z env.
- **Klasyfikacja:** `claim-extract-v1`, strict JSON, jedna próba naprawy, fallback
  `heuristic-v1`; stance `reports/endorses/debunks/mentions`; kind
  `fact/causal/concealment/conspiracy/prophecy/other`; encje tylko nie-osobowe
  (`person`/`public_actor`/`private_individual` odrzucane); PII odrzuca twierdzenie.
- **Evidence ledger:** `claims`/`evidence`/`entities`/`claim_entities`; deterministyczne
  `claim_id` i `entity_id`; unikalność `(claim_id, document_id, quote_hash)`; zapis tylko
  po `CitationGate` (exact/normalized, Jaccard ≥ 0,92) i przy cytacie ≥ 24 znaki;
  `quote_too_short` i odrzucone cytaty pozostają wyłącznie w raporcie; `noindex` pomijany
  przed LLM; audyt bez surowych treści.
- **Desktop:** odłączony crate Tauri 2 ze statycznym mockiem UI; build zablokowany do instalacji
  WebKitGTK (`apps/desktop/README.md`).
- **Odstępstwa/ograniczenia:** `rusqlite` zamiast `sqlx` (mniejszy dependency surface,
  `RunStore` trait na przyszłość); brak JS-renderingu, crawlera i harmonogramu; brak pomiaru
  p95 na żywych providerach; UI nie jest podłączone do backendu; `CitationGate` nie weryfikuje
  semantycznego poparcia twierdzenia; brak embeddingów.
- **ADR:** `ADR-005-bse-guard.md` (bramki/audit), `ADR-006-fetch-index.md` (SSRF/robots/cache/
  Tantivy/dedupe), `ADR-007-llm-classify.md` (transport LLM/claims/ledger).

---

## 20. Changelog

| Wersja | Zmiany |
|---|---|
| 0.1 | Pierwszy plan: metasearch, claims, evidence ledger, Postgres, web UI |
| 0.2 | Tauri + Leptos, SQLite zamiast Postgresa, Tantivy, graf narracji, stance, klasyfikacja, custom provider, YaCy/Oxylabs, ewaluacja na datasetach, decyzje D1–D12 |
| 0.3 | Transport Tor/onion (§3.5, D13), integracja seed-core i edge-radar z korektą zakresu (§3.6, D14–D16), `bse-guard`/`bse-transport`/`bse-gateway`, SSRF w `bse-fetch`, izolacja licencyjna JFP, roadmapa 8–9 tyg. |
| 0.4 | Faza 0: workspace Rust, ADR-005, guard + citation gate + audit chain, SQLite, providerzy SearXNG/Brave/Demo, CLI `bald`, odłączony shell Tauri; `rusqlite` zamiast `sqlx`; stan w §19a. |
| 0.5 | Faza 1: `bse-fetch` (SSRF+robots+cache+ekstrakcja), `bse-index` (Tantivy/BM25), near-duplicate MinHash/LSH, entropia domen, CLI `fetch`/`index add/search/stats`, ADR-006; MSRV 1.88; stan w §19a. |
| 0.6 | Faza 2: `bse-llm` (Ollama/OpenAI-compatible, endpoint policy), `bse-classify` (claims/stance/encje/PII/fallback), evidence ledger SQLite, `CitationGate` + cytat ≥ 24 znaki, CLI `evidence add/list/show/stats`, ADR-007; stan w §19a. |
| 0.7 | Faza 3: `bse-api` (Axum loopback, session token, CSP), `bald serve`, frontend React+Tailwind `apps/ui` (claims grid, evidence drawer, settings), Tauri IPC bridge, atomowy config 0600, ADR-008; stan w §19a. |
| 0.8 | Faza 4 (W toku): Wbudowany Mini-LLM (Qwen 2.5 1.5B Tool-Calling Out-of-the-Box) + Model Decyzyjno-Scoringowy ONNX (Wskaźnik wiarygodności 0–100%, ochrona bazy, re-ranking dowodowy) + Uproszczony UX z profilem Zaawansowane. |
