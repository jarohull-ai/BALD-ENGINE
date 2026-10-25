# ADR-007 — `bse-llm` i `bse-classify`: ekstrakcja twierdzeń i evidence ledger

- **Status:** accepted
- **Data:** 2026-10-25
- **Decydenci:** Jaro (właściciel produktu), implementacja: BALD
- **Zastępuje:** brak
- **Powiązane:** PLAN.md §5, §6.5, §11.1, §11.4, §13, D9, D11, D14, D16

## 1. Kontekst

Faza 1 dawała dokumenty i indeks, ale nie odpowiadała na pytanie, **co dane źródło
twierdzi**. Faza 2 dodaje ekstrakcję twierdzeń, encji i stance oraz lokalny **evidence
ledger**. To najbardziej ryzykowna warstwa produktu, bo model językowy:

- potrafi wymyślić twierdzenie, którego w źródle nie ma,
- może pominąć cytat albo „doczytać” sens, którego źródło nie wyraża,
- jest podatny na prompt injection w treści pobranej z sieci,
- może zamienić temat w profil osoby, jeśli encje nie są filtrowane,
- może zapisać PII, której nie chcemy w lokalnej bazie.

Dodatkowo transport do modelu jest powierzchnią ataku: klucze API, dowolne endpointy,
proxy środowiskowe i redirecty. Dlatego warstwa LLM nie może być „jednym plikiem w CLI”.

Zgodnie z D14 `seed-core` **nie** jest klasyfikatorem zapytań ani walidatorem cytatów.
Nie jest też używany do ekstrakcji; pozostaje action gate'em w fazie 3.

## 2. Decyzja

### 2.1 Dwa crate'y, jedna granica

- **`bse-llm`** — wyłącznie transport: walidacja endpointu, budowa requestu, limity,
  parsowanie odpowiedzi, redakcja błędów. Nie zna pojęcia claim, stance ani encji.
- **`bse-classify`** — kontrakt `Classifier`, prompty, parsowanie JSON, filtry PII i osób,
  klasyfikator heurystyczny oraz `FallbackClassifier`.
- **`bse-pipeline`** — spina fetch → classify → `CitationGate` → `Store` i zapisuje audyt.
- **`bse-core::evidence`** — współdzielone typy `EvidenceBundle`, `NewClaim`, `NewEvidence`,
  `NewEntity`, `ClaimKind`, `EntityKind`.

Klasyfikator **niczego nie zapisuje**. Zapis jest możliwy dopiero po bramce cytatu, w
warstwie pipeline'u. Dzięki temu test klasyfikatora nie potrzebuje bazy, a błąd modelu nie
omija walidacji.

Kontrakt wyniku:

```rust
pub struct ClassifiedClaim {
    pub text: String,      // zwięzłe twierdzenie
    pub quote: String,     // dosłowny fragment źródła
    pub stance: Stance,    // reports | endorses | debunks | mentions
    pub kind: ClaimKind,   // fact | causal | concealment | conspiracy | prophecy | other
    pub confidence: f32,
    pub entities: Vec<ClassifiedEntity>,
}
```

### 2.2 Transport `bse-llm`

Obsługiwane są dwa protokoły:

1. **Ollama** — `POST /api/chat`, domyślnie `http://127.0.0.1:11434`, model `llama3.1:8b`.
2. **OpenAI-compatible** — `POST /chat/completions`, np. OpenAI, OpenRouter, Groq,
   vLLM, llama.cpp server, LM Studio.

Zasady bezpieczeństwa transportu:

- tylko `http`/`https`; **`http` jest dozwolone wyłącznie dla loopbacku**
  (`127.0.0.1`, `::1`, `localhost`), zdalny endpoint musi być `https`,
- credentiale w URL są odrzucane,
- proxy środowiskowe jest wyłączone (`no_proxy`),
- redirecty są wyłączone (`Policy::none()`) — redirect mógłby wynieść prompt lub klucz
  pod inny host,
- osobne limity connect/request timeoutu; domyślnie 5 s / 60 s,
- odpowiedź jest ograniczona do **4 MiB**,
- treść błędu API jest redagowana (tokeny, `Bearer`, parametry `key=`/`token=`/`secret=`)
  i przycinana do **400 znaków**,
- klucz API jest czytany **tylko ze zmiennej środowiskowej** wskazanej w konfiguracji;
  nie jest zapisywany w TOML ani w audit logu.

### 2.3 Prompt i structured output

Prompt jest wersjonowany (`claim-extract-v1`) i wymusza JSON:

```json
{"claims":[{"claim":"...","quote":"...","stance":"reports|endorses|debunks|mentions",
"kind":"fact|causal|concealment|conspiracy|prophecy|other","confidence":0.0,
"entities":[{"name":"...","kind":"organization|location|event|document|technology|law|work|other"}]}]}
```

- Treść źródła jest zawsze w bloku `<untrusted_source>`, nigdy w instrukcjach.
- Model nie dostaje narzędzi ani dostępu do sieci.
- Odpowiedź może być w code fence; parser zdejmuje fence.
- Przy błędzie JSON wykonywana jest **jedna próba naprawy** (re-prompt), a po niej
  fallback. Liczba prób jest ograniczona `max_attempts` (domyślnie 2).
- Wykryte wzorce „ignore previous instructions” nie są wykonywane; trafiają do
  `prompt_injection_suspected` i ostrzeżeń raportu.
- Limity: 14 000 znaków wejścia, 8 twierdzeń, 800 znaków twierdzenia, 4 000 znaków
  cytatu, 120 znaków nazwy encji.

### 2.4 Stance i kind

Stance jest obowiązkowy (D11). Model musi wybrać jedną z wartości:

| Stance | Znaczenie |
|---|---|
| `reports` | relacjonuje twierdzenie bez popierania |
| `endorses` | popiera twierdzenie |
| `debunks` | obala lub wyjaśnia |
| `mentions` | tylko wzmianka |

`kind` opisuje typ twierdzenia (fakt, przyczynowość, ukrywanie, spisek, proroctwo, inne).
To nie jest „truth score”; to etykieta porządkująca ledger i przyszły ranking.

### 2.5 CitationGate i minimalna długość cytatu

Każde twierdzenie przechodzi `CitationGate`:

1. exact match po normalizacji (whitespace, interpunkcja, wielkość liter),
2. w przeciwnym razie podobieństwo Jaccarda okien tokenów ≥ **0,92**,
3. `span` jest zapisywany **tylko** dla exact match.

Dodatkowo pipeline odrzuca cytaty krótsze niż **24 znaki** (`quote_too_short`), nawet
jeśli występują w źródle. Fragment „The Great Reset” jest poprawny leksykalnie, ale nie
dokumentuje twierdzenia. Taki wpis pozostaje w raporcie jako odrzucony i **nie trafia do
ledgera**.

Ważne rozróżnienie: bramka potwierdza, że **cytat istnieje w źródle**. Nie dowodzi, że
cytat w pełni uzasadnia parafrazę twierdzenia. Dlatego UI i raport muszą pokazywać cytat
obok twierdzenia, a nie tylko samą etykietę „verified”.

### 2.6 Osoby i PII

Guard zapytań (ADR-005) nadal działa przed wyszukiwaniem. W ekstrakcji obowiązuje
dodatkowa zasada: **temat tak, osoba nie**.

Dozwolone rodzaje encji:

`organization`, `location`, `event`, `document`, `technology`, `law`, `work`, `other`.

Odpowiedzi modelu z rodzajem `person`, `public_actor` lub `private_individual` są
**odrzucane**. Nazwy osób mogą wystąpić w cytacie lub w treści twierdzenia, jeśli są
częścią opisywanego zdarzenia (np. szczyt, podpisanie ustawy), ale nie tworzą encji
osobowej ani profilu.

Filtry PII odrzucają twierdzenie lub cytat zawierający:

- adres e-mail,
- ciąg 9–15 cyfr wyglądający jak numer telefonu,
- 11-cyfrowy identyfikator (PESEL-like).

Zakresy dat, np. `2020-2021`, nie są traktowane jako telefon. Wykrycie PII powoduje
odrzucenie całego twierdzenia, nie tylko wycięcie fragmentu.

### 2.7 `noindex`

Lokalny evidence ledger jest także indeksem. Dlatego źródło z `meta robots=noindex`
jest domyślnie **pomijane przed wywołaniem LLM**. Audyt dostaje `skip` z powodem
`noindex`, bez treści dokumentu.

### 2.8 Deterministyczne ID i zapis

- `claim_id = SHA-256("claim:" + normalize_for_match(text))`
- `entity_id = SHA-256("entity:" + kind + ":" + normalize_for_match(name))`
- dokument: `documents.canonical_url` jest unikalny; ID dokumentu jest stabilne w bazie.
- unikalność dowodu: `(claim_id, document_id, quote_hash)`.

Ponowna ingestia tego samego źródła aktualizuje dokument i liczniki, nie tworzy
duplikatów. `document_count` i `evidence_count` są przeliczane z tabeli `evidence`,
a nie inkrementowane „w ciemno”.

Schemat (SQLite):

| Tabela | Rola |
|---|---|
| `documents` | źródło, URL kanoniczny, hashe, excerpt, `fetched_at` |
| `claims` | twierdzenie, normalized, kind, liczniki dokumentów/dowodów |
| `evidence` | stance, siła, cytat, `quote_hash`, span, model/prompt |
| `entities` | encje nie-osobowe |
| `claim_entities` | relacja wiele-do-wielu |

Migracje fazy 2 dodają kolumny provenance do istniejących `documents` przez
`PRAGMA table_info` + `ALTER TABLE`; stare bazy zachowują historię.

### 2.9 Heurystyka i fallback

`HeuristicClassifier` jest deterministycznym, offline'owym ekstraktorem zdań o wysokim
sygnałe. Nie udaje LLM-a:

- dzieli tekst na zdania z obsługą skrótów (`U.S.`, `3.14`) i cudzysłowu zamykającego,
- odrzuca zdania krótsze niż 24 znaki i dłuższe niż 600,
- pomija PII,
- nadaje stance/kind na podstawie leksykonu PL/EN,
- rozpoznaje tylko zamkniętą listę encji nie-osobowych.

Wynik jest jawnie oznaczony jako `heuristic-v1` (model i prompt version). Nie jest
prezentowany jako odpowiedź modelu.

`FallbackClassifier` próbuje najpierw backendu z `[llm].provider`, a po błędzie
(np. brak Ollamy, timeout, zły JSON) uruchamia `[llm].fallback_provider`. Ostrzeżenie
`primary_failed:<kind>` mówi wprost, że wynik pochodzi z fallbacku.

### 2.10 CLI, konfiguracja i audyt

Nowe komendy:

```bash
bald evidence add <url> [--offline] [--lang pl|en|auto] [--json] [--no-cache]
bald evidence list [--limit N] [--json]
bald evidence show <claim_id|prefiks> [--json]
bald evidence stats [--json]
```

Konfiguracja `[llm]` ma domyślnie Ollamę i heurystyczny fallback. Nadpisania
środowiskowe: `BALD_LLM_PROVIDER`, `BALD_LLM_BASE_URL`, `BALD_LLM_MODEL`,
`BALD_LLM_TIMEOUT_SECS`, `BALD_LLM_MAX_INPUT_CHARS`, `BALD_LLM_MAX_CLAIMS`,
`BALD_LLM_FALLBACK_*`. Klucze: `BALD_LLM_API_KEY` / `BALD_LLM_FALLBACK_API_KEY`
albo zmienne wskazane przez `*_api_key_env`.

Audyt (`evidence.fetch`, `evidence.quote`, `evidence.citation`, `evidence.ingest`)
zawiera wyłącznie:

- hashe URL/treści/cytatów/twierdzeń,
- model i prompt version,
- statusy, liczby, latency, `cache_hit`, `noindex`,
- powód odrzucenia.

Nie zawiera surowych URL-i, twierdzeń, cytatów, promptów ani kluczy API.

## 3. Konsekwencje

**Pozytywne**

- Model nie może zapisać niczego bez potwierdzonego cytatu.
- Transport jest testowalny bez sieci; endpoint policy blokuje wyciek klucza przez redirect.
- Odrzucenie osoby i PII jest deterministyczne, niezależne od jakości modelu.
- Ten sam ledger obsługuje LLM i tryb offline; `--offline` daje powtarzalne wyniki.
- ID i unikalność dowodów są deterministyczne, więc re-ingestia jest idempotentna.
- Audyt pozostaje bezpieczny nawet przy błędzie modelu.

**Koszty i ograniczenia**

- CitationGate sprawdza obecność cytatu, nie semantyczne poparcie twierdzenia.
- Krótkie cytaty są odrzucane; część poprawnych, ale lakonicznych źródeł nie wejdzie.
- Heurystyka ma zamkniętą listę encji i nie wykryje nowych organizacji.
- Heurystyka nie czyści markerów cytowań (`[4]`) ani przypisów; daje surowszy, czasem
  „dosłowny” materiał niż LLM. To fallback, nie obietnica jakości modelu.
- Nazwy osób mogą pozostać w cytacie, jeśli są częścią opisywanego zdarzenia.
- Brak kosztowego budżetu LLM na poziomie pipeline'u; limitem jest czas i `max_tokens`.
- Brak embeddingów i weryfikacji semantycznej; to warstwa późniejsza.
- Jakość LLM zależy od lokalnego modelu; `llama3.1:8b` bywa wolny i czasem zwraca
  niepoprawny JSON, co kończy się fallbackiem.
- UI nadal nie jest podłączone do ledgera; to faza 3.

## 4. Alternatywy odrzucone

| Alternatywa | Dlaczego nie |
|---|---|
| Jeden crate `bse-classify` z HTTP w środku | transport i domena mają różne testy i różne ryzyka; rozdzielenie pozwala podmienić backend bez ruszania parsera |
| Zapis twierdzeń bez cytatu | łamie definicję „udokumentowane” i robi z ledgera zbiór plotek |
| Ufanie `quote` z modelu bez bramki | model potrafi zacytować tekst, którego nie ma w źródle |
| Tylko LLM, bez heurystyki | brak sieci/Ollamy = brak produktu; heurystyka daje deterministyczny fallback |
| Tylko heurystyka | nie pokrywa parafraz i nowych tematów; LLM jest potrzebny jako primary |
| `person` jako encja | produkt zamienia się w wyszukiwarkę osób; łamie „temat tak, osoba nie” |
| Surowy prompt w audit logu | wyciek treści źródła i instrukcji; audyt ma przechowywać hashe i kody |
| Proxy środowiskowe dla wygody | klucz API i treść źródła mogą przejść przez niekontrolowany host |
| `http` dla zdalnego LLM | prompt i klucz w plaintext; loopback jest jedynym wyjątkiem |
| Automatyczne przełączanie na chmurę bez zgody | koszt i eksfiltracja treści; fallback musi być jawny w konfiguracji |

## 5. Weryfikacja

Testy jednostkowe (stan na 2026-10-25, łącznie **93** w workspace):

- `bse-classify`: 16 testów — JSON/fence, naprawa odpowiedzi, stance/kind, filtr osób,
  PII, dedupe, prompt injection, heurystyka (skróty, decyzje, PII, topic ≠ endorses).
- `bse-llm`: 5 testów — endpoint policy (https/loopback/http), credentiale w URL,
  budowa URL, redakcja błędów, limit odpowiedzi.
- `bse-store`: 7 testów — zapis/upsert, wspólny claim dla dwóch dokumentów, prefiksy ID,
  statystyki, migracje.
- `bse-pipeline`: 16 testów — akceptacja, odrzucenie cytatu spoza źródła,
  `quote_too_short`, `noindex`, audyt bez surowych danych, end-to-end.

Smoke (ręczny):

- `evidence add https://example.com --offline` → 2 exact citations, ledger zapisany,
  `list/show/stats` działają, `audit verify` OK.
- Ollama `llama3.1:8b` na `https://example.com` → poprawny JSON, 0 twierdzeń,
  audyt ważny, ~17 s.
- Heurystyka na `https://en.wikipedia.org/wiki/Great_Reset` → 8 zaakceptowanych
  cytatów, encje nie-osobowe, brak surowych danych w audycie.
- LLM na `Great Reset` → twierdzenia i cytaty, osoby odrzucone jako encje. Po dodaniu
  bramki długości cytat „The Great Reset” (14 znaków) jest odrzucany jako
  `quote_too_short`; pokrywa to test jednostkowy pipeline'u.

`cargo fmt --all`, `cargo test --workspace`, `cargo clippy --workspace --all-targets
-- -D warnings` i `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` przechodzą.
