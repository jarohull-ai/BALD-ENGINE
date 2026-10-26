<p align="center">
  <img src="assets/logo.png" alt="VIPER STUDIO / BALD" width="180" />
</p>

# BALD

Wyszukiwarka **twierdzeń, narracji i materiałów źródłowych** (ang. *Claim, Narrative & Source Evidence Engine*).
Nie szuka osób — szuka twierdzeń, narracji i dowodów źródłowych. Wynikiem nie jest zwykła lista linków,
lecz suwerenny **rejestr dowodowy (evidence ledger)**: dokument + tier źródła + autorytet + niezależność + postawa
(`reports / endorses / debunks / mentions`) + stan archiwum cyfrowego.

> „Temat tak, osoba nie.”

## Status: faza 3 (rdzeń + evidence ledger + hybrydowe UI Web & Desktop)

Działa:

- **CLI** `bald search` z offline demo, JSON-em i historią w SQLite.
- **Person guard** — deterministyczne reguły PL/EN, obfuskacja (leetspeak), fail-closed.
- **Audit trail** — hash chain SHA-256, weryfikacja `bald audit verify`, bez surowych PII.
- **Citation gate** — exact/normalized + próg Jaccard ≥ 0,92; `bald citation check`.
- **Providerzy**: DuckDuckGo HTML (bezkluczowy crawler sieci na żywo), SearXNG (JSON), Brave Web Search, Wikipedia API (PL & EN), Demo (offline fallback).
- **Fetch**: `bse-fetch` z polityką SSRF, walidacją każdego redirectu, limitami, robots.txt, cache i
  heurystyczną ekstrakcją HTML (bez JavaScriptu).
- **Indeks**: Tantivy/BM25, `bald index add/search/stats`, upsert po SHA-256 kanonicznego URL.
- **Store**: SQLite + migracje, runy, dokumenty, dedupe po kanonicznym URL.
- **Fuzja**: RRF (k=60) + kanonizacja URL + usuwanie parametrów śledzących + near-duplicate
  (MinHash/LSH) + metryka różnorodności domen.
- **Ekstrakcja twierdzeń**: `bald evidence add/list/show/stats`; Ollama
  (`/api/chat`) lub OpenAI-compatible (`/chat/completions`), deterministyczny fallback
  `heuristic-v1`, stance `reports/endorses/debunks/mentions`. Zachowuje postacie publiczne (`EntityKind::PublicActor`), eliminując wyłącznie doxxing osób prywatnych (PESEL, adresy domowe).
- **Evidence ledger**: SQLite `claims`/`evidence`/`entities`; do bazy trafia wyłącznie
  twierdzenie z cytatem potwierdzonym przez `CitationGate` (exact/normalized) i mającym
  min. 24 znaki. Odrzucone twierdzenia są tylko w raporcie.
- **Lokalne Web UI (`bald serve`)**: nowoczesny interfejs React + TypeScript + Tailwind + Lucide
  wbudowany bezpośrednio w plik binarny `bald`. Uruchamia się na `http://127.0.0.1:7333` z zabezpieczeniem tokenem
  sesyjnym przeciwko CSRF. Działa na każdym systemie bez wymogu instalacji WebKitGTK.
- **Autonomiczne Badanie Sieci (Perplexity / Firecrawl-grade Auto-Research)**: czat i rejestr twierdzeń badają otwartą sieć na żywo, pobierają cytaty, automatycznie zasilają Evidence Ledger i generują odpowiedzi z numerowanymi odnośnikami `[1]`, `[2]` oraz interaktywną półką źródeł (Sources shelf).
- **Interaktywny Rejestr Twierdzeń (Evidence Ledger UI)**: wyszukiwarka tematów z przyciskiem 1-kliknięciem `[Zbadaj w sieci]`, natychmiastowe zasilanie bazy i podpowiedzi tematów badawczych.
- **Bot Researcher (Autonomiczny monitoring tematów)**: konfigurowalne boty badawcze działające cyklicznie w tle, automatycznie przeszukujące sieć i zasilające Evidence Ledger nowo zweryfikowanymi twierdzeniami.
- **Tauri 2**: desktop shell podłączony do tego samego serwisu API przez IPC.

Zapytania `Allow` są zapisywane lokalnie w SQLite (surowy tekst + hash) na potrzeby
historii; zapytania `Refuse/Review` nie są zapisywane, tylko hashowane w audit logu.
Cache i indeks są lokalne, z uprawnieniami `0700/0600` na Unixie.

Jeszcze nie ma: grafu narracji, diffów archiwum, własnego headless browsera (Playwright/render API), embeddingów/HNSW i Tor/onion.

## Architektura Badawcza (Dla Team Panda — Co i dlaczego zmieniono)

> [!NOTE]
> Po testach przeprowadzonych przez zespół BaldTV (`@BaldTV`) wyeliminowano nadgorliwe obwarowania i przekształcono BALD w suwerenny silnik badawczy klasy Perplexity / Firecrawl.

### 1. Weryfikacja Aktorów Publicznych (`EntityKind::PublicActor`)
- **Problem:** Wcześniejsza implementacja zasady „Temat tak, osoba nie” bezwzględnie wycinała wszystkie nazwiska, w tym autorów książek, badaczy, polityków i sygnalistów (np. Klaus Schwab, Rob Brotherton, Edward Snowden).
- **Rozwiązanie:** Dodano wariant `EntityKind::PublicActor`. Prompt klasyfikatora i filtry PII chronią wyłącznie **osoby prywatne** (blokada doxxingu, adresów domowych, PESEL, prywatnych numerów telefonów). Postacie publiczne, autorzy i decydenci są w pełni rejestrowani w dowodach i odpowiedziach modelu.

### 2. Bezkluczowy Live Web Crawler (`DuckDuckGoProvider`)
- **Problem:** Gdy lokalny SearXNG lub płatne API Brave były wyłączone, wyszukiwanie sieciowe opierało się na offline fixtures lub samej Wikipedii.
- **Rozwiązanie:** Zaimplementowano natywny `DuckDuckGoProvider` (`crates/bse-providers/src/duckduckgo.rs`), który odpytuje otwarty punkt końcowy HTML, dekoduje przekierowania (`uddg=`) i wyciąga świeże linki oraz snippety bez żadnych kluczy API czy kontenerów Docker.

### 3. Autonomiczne zasilanie Evidence Ledger w Czacie (Auto-Research)
- **Problem:** Nowo uruchomiony BALD posiadał pustą bazę twierdzeń, a czat zwracał szablonową odpowiedź o braku materiału źródłowego.
- **Rozwiązanie:** Gdy użytkownik zadaje w Czacie pytanie lub podaje temat, silnik:
  1. Wykonuje badanie sieci na żywo (DuckDuckGo + Wikipedia).
  2. Pobiera i weryfikuje cytaty z czołowych źródeł (CitationGate).
  3. Automatycznie zapisuje zweryfikowane twierdzenia do lokalnego Evidence Ledgera (SQLite).
  4. Generuje syntezę z numerowanymi odsyłaczami `[1]`, `[2]`, pełnymi linkami oraz interaktywną listą źródeł w UI.

### 4. Aktywny Rejestr Twierdzeń w UI
- **Problem:** Przy pustym Rejestrze użytkownik widział jedynie instrukcję CLI, a brak pasujących filtrów blokował badanie.
- **Rozwiązanie:** Wbudowano formularz natychmiastowego badania tematu `[Zbadaj w sieci]` bezpośrednio w widoku Rejestru, wraz z gotowymi przykładami i obsługą dowolnych zapytań badawczych.

### 5. Chronologiczny Porządek Źródeł (Od Najnowszych do Najstarszych)
- **Problem:** Informacje w internecie pojawiają się w różnym czasie, a losowa kolejność wyników utrudniała śledzenie rozwoju narracji i najświeższych doniesień.
- **Rozwiązanie:** Zaimplementowano moduł heurystycznej ekstrakcji dat (`extract_date_heuristic`), który normalizuje daty publikacji z metadanych, nagłówków, adresów URL oraz snippetów. Wszystkie źródła (`live_sources`) oraz wyniki wyszukiwania są automatycznie sortowane chronologicznie od najświeższych do najstarszych, a model otrzymuje wytyczne do przedstawiania faktów i ewolucji zdarzeń w porządku czasowym.

### 6. Interaktywne Drążenie Tematu (Follow-up / Proponowane Wątki)
- **Problem:** Po otrzymaniu odpowiedzi użytkownik musiał ręcznie wymyślać kolejne zapytania, aby zgłębić lokalne tajemnice lub wątki poboczne.
- **Rozwiązanie:** Silnik po każdej odpowiedzi generuje 3 intrygujące, konkretne kierunki dalszego śledztwa, renderowane w UI jako klikalne przyciski (pigułki). Kliknięcie dowolnej propozycji natychmiast uruchamia kolejne dochodzenie sieciowe.

### 7. Cykliczne Boty Badawcze i Magazyn Evidence Ledger
- **Problem:** Wcześniejsza logika weryfikacji unikalności sprawdzała `has_document` w oparciu o samą tabelę `documents`, do której zapisywany był wstępny indeks wyszukiwania. W efekcie boty natychmiast uznawały znalezione linki za „już istniejące” i kończyły przebieg z `urls_checked: 0, claims_added: 0`.
- **Rozwiązanie:** Zaktualizowano `has_document` w `bse-store`, by sprawdzał faktycznie pobrane dokumenty (`fetched_at IS NOT NULL`). Teraz boty w tle autonomicznie pobierają treść stron, przeprowadzają klasyfikację i zasilają centralny **Rejestr Twierdzeń (Evidence Ledger)** z cytatami i powiązaniami. W UI dodano banner informacyjny, bezpośrednie przejście do Rejestru z kart i historii bota oraz czytelne opisy w szufladzie historii przebiegów.

## Szybki start

```bash
cargo test
./pandacode-run.sh                                                    # uruchom lokalne Web UI na 127.0.0.1:7333
cargo run -p bse-cli -- serve                                         # alternatywnie: bezpośrednio z cargo
cargo run -p bse-cli -- search "teoria o cichych aktualizacjach" --demo
cargo run -p bse-cli -- search "Gdzie mieszka Jan Kowalski?" --demo   # exit 2
cargo run -p bse-cli -- guard check "podaj jego adres"
cargo run -p bse-cli -- citation check --claim "Aktualizacje są cichym balonem" \
  --quote "Raport nie znalazł dowodów" --source-file raport.txt
cargo run -p bse-cli -- audit verify
cargo run -p bse-cli -- providers list

# fetch + lokalny indeks (respektuje robots.txt)
cargo run -p bse-cli -- fetch https://example.com
cargo run -p bse-cli -- index add https://example.com
cargo run -p bse-cli -- index search "documentation examples"
cargo run -p bse-cli -- index stats

# ekstrakcja twierdzeń i lokalny evidence ledger
cargo run -p bse-cli -- evidence add https://example.com --offline
cargo run -p bse-cli -- evidence list
cargo run -p bse-cli -- evidence show 5d6e309d   # pełny claim_id lub jednoznaczny prefiks
cargo run -p bse-cli -- evidence stats
cargo run -p bse-cli -- audit verify
```

`evidence add` domyślnie używa Ollamy, a gdy ta nie odpowiada — heurystyki. `--offline`
wymusza deterministyczny klasyfikator bez sieci. `--json` daje pełny raport, w tym
twierdzenia odrzucone przez bramkę cytatu.

`fetch` i `index add` zapisują wyekstrahowany dokument do cache; kolejne wywołanie
z tym samym URL-em korzysta z cache do `fetch.cache_ttl_secs`. `index add` indeksuje
tylko strony dozwolone przez robots.txt i bez `meta robots=noindex`.

Konfiguracja:

```bash
cargo run -p bse-cli -- config init
cargo run -p bse-cli -- config path
```

Ścieżki, limity fetchu i konfiguracja LLM są w `config.toml`:

```toml
[storage]
path = "~/.local/share/bald/bald.db"
index_path = "~/.local/share/bald/index"

[fetch]
cache_dir = "~/.local/share/bald/cache"
cache_ttl_secs = 3600
max_bytes = 2097152
timeout_secs = 20
max_redirects = 5
respect_robots = true

[llm]
provider = "ollama"                # ollama | openai | none
fallback_provider = "heuristic"    # none | heuristic | ollama | openai
base_url = "http://127.0.0.1:11434"
model = "llama3.1:8b"
api_key_env = "OLLAMA_API_KEY"     # klucz tylko z env, nigdy z TOML
fallback_base_url = "https://api.openai.com/v1"
fallback_model = "gpt-4o-mini"
fallback_api_key_env = "OPENAI_API_KEY"
timeout_secs = 60
max_input_chars = 14000
max_claims = 8
max_attempts = 2
```

Nadpisania: `BALD_INDEX_PATH`, `BALD_CACHE_DIR`, `BALD_FETCH_TIMEOUT_SECS`,
`BALD_RESPECT_ROBOTS`, `BALD_LLM_PROVIDER`, `BALD_LLM_BASE_URL`, `BALD_LLM_MODEL`,
`BALD_LLM_TIMEOUT_SECS`, `BALD_LLM_MAX_INPUT_CHARS`, `BALD_LLM_MAX_CLAIMS`.
Klucze LLM: `BALD_LLM_API_KEY` / `BALD_LLM_FALLBACK_API_KEY` albo zmienne wskazane
przez `api_key_env` / `fallback_api_key_env`. Dla zdalnego endpointu wymagane jest
HTTPS; `http` jest dozwolone tylko dla loopbacku.

Domyślnie nic nie jest włączone poza `--demo`. Aby użyć SearXNG:

```bash
export BALD_SEARXNG_ENABLED=true
export BALD_SEARXNG_URL=http://127.0.0.1:8080
cargo run -p bse-cli -- search "deplatforming" --lang pl
```

Brave:

```bash
export BALD_BRAVE_ENABLED=true
export BRAVE_API_KEY=...
cargo run -p bse-cli -- search "deplatforming" --lang pl
```

Klucz może też być w `~/.config/bald/config.toml` (uprawnienia 0600), ale zmienne
środowiskowe są bezpieczniejsze. Keyring wchodzi w fazie 3.

## Workspace

```
crates/
  bse-core/       typy domenowe, trait Provider, Gate, SearchReport
  bse-config/     TOML + env, ścieżki, sample config
  bse-guard/      RulesGate, CitationGate, AuditLog (hash chain)
  bse-store/      SQLite (rusqlite), migracje, runy, dokumenty
  bse-providers/  SearXNG, Brave, Demo
  bse-fetch/      HTTP+SSRF, robots.txt, cache, ekstrakcja treści
  bse-index/      Tantivy/BM25, upsert, lokalne wyszukiwanie
  bse-llm/        transport Ollama / OpenAI-compatible (bez logiki domenowej)
  bse-classify/   ekstrakcja twierdzeń, stance, encje, filtry PII, fallback
  bse-pipeline/   guard → fan-out → kanonizacja → RRF → near-dedupe → store;
                  evidence: fetch → classify → CitationGate → ledger
  bse-cli/        binarka `bald`
apps/desktop/     Tauri 2 shell (poza workspace, wymaga WebKitGTK)
docs/adr/         ADR-005 (guard/audit), ADR-006 (fetch/indeks), ADR-007 (LLM/claims)
PLAN.md           pełny plan architektury (v0.6, stan faz 0–2)
```

`seed-core/` i `jfp-edge-radar/` w katalogu projektu to **niezależne repo JFP**,
celowo poza workspace. Rdzeń BALD jest MIT i nie linkuje żadnego z nich.

## Decyzje, które warto znać

- **SQLite, nie Postgres** — jeden plik, zero serwera; trait `RunStore` zostawia drogę do
  Postgresa w trybie `server`.
- **rusqlite, nie sqlx** — świadome odstępstwo od PLAN.md na fazę 0: mniej zależności,
  prostszy build, zapytania są krótkie. Wymiana za traitem, bez zmian w pipeline.
- **Tantivy od fazy 1** — BM25 na lokalnym indeksie, obok RRF na świeżych wynikach.
  Pełna treść jest indeksowana, ale nie przechowywana; w indeksie zostaje excerpt.
- **SSRF i robots w `bse-fetch`, nie w providerach** — walidacja URL, DNS i każdego
  redirectu; tylko HTTP/HTTPS na portach 80/443; bez proxy i bez credentiali.
  `robots.txt` jest sprawdzany per origin, a 5xx/awaria robots oznacza fail-closed.
- **Cache to wyekstrahowany dokument, nie surowy HTML** — mniejsza powierzchnia
  przechowywania treści i brak przypadkowego wykonywania skryptów; TTL domyślnie 1 h.
- **Near-duplicate na snippetach** — MinHash/LSH (64 hashe, 16 pasm, próg 0,90)
  scala mirrory i kopie, ale nie krótkie snippety (< 8 tokenów).
- **LLM nie zapisuje do bazy** — `bse-classify` tylko zwraca twierdzenia; dopiero
  `EvidencePipeline` przepuszcza je przez `CitationGate` i zapisuje. Brak cytatu =
  brak dowodu, nawet jeśli model „wie lepiej”.
- **Cytat musi mieć min. 24 znaki** — krótkie fragmenty („The Great Reset”) są
  odrzucane jako `quote_too_short`, bo nie dokumentują twierdzenia.
- **Osoba nie jest encją** — dozwolone są organizacje, miejsca, wydarzenia, dokumenty,
  technologie, prawo i dzieła; `person`/`public_actor`/`private_individual` są odrzucane.
- **PII odrzuca twierdzenie** — e-mail, telefon-like 9–15 cyfr i 11-cyfrowy ID; nie
  wycinamy fragmentu, tylko nie zapisujemy wpisu.
- **`noindex` przed LLM** — lokalny ledger jest indeksem, więc źródło `noindex` jest
  pomijane zanim trafi do modelu.
- **Transport LLM osobno** — `bse-llm` waliduje endpoint (HTTPS poza loopbackiem),
  blokuje credentiale w URL, proxy i redirecty; klucz tylko z env, błędy redagowane.
- **Audyt bez treści** — `evidence.*` zapisuje hashe, model/prompt version, statusy
  i liczby; nie zapisuje URL-i, twierdzeń, cytatów ani kluczy.
- **seed-core tylko jako action gate (faza 3)** — nie klasyfikator zapytań i nie
  walidator cytowań; szczegóły i mapowanie `etm`/`lid` w `docs/adr/ADR-005-bse-guard.md`.
- **jfp-edge-radar tylko ingress w daemonie** — nie egress SSRF; SSRF należy do
  `bse-fetch` (faza 1).
- **Tor/onion w v1.1** — poza fazą 0/1; wejdzie jako opcjonalny transport SOCKS5.

## Guard: co blokuje

Przykłady `Refuse`: „gdzie mieszka…”, „podaj jego adres”, „czyj to PESEL”,
„ustal tożsamość”, „namierz go”, „jak zabić…”, „doxx”, „swatting”, warianty EN
i leetspeak („gdz13 m13szka”).

Przykłady `Allow`: „teoria o cichych aktualizacjach”, „raport o wycieku danych
osobowych”, „jak działa PESEL”, „adresy baz wojskowych”, „historia deplatformingu”.

`Review` (np. „kto to jest…”) jest w CLI traktowane jak zatrzymanie (exit 2).
UI w fazie 3 dostanie świadomą ścieżkę potwierdzenia.

## Testy

```bash
cargo test
```

Zestaw obejmuje: 40 zapytań person-targeted i 40 benign (PL/EN/obfuskacja), walidację cytatów,
tamper-detection audit logu, kanonizację URL, RRF/dedupe, near-duplicate MinHash/LSH,
politykę SSRF, robots.txt, cache/TTL i uprawnienia plików, ekstrakcję HTML, SQLite,
Tantivy upsert/search, parsery providerów, transport LLM, ekstrakcję twierdzeń i PII,
auto-discovery lokalnych modeli (Ollama, LM Studio, LocalAI), asystenta czatu z weryfikacją cytatów,
wyszukiwanie w internecie z badaniem 1-kliknięciem oraz
end-to-end evidence ledger i API. Łącznie **109 testów**.

## Ograniczenia fazy 2

- **Brak JavaScriptu** — strony renderowane po stronie klienta mogą dać pusty tekst;
  render API/Playwright jest opcjonalny w kolejnej fazie.
- **Brak własnego crawlera i kolejki** — `index add` i `evidence add` pobierają jeden
  URL na raz.
- **Brak harmonogramu świeżości** — cache TTL i ręczne `evidence add`.
- **Brak pomiaru p95 na żywym SearXNG/Brave** — mierzymy lokalny indeks i logujemy
  `latency_ms`; powtarzalny benchmark providerów wymaga stabilnego środowiska.
- **UI nadal statyczne** — nie jest podłączone do backendu Rust/Leptosa ani do ledgera.
- **Brak HNSW/embeddingów** — semantyka i klasteryzacja narracji w późniejszej fazie.
- **CitationGate nie weryfikuje sensu** — potwierdza obecność cytatu w źródle, nie to,
  że cytat w pełni uzasadnia parafrazę. Dlatego cytat jest zawsze pokazywany obok twierdzenia.
- **Nazwy osób mogą zostać w cytacie** — jeśli są częścią opisywanego zdarzenia; nie
  tworzą jednak encji osobowych ani profilu.
- **Jakość lokalnego LLM** — `llama3.1:8b` bywa wolny i czasem zwraca niepoprawny JSON;
  wtedy działa jawny fallback `heuristic-v1` (ostrzeżenie `primary_failed:*`). Heurystyka
  nie czyści markerów cytowań (`[4]`) i bywa bardziej „dosłowna” niż model.

## Dokumentacja

- [`PLAN.md`](PLAN.md) — architektura, model domenowy, roadmapa, decyzje D1–D16.
- [`docs/adr/ADR-005-bse-guard.md`](docs/adr/ADR-005-bse-guard.md) — kontrakt bramek,
  audit chain, adapter seed-core, izolacja licencyjna.
- [`docs/adr/ADR-006-fetch-index.md`](docs/adr/ADR-006-fetch-index.md) — granice fetch/indeks,
  SSRF, robots.txt, cache, ekstrakcja, BM25 i near-duplicate.
- [`docs/adr/ADR-007-llm-classify.md`](docs/adr/ADR-007-llm-classify.md) — transport LLM,
  ekstrakcja twierdzeń, stance, `CitationGate`, osoby/PII i evidence ledger.

## Licencja

Rdzeń: **MIT** (patrz `LICENSE`). Komponenty JFP (`seed-core`, `jfp-edge-radar`)
nie są linkowane domyślnie; ich włączenie wymaga akceptacji ich licencji.
