# ADR-006 — `bse-fetch` i `bse-index`: SSRF, robots.txt, cache, BM25

- **Status:** accepted
- **Data:** 2026-10-25
- **Decydenci:** Jaro (właściciel produktu), implementacja: BALD
- **Zastępuje:** brak
- **Powiązane:** PLAN.md §6.3, §6.4, §8.1–8.2, §11.3, §11.4, §14, D7, D15

## 1. Kontekst

Faza 1 dodaje dwie rzeczy, których nie ma w metasearchu: **kontrolowane pobieranie treści**
i **lokalny indeks pełnotekstowy**. Obie są ryzykowne, jeśli zrobimy je „przy okazji”:

- fetch musi blokować SSRF, w tym redirecty i DNS rebinding,
- fetch musi respektować `robots.txt` i nie udawać przeglądarki użytkownika,
- cache nie może stać się cichym archiwum surowego HTML z dowolną treścią,
- indeks nie może być wektorem person-targetingu,
- duplikaty z trzech providerów nie mogą zajmować trzech miejsc w wynikach.

Zgodnie z §3.6 i D15 SSRF należy do `bse-fetch`, a **nie** do `jfp-edge-radar`. Edge-radar jest
ingressem L7 w trybie daemon i nie jest egress-shieldem.

## 2. Decyzja

### 2.1 Granica `bse-fetch`

`bse-fetch` jest jedynym miejscem, które wykonuje żądania HTTP do źródeł. Providerzy wyszukiwania
zwracają tylko metadane; pipeline nie pobiera treści „obok” tej warstwy.

Kontrakt wyniku to `FetchedDocument` — już wyekstrahowany dokument, nie surowa odpowiedź:

```rust
pub struct FetchedDocument {
    pub requested_url: String,
    pub final_url: String,
    pub canonical_url: String,
    pub status: u16,
    pub content_type: String,
    pub title: String,
    pub description: String,
    pub text: String,
    pub lang: Option<String>,
    pub published: Option<String>,
    pub content_hash: String,   // SHA-256(text)
    pub html_hash: String,      // SHA-256(decoded HTML/plain)
    pub fetched_at: String,
    pub noindex: bool,
    pub cache_hit: bool,
}
```

Surowy HTML nie jest zapisywany przez `bse-fetch`. Snapshoty/WARC będą osobną warstwą fazy 4,
z własną retencją i polityką prawną.

### 2.2 Polityka SSRF — dwa poziomy

1. **`validate_syntax`** — bez DNS: tylko `http`/`https`, porty 80/443, bez credentiali w URL,
   bez lokalnych nazw (`localhost`, `.local`, `.internal`, `.lan`, `.test`, `.invalid`, `.example`,
   `.onion`), literały IP sprawdzane od razu.
2. **`validate_url` + `SsrfResolver`** — każdy hostname jest rozwiązywany, a **wszystkie** zwrócone
   adresy muszą być publiczne. Ten sam resolver jest wpięty w `reqwest`, więc kontrola działa
   również przy ponownym połączeniu, a nie tylko w pierwszym „sprawdzam i zapominam”.

Blokowane zakresy IPv4 obejmują m.in. `0/8`, `10/8`, `100.64/10`, `127/8`, `169.254/16`,
`172.16/12`, `192.168/16`, `198.18/15`, multicast i reserved. IPv6 obejmuje m.in. `::/96`,
`64:ff9b::/96`, `2001:db8::/32`, `fc00::/7`, `fe80::/10`, `ff00::/8`. Adresy IPv4-mapped
są mapowane na IPv4 przed klasyfikacją.

Redirecty są obsługiwane **ręcznie**: każdy kolejny URL przechodzi pełną walidację i kontrolę
`robots.txt`. Limit: 5 redirectów. Brak `Location` = błąd, a nie ciche podążenie.

Limity: **2 MiB** na odpowiedź (również po dekompresji), **20 s** timeoutu żądania, allowlista
`text/html`, `application/xhtml+xml`, `text/plain`, `text/markdown`. `Content-Length` jest
wczesnym odrzuceniem, ale licznik faktycznie odebranych bajtów jest nadrzędny.

Proxy środowiskowe jest wyłączone (`no_proxy`), certyfikaty TLS są weryfikowane przez rustls.
Domyślny UA identyfikuje bota: `BALD/0.1 (research; +https://github.com/jarohull-ai/bald)`.

### 2.3 robots.txt

- Sprawdzany **per origin** (scheme + host + port), domyślnie włączony (`respect_robots = true`).
- Parsowany parserem `robotstxt` (port Google) dla agenta `BALD`; grupa `User-agent: BALD`
  wygrywa z `*`.
- `4xx` oznacza „brak reguł” → dostęp dozwolony (RFC 9309). `5xx`, błąd sieci albo zbyt duży
  `robots.txt` → **fail-closed** (`RobotsUnavailable`).
- Treść `robots.txt` jest cache'owana w pamięci procesu. Nie ma globalnego cache na dysku, żeby
  zmiana reguł nie była maskowana przez stary plik.
- `meta robots=noindex` jest odnotowywany w `FetchedDocument.noindex`; `index add` pomija taki
  dokument. To nie jest to samo co `robots.txt`: jedno mówi o indeksowaniu, drugie o pobieraniu.

### 2.4 Cache

- Klucz: **SHA-256 z URL** (po parserze), plik `cache/<hex>.json`.
- Wartość: `CacheEntry { stored_at, document: FetchedDocument }` — czyli już wyekstrahowany
  tekst, nie HTML.
- TTL z `fetch.cache_ttl_secs` (domyślnie 3600 s); `0` oznacza natychmiastowe wygaśnięcie.
- Zapis atomowy: plik tymczasowy → `rename`; uprawnienia `0600`, katalog `0700` (Unix).
- Cache jest best effort: błąd zapisu nie psuje udanego fetchu. Błąd odczytu = cache miss.
- Cache działa też bez sieci: pełny DNS jest potrzebny dopiero przed żądaniem, więc trafienie
  w cache nie wymaga resolvera. To świadomy kompromis: lokalna kopia nie jest nowym „crawlem”
  i nie omija `robots.txt` w momencie pobierania.
- Cache **nie jest** rejestrem dowodowym. Dowodem jest dokument + hash + źródło w SQLite,
  a nie plik cache.

### 2.5 Ekstrakcja

- HTML: `scraper` 0.29, bez JavaScriptu i bez zewnętrznych modeli.
- Priorytet kontenera: `article` → `main` → `[role=main]` → `body`; z kontenera bierzemy
  `p`, `li`, `blockquote`, nagłówki, `pre`, `td`, a nie całe drzewo (żeby nie wciągnąć menu).
- Metadane: `<title>`, `og:title`, `meta description`, `og:description`, `link rel=canonical`
  (tylko ta sama domena i dozwolony port), `lang`, `article:published_time`/`date`/`pubdate`/
  `time[datetime]`, `meta robots`.
- Tekst jest normalizowany (whitespace, puste linie) i przycinany do **120 000 znaków**.
- Charset: nagłówek `Content-Type`, a jeśli go brak — `<meta charset>`/`http-equiv` z pierwszych
  4 KiB. To obsługuje stare strony PL (ISO-8859-2, windows-1250).
- Brak treści lub strona JS-only → `index add` kończy się błędem, a nie „cichym” pustym wpisem.

### 2.6 Indeks Tantivy

`bse-index` jest embedded i lokalny. Dziś implementuje go `DocumentIndex`, ale publiczny trait
`SearchIndex` zostawia granicę dla przyszłego backendu (np. zdalnego klastra w trybie serwerowym).

Schema (Tantivy 0.27):

| Pole | Typ | Przechowywane | Rola |
|---|---|---|---|
| `id` | STRING | tak | SHA-256 kanonicznego URL, upsert |
| `canonical_url` | STRING | tak | prezentacja/dedupe |
| `title` | TEXT | tak | BM25 + snippet |
| `body` | TEXT | **nie** | BM25 |
| `excerpt` | stored-only | tak | snippet bez kopiowania całej treści |
| `domain` | STRING | tak | diversity/UI |
| `published` | STRING | tak | UI |
| `lang` | STRING | tak | filtr w przyszłości |
| `fetched_at` | STRING | tak | świeżość |
| `content_hash` | STRING | tak | dedupe/audyt |
| `html_hash` | STRING | tak | wykrywanie zmian |

- Upsert = `delete_term(id)` + `add_document` + commit; dzięki deterministycznemu `id` ten sam
  kanoniczny URL nie tworzy duplikatu.
- Pełna treść jest indeksowana, ale **nie jest przechowywana** w indeksie — tylko excerpt.
- Zapytanie składa się z pól `title` i `body`; parser Tantivy obsługuje składnię zapytań.
- Pusty `query` jest błędem, nie „zwróć wszystko”.
- Katalog indeksu `0700`, pliki po commicie `0600` (Unix). Indeks nie jest serwerem i nie
  nasłuchuje na porcie.

### 2.7 Near-duplicate i diversity

Po RRF i kanonizacji URL:

1. Sygnatura MinHash z 4-tokenowych shingli: 64 hashe, 16 pasm × 4 wartości.
2. Jeśli snippet ma ≥ 8 tokenów, sygnatura liczy się **ze snippetu**, nie z tytułu — mirrory
   często mają inny tytuł, ale ten sam opis.
3. Próg podobieństwa Jaccarda: **0,90**. Krótkie snippety (< 8 tokenów) nie są scalane, żeby nie
   łączyć przypadkowych wyników.
4. Duplikat jest scalany z istniejącym wpisem: union providerów, `best_rank = min`, dłuższy snippet.
5. Po dedupe liczymy `unique_domains` i znormalizowaną entropię Shannona `source_diversity`.
   Ostrzeżenia: `single-source` (wyniki tylko z jednej domeny) i `low-diversity` (< 0,60 przy
   ≥ 3 wynikach).

To nie jest „kara rankingowa” ani truth score. To sygnał dla użytkownika, że wynik może być
jednym źródłem powielonym przez wiele serwisów.

### 2.8 CLI i audyt

- `bald fetch <url>` — pobiera i pokazuje dokument; `--json`, `--text`, `--no-cache`.
- `bald index add <url>` — fetch + zapis do Tantivy; `noindex` jest pomijany.
- `bald index search <query>` — guard działa **przed** otwarciem indeksu; `Refuse/Review` = exit 2.
  Do audit logu trafia `query.guard` (hash zapytania) i `index.search` (liczba trafień, latencja).
- `bald index stats` — liczba dokumentów i ścieżka.
- `fetch` i `index add` zapisują do audit logu hash URL, status, `content_hash`, `cache_hit`
  i `noindex`; surowy URL nie jest zapisywany w audit logu.

## 3. Konsekwencje

**Pozytywne**

- SSRF jest testowalny bez sieci (syntax + klasyfikator IP) i egzekwowany także w resolverze.
- `robots.txt` jest respektowany domyślnie, a nie opcjonalnym „trybem grzeczności”.
- Cache nie przechowuje surowego HTML, więc nie staje się przypadkowym archiwum.
- Indeks jest szybki, lokalny i nie wymaga JVM/Elasticsearch.
- Dedupe near-duplicate działa na mirrorach, których kanoniczny URL nie złapie.
- Wszystkie nowe ścieżki mają testy jednostkowe; guard obejmuje też wyszukiwanie lokalne.

**Koszty i ograniczenia**

- Brak JavaScriptu: strony SPA i paywalle renderowane po stronie klienta dają pusty tekst.
  Render API/Playwright to osobna, opcjonalna warstwa fazy 2.
- Brak kolejki i harmonogramu: `index add` pobiera jeden URL na raz.
- `robots.txt` 5xx blokuje fetch (fail-closed). Dla niedostępnych serwerów jest to celowe,
  ale może wymagać ręcznego `respect_robots = false` w przyszłym trybie diagnostycznym.
- Cache nie jest współdzielony między procesami; atomowy `rename` chroni plik, ale nie ma locka
  na cały katalog.
- `SearchIndex` nie ma jeszcze HNSW/embeddingów; to faza 2+.
- Nie mierzymy p95 na żywych providerach; logujemy `latency_ms`, ale powtarzalny benchmark
  wymaga stabilnego SearXNG/Brave i osobnego scenariusza.

## 4. Alternatywy odrzucone

| Alternatywa | Dlaczego nie |
|---|---|
| `readability`/`dom_smoothie` od razu | większa zależność i mniej przewidywalne wyniki na starym HTML; heurystyka `scraper` wystarcza w fazie 1 |
| Cache surowego HTML | większa powierzchnia danych wrażliwych i ryzyko przypadkowego wykonania skryptów; snapshoty będą osobną warstwą |
| SQLite FTS5 zamiast Tantivy | brak BM25 i dojrzałych facetów; Tantivy jest embedded i zgodny z PLAN.md |
| SimHash zamiast MinHash/LSH | gorsza kontrola progu Jaccarda dla krótkich snippetów; MinHash/LSH jest znany i testowalny |
| robots.txt tylko jako warning | plan mówi „nie scrapuje wbrew robots.txt”; warning to nie egzekucja |
| SSRF w edge-radar | edge-radar to ingress, nie egress; mylenie warstw daje iluzję bezpieczeństwa |
| Globalny cache robots.txt | zmiana reguł byłaby maskowana; cache per proces jest wystarczający dla CLI |

## 5. Weryfikacja

Testy jednostkowe (stan na 2026-10-25):

- `bse-fetch`: 18 testów — SSRF (loopback, private, metadata, IPv4-mapped, redirecty, localhost),
  charset z nagłówka i `<meta>`, content-type, robots (allow/disallow, agent-specific),
  cache roundtrip/TTL i uprawnienia `0600/0700`, ekstrakcja HTML/plain/noindex.
- `bse-index`: 5 testów — add/search/upsert, tytuł+body, pusty query, stabilne id, uprawnienia.
- `bse-pipeline`: 12 testów — RRF, kanoniczny URL, near-duplicate, diversity, guard, end-to-end.
- `bse-config`: 2 testy — sample TOML i ścieżki.
- `bse-guard`: 19 testów — 40/40 person-targeted, 40/40 benign, citation, audit.

Smoke (ręczny, poza testami):

- `fetch https://example.com` → 200, cache miss/hit, robots.txt poprawnie pobrany.
- `fetch http://169.254.169.254/...` → exit 2, `blocked IP`.
- `fetch https://www.google.com/search?...` → exit 2, `robots.txt disallows`.
- `index add https://example.com` → 1 dokument; `index search` zwraca snippet; person query → exit 2.
- `audit verify` → OK po fetch/index.
