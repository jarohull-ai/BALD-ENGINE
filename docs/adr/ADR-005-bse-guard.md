# ADR-005 — `bse-guard`: bramki decyzji, walidacja cytowań i audit trail

- **Status:** accepted
- **Data:** 2026-10-25
- **Decydenci:** Jaro (właściciel produktu), implementacja: BALD
- **Zastępuje:** brak
- **Powiązane:** PLAN.md §1.3, §3.1, §6.2, §6.5, §11.1, §11.4, D14, D16

## 1. Kontekst

BALD jest wyszukiwarką **tematów i twierdzeń**, nie ludzi. Dwie twarde zasady z PLAN.md wymagają
deterministycznej egzekucji:

1. **Person guard** — zapytanie, które próbuje ustalić dane prywatne, adres, miejsce pobytu,
   miejsce pracy, PESEL, wizerunek albo zachęca do przemocy wobec osoby, ma zostać odrzucone
   **przed** jakimkolwiek wywołaniem providera.
2. **Citation gate** — każde zdanie syntezy musi wskazywać na konkretny fragment dowodu. Brak
   pokrycia = zdanie trafia do `unsupported`, nie do odpowiedzi.

Dodatkowo każda decyzja (allow/refuse/review) i każde odrzucenie cytatu muszą być audytowalne:
kto/co, kiedy, jaka reguła, jaki hash treści. Audit ma być **tamper-evident**, ale nie może
przechowywać surowych zapytań ani PII.

W ekosystemie JFP istnieje `seed-core 0.1.6` — deterministyczny gate **akcji agenta** z łańcuchem
SHA-256. Z analizy kodu (§3.6 PLAN.md) wynika, że:

- `seed-core` nie jest klasyfikatorem zapytań o osoby i nie zna pojęcia „claim”/„cytat”,
- jego reguły są wbudowane (`SeedDecider::new` + `evaluate`), bez publicznego `RuleSet`,
- benchmark ~15 µs dotyczy istniejącego zestawu reguł, nie nowych reguł PII,
- licencja darmowa jest niekomercyjna.

Dlatego `bse-guard` jest **naszym** crate'em z własnym kontraktem, a `seed-core` jest opcjonalnym
adapterem dla akcji, nie fundamentem bramki zapytań.

## 2. Decyzja

### 2.1 Kontrakt `Gate`

Bramka jest synchroniczna, bez I/O i bez sieci. Dzięki temu czas odpowiedzi jest przewidywalny
i nie zależy od providera LLM. Semantyczna druga linia (`LlmGate`) jest osobnym, asynchronicznym
krokiem pipeline'u i **nie** implementuje `Gate`.

```rust
// bse-core/src/gate.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatePurpose {
    Search,
    Fetch,
    Publish,
    Export,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateDecision {
    Allow,
    Rewrite,
    Review,
    Refuse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateVerdict {
    pub gate: String,                // "rules" | "citation" | "seed"
    pub decision: GateDecision,
    pub score: f32,                  // 0.0..=1.0, tylko do UI/audytu
    pub reasons: Vec<String>,        // kody, nie surowe PII
    pub matched: Vec<String>,        // id reguł / kategorie
    pub rewritten: Option<String>,   // tylko dla Rewrite
}

pub trait Gate: Send + Sync {
    fn id(&self) -> &'static str;
    fn check(&self, ctx: &GateCtx<'_>) -> GateVerdict;
}

pub struct GateCtx<'a> {
    pub text: &'a str,
    pub lang: Lang,
    pub purpose: GatePurpose,
    pub citation_coverage: Option<f32>, // dla SeedGate: ETM
    pub semantic_dispersion: Option<f32>, // dla SeedGate: LID
}
```

Zasady:

- **Brak `Result` w `Gate::check`.** Bramka nie może „wywalić się” i przepuścić zapytania.
  W fazie 0 `RulesGate` nie ma ścieżki błędu: panika lub błąd inicjalizacji reguł kończy proces
  (nigdy `Allow`). `LlmGate`/`SeedGate` mapują timeout/błąd na `Review`/`Refuse` (`gate_error`).
- **Brak I/O.** Audit zapisuje pipeline, nie bramka.
- **Kolejność:** `RulesGate` → (opcjonalnie) `LlmGate` → `SeedGate` tylko dla `Purpose::Action`.
- **Jedna decyzja końcowa:** jeśli dowolna bramka zwróci `Refuse`, kończymy. `Review` wygrywa
  z `Allow`. `Rewrite` jest dozwolone tylko wtedy, gdy nie ma `Refuse`.

### 2.2 `RulesGate` (faza 0)

Deterministyczny klasyfikator wzorców PL/EN. Nie udaje semantyki — ma być szybki i konserwatywny.

Normalizacja (przed dopasowaniem):

1. Unicode lowercase, usunięcie znaków zero-width.
2. Mapowanie polskich diakrytyków: `ą→a`, `ć→c`, `ę→e`, `ł→l`, `ń→n`, `ó→o`, `ś→s`, `ź→z`, `ż→z`.
3. Leetspeak tylko w wariantach reguł (`0→o`, `1→i`, `3→e`, `4→a`, `5→s`, `7→t`), nie w tekście
   wyświetlanym.
4. Kolaps whitespace; interpunkcja zamieniona na spacje (żeby „adres,” = „adres”).
5. Zachowanie oryginału do audytu/UI.

Kategorie reguł:

| Kategoria | Przykłady (PL/EN) | Decyzja |
|---|---|---|
| `pii_request` | „podaj jego adres”, „gdzie mieszka”, „jego numer telefonu”, „czyj to pesel”, „adres domowy” | `Refuse` |
| `identity_targeting` | „ustal tożsamość”, „namierz”, „znajdź osobę”, „doxx”, „swatting” | `Refuse` |
| `person_bio` | „kto to jest…” (bez próby ustalenia danych prywatnych) | `Review` |
| `harm` | „jak zabić”, „jak skrzywdzić”, „otruć” | `Refuse` |
| `pii_term` | „pesel”, „dane osobowe”, „wizerunek”, „miejsce pracy” | `Review` tylko z markerem osoby |
| `research_context` | „teoria”, „raport”, „jak działa”, „co to jest”, „wyciek danych” | łagodzi `pii_term`/`person_bio`, nie łagodzi `pii_request` |

Zasada kompozycji: **`pii_term` sam nie blokuje** — inaczej zablokujemy uzasadnione zapytania
(„jak działa PESEL”, „raport o wycieku danych osobowych”). Blokuje dopiero para
`pii_term` + marker osoby/„podaj/znajdź/ustal” albo sam `pii_request`/`harm`.

Dodatkowa heurystyka fazy 0: **dwa tokeny pisane wielką literą + czasownik żądania + termin PII**
(np. „Jan Kowalski adres zamieszkania podaj”) daje `Refuse`. To łapie odmianę, której nie widzą
proste wzorce, ale nie blokuje samej wzmianki o osobie w kontekście badawczym. Granica jest
świadoma: bez NER/LLM nie odróżnimy „adres Kowalskiego” od „adres Muzeum Narodowego”; dlatego
`LlmGate` (faza 2) jest drugą linią, a nie opcją.

Progi:

- `Refuse` — dopasowanie reguły blokującej (`score = 1.0`).
- `Review` — dopasowanie reguły przeglądowej (`score = 0.5`), domyślnie pipeline traktuje
  `Review` jak `Refuse` w CLI; UI (faza 3) pokaże „wymaga potwierdzenia”.
- `Allow` — brak dopasowań.

`RulesGate` mierzy się p99 < 1 ms na zapytaniu do 512 znaków (bez I/O). To budżet, nie obietnica
dla przyszłych reguł semantycznych.

### 2.3 `LlmGate` (faza 2, poza tym ADR w zakresie implementacji)

- Dostaje oryginał + listę dopasowań z `RulesGate`, zwraca `allow | review | refuse` + uzasadnienie.
- Nie może **przepuścić** tego, co `RulesGate` zablokowało (tylko zaostrzać).
- Timeout 1,5 s; błąd = `Review` (nie `Allow`).
- Prompt injection: zapytanie użytkownika jest danymi, nie instrukcją; brak narzędzi w prompcie.

### 2.4 `CitationGate` (faza 0)

Wejście: `claim` (zdanie syntezy), `quote` (fragment dowodu), `source` (tekst dokumentu).
Wyjście: `verified_exact | verified_normalized | rejected | source_too_short`.

Algorytm:

1. Normalizacja dopasowania: lowercase, kolaps whitespace, interpunkcja jako separatory,
   ujednolicenie cudzysłowów. **Bez** usuwania diakrytyków (cytat ma być wierny).
2. Dokładne `contains` w znormalizowanym źródle → `VerifiedExact`, `score = 1.0`.
3. Jeśli nie — tokeny (split whitespace), sliding window o długości cytatu; Jaccard:
   - Jaccard ≥ 0,92 → `VerifiedNormalized`, `score = jaccard`.
   - inaczej `Rejected`, `score = jaccard`.
4. `quote` krótszy niż 12 znaków albo źródło krótsze niż cytat → `SourceTooShort`.
5. Hash: `quote_hash = SHA-256(normalized_quote)`, `source_hash = SHA-256(normalized_source)`.
   `span` (byte offsets w **znormalizowanym** tekście) jest zwracany tylko dla dopasowania
   dokładnego; dla fuzzy nie udajemy precyzyjnego podświetlenia.

Czego to **nie** robi: nie wykrywa parafrazy semantycznej, nie ocenia prawdziwości, nie zastępuje
LLM. Dla parafrazy wymagany jest `LlmGate`/weryfikacja człowieka; `approx` zawsze ma flagę.

### 2.5 `SeedGate` (faza 3, opcjonalny)

`SeedGate` dotyczy **wyłącznie akcji własnych BALD**, nie zapytań użytkownika:

| Akcja BALD | `ActionType` | `action_args` |
|---|---|---|
| pobranie URL | `NetworkFetch` | `url` |
| otwarcie linku w przeglądarce | `BrowserAction` | `url` |
| eksport WARC/JSON | `FileWrite` | `path` |
| instalacja modelu | `CommandExec` | `cmd` |
| krok planu | `PlanStep` | `step` |

Mapowanie metryk:

- `GeometryMetrics.etm = citation_coverage` (0..1) — pokrycie cytowań dla publikowanej syntezy.
- `GeometryMetrics.lid = semantic_dispersion` (0..1) — rozproszenie semantyczne odpowiedzi.
- `GeometryMetrics.cos_gap` = 0.0, `activated_domains`/`activated_ages` z evidence ledgeru (faza 3).

Szkic koncepcyjny adaptera (feature `seed-gate`, **nie** w domyślnym buildzie; w fazie 0 feature
jest pustym placeholderem, a nazwy pól seed-core trzeba potwierdzić przy implementacji):

```rust
#[cfg(feature = "seed-gate")]
pub fn judge_action(action: &ActionRequest, metrics: &ActionMetrics) -> GateVerdict {
    use seed_core::{ActionType, CandidateAction, GeometryMetrics, SeedDecider, VerdictStatus};

    let candidate = CandidateAction {
        step_id: action.step_id,
        intent: action.intent.clone(),
        action_type: match action.kind {
            ActionKind::Fetch => ActionType::NetworkFetch,
            ActionKind::OpenLink => ActionType::BrowserAction,
            ActionKind::Export => ActionType::FileWrite,
            ActionKind::Shell => ActionType::CommandExec,
        },
        action_args: action.args.clone(),
        planner_thought: action.thought.clone(),
    };
    let geom = GeometryMetrics {
        lid: metrics.semantic_dispersion.unwrap_or(0.0),
        cos_gap: 0.0,
        etm: metrics.citation_coverage.unwrap_or(0.0),
        activated_domains: metrics.domains.clone(),
        activated_ages: metrics.ages.clone(),
    };
    let v = SeedDecider::new("HOST_SOVEREIGN", &action.scope).evaluate(&candidate, &geom);

    let decision = match v.verdict {
        VerdictStatus::Approved => GateDecision::Allow,
        VerdictStatus::VetoDenied => GateDecision::Refuse,
        VerdictStatus::EscalateParliament => GateDecision::Review,
    };
    GateVerdict {
        gate: "seed".to_string(),
        decision,
        score: v.confidence,
        reasons: vec![v.reason],
        matched: vec![format!("seed:{}", v.action as u8)],
        rewritten: None,
    }
}
```

Wymagania:

- `seed-core = { version = "0.1.6", optional = true }` tylko pod feature `seed-gate`.
- Domyślny build BALD (MIT) **nie linkuje** seed-core. Włączenie feature oznacza akceptację
  licencji seed-core (darmowa niekomercyjna; komercyjna wymaga umowy).
- `seed-core` nie jest zależnością `bse-core`, tylko adaptera w `bse-guard` (izolacja licencyjna).
- Jeśli `seed-core` zwróci błąd/panikę — `Refuse` (`gate_error`), nie `Allow`.
- Gdy `seed-core` zmieni API, wymieniamy adapter, nie pipeline.

### 2.6 Audit trail

Format: JSONL, jeden rekord na linię, plik `audit.jsonl` (domyślnie katalog danych
`directories` — na Linux `~/.local/share/bald/audit.jsonl`; uprawnienia `0600`).

```json
{
  "v": 1,
  "seq": 42,
  "ts": "2026-10-25T15:30:00.123Z",
  "kind": "query.guard",
  "subject": "sha256:9f2c...",
  "gate": "rules",
  "decision": "allow",
  "detail": { "matched": [], "lang": "pl" },
  "prev": "sha256:0000...",
  "hash": "sha256:7a1b..."
}
```

- `subject` = SHA-256 znormalizowanego zapytania/cytatu — **bez surowego tekstu**.
- `detail` zawiera tylko kody reguł, liczby, hashe i zakresy; **nigdy** surowe PII.
- `hash = SHA-256("bald-audit-v1|v|seq|ts|kind|subject|gate|decision|detail_json|prev")`.
- `prev` = `hash` poprzedniego rekordu; genesis = 64 zera.
- `verify()` czyta cały plik, sprawdza sekwencję, `prev`, hash i zwraca pierwszy uszkodzony rekord.
- Zapis: append + `fsync` (best effort); docelowo `fd-lock` dla wielu procesów (faza 3).
- Retencja: domyślnie bez limitu (to ledger decyzji), ale UI pozwala wyeksportować i usunąć.
- **Fail policy:** w fazie 0 CLI otwiera log **przed** wyszukiwaniem; błąd otwarcia lub
  uszkodzony ostatni rekord = przerwanie (fail-closed). Błąd pojedynczego `append` po wykonanym
  zapytaniu = ostrzeżenie, bo fan-outu nie da się cofnąć. Dla `Action`/`Publish`/`Export`
  każdy błąd audytu = `Refuse` (fail-closed); ta część wchodzi z `SeedGate`.

### 2.7 Wydajność

| Operacja | Budżet p99 |
|---|---|
| `RulesGate::check` (512 znaków) | < 1 ms |
| `CitationGate::verify` (cytat 500 znaków, źródło 20 kB) | < 5 ms |
| `AuditLog::append` (fsync) | < 10 ms |
| `AuditLog::verify` (10 tys. rekordów) | < 500 ms |

### 2.8 Testy akceptacyjne

- **Person guard:** 40 zapytań person-targeted (PL/EN, obfuskacja, leetspeak) → zero `Allow`;
  40 benign (w tym „PESEL”, „adresy baz wojskowych”, „wyciek danych osobowych”) → zero `Refuse`.
  **Stan fazy 0: zrealizowane (testy `bse-guard`).**
- **Citation:** exact, interpunkcja, whitespace, parafraza (musi `Reject`), za krótki cytat.
- **Audit:** poprawny łańcuch; edycja dowolnego pola → `verify` wskazuje rekord; usunięcie
  środkowego rekordu → wykryte; pusty plik = valid.
- **Fail-closed:** bramka z błędem wewnętrznym nie może zwrócić `Allow`. `RulesGate` nie ma
  ścieżki `Result`; panika przerywa proces. Test na stubie `GateChain` dochodzi z fazą 2.
- **Brak PII w audycie:** rekord nie zawiera surowego zapytania (test substring).

### 2.9 Stan implementacji fazy 0

- `GateVerdict.gate` jest `String` (nie `&'static str`) — łatwiejsza serializacja i adaptery.
- Pipeline używa jednej `RulesGate`; `LlmGate`, `SeedGate`, `GateChain` i wspólna polityka
  „najgorsza decyzja wygrywa” są poza fazą 0. Feature `seed-gate` istnieje jako placeholder
  bez zależności od seed-core.
- `CitationGate` implementuje exact + Jaccard ≥ 0,92; `span` tylko dla exact. CLI:
  `bald citation check --claim … --quote … --source-file …`.
- Audit dla `Search`: otwarcie/ciągłość logu jest warunkiem startu (fail-closed), pojedynczy
  błąd `append` = ostrzeżenie (§2.6). Fail-closed dla akcji/publikacji wejdzie z `SeedGate` (faza 3).
- CLI zapisuje do SQLite tylko zapytania `Allow`; `Review`/`Refuse` kończą się kodem 2 i nie
  trafiają do store. Audit zawiera wyłącznie hashe i kody, nigdy surowy tekst zapytania.

## 3. Konsekwencje

Pozytywne:

- Deterministyczna, szybka i audytowalna pierwsza linia obrony.
- Cytowania weryfikowalne bez LLM; „unsupported” jest mierzalne.
- Czysta licencja MIT w rdzeniu; JFP tylko opt-in.
- Jeden format audytu dla zapytań, cytatów i akcji.

Koszty:

- Reguły wymagają utrzymania i testów fałszywych trafień (szczególnie PL/EN).
- `Review` w CLI jest uciążliwe; UI musi mieć świadomą ścieżkę potwierdzenia (faza 3).
- Fuzzy citation ma limit; parafrazy nadal wymagają LLM/człowieka.
- SeedGate wymaga upstreamowego `RuleSet` albo forka, jeśli chcemy własne reguły akcji.

## 4. Poza zakresem

- Klasyfikacja teorii spiskowych (to `bse-classify`, §5.5 PLAN.md).
- LLM jako bramka (faza 2).
- Podpisy kryptograficzne kluczem prywatnym (faza 4; teraz hash chain).
- Wieloprocesowy lock i rotacja logów (faza 3/6).
- `jfp-edge-radar` — to ingress, nie guard (patrz §3.6 PLAN.md).
