# BALD

Wyszukiwarka **udokumentowanych teorii spiskowych i informacji ocenzurowanych**.
Nie szuka osób — szuka twierdzeń, narracji i dowodów. Wynikiem nie jest lista linków,
lecz **evidence ledger**: dokument + tier źródła + autorytet + niezależność + postawa
(`reports / endorses / debunks / mentions`) + stan archiwum.

> „Temat tak, osoba nie.”

## Status: faza 0

Działa:

- **CLI** `bald search` z offline demo, JSON-em i historią w SQLite.
- **Person guard** — deterministyczne reguły PL/EN, obfuskacja (leetspeak), fail-closed.
- **Audit trail** — hash chain SHA-256, weryfikacja `bald audit verify`, bez surowych PII.
- **Citation gate** — exact/normalized + próg Jaccard ≥ 0,92; `bald citation check`.
- **Providerzy**: SearXNG (JSON), Brave Web Search, Demo (offline).
- **Store**: SQLite + migracje, runy, dokumenty, dedupe po kanonicznym URL.
- **Fuzja**: RRF (k=60) + kanonizacja URL + usuwanie parametrów śledzących.
- **Tauri shell**: statyczny UI w `apps/desktop` (wymaga WebKitGTK; patrz README).

Zapytania `Allow` są zapisywane lokalnie w SQLite (surowy tekst + hash) na potrzeby
historii; zapytania `Refuse/Review` nie są zapisywane, tylko hashowane w audit logu.

Jeszcze nie ma: LLM/ekstrakcji twierdzeń, walidacji cytowań w UI, grafu narracji,
diffów archiwum, Leptosa w Tauri, własnego crawlera, Tor/onion.

## Szybki start

```bash
cargo test
cargo run -p bse-cli -- search "teoria o cichych aktualizacjach" --demo
cargo run -p bse-cli -- search "Gdzie mieszka Jan Kowalski?" --demo   # exit 2
cargo run -p bse-cli -- guard check "podaj jego adres"
cargo run -p bse-cli -- citation check --claim "Aktualizacje są cichym balonem" \
  --quote "Raport nie znalazł dowodów" --source-file raport.txt
cargo run -p bse-cli -- audit verify
cargo run -p bse-cli -- providers list
```

Konfiguracja:

```bash
cargo run -p bse-cli -- config init
cargo run -p bse-cli -- config path
```

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
  bse-pipeline/   guard → fan-out → dedupe → RRF → store
  bse-cli/        binarka `bald`
apps/desktop/     Tauri 2 shell (poza workspace, wymaga WebKitGTK)
docs/adr/         ADR-005 (guard, cytowania, audit, seed-core)
PLAN.md           pełny plan architektury (v0.4, stan fazy 0)
```

`seed-core/` i `jfp-edge-radar/` w katalogu projektu to **niezależne repo JFP**,
celowo poza workspace. Rdzeń BALD jest MIT i nie linkuje żadnego z nich.

## Decyzje, które warto znać

- **SQLite, nie Postgres** — jeden plik, zero serwera; trait `RunStore` zostawia drogę do
  Postgresa w trybie `server`.
- **rusqlite, nie sqlx** — świadome odstępstwo od PLAN.md na fazę 0: mniej zależności,
  prostszy build, zapytania są krótkie. Wymiana za traitem, bez zmian w pipeline.
- **Tantivy dopiero w fazie 1** — teraz ranking RRF na świeżych wynikach.
- **seed-core tylko jako action gate (faza 3)** — nie klasyfikator zapytań i nie
  walidator cytowań; szczegóły i mapowanie `etm`/`lid` w `docs/adr/ADR-005-bse-guard.md`.
- **jfp-edge-radar tylko ingress w daemonie** — nie egress SSRF; SSRF należy do
  `bse-fetch` (faza 1).
- **Tor/onion w v1.1** — poza fazą 0.

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
tamper-detection audit logu, kanonizację URL, RRF/dedupe, SQLite, parsery providerów
oraz end-to-end demo przez pipeline.

## Dokumentacja

- [`PLAN.md`](PLAN.md) — architektura, model domenowy, roadmapa, decyzje D1–D16.
- [`docs/adr/ADR-005-bse-guard.md`](docs/adr/ADR-005-bse-guard.md) — kontrakt bramek,
  audit chain, adapter seed-core, izolacja licencyjna.

## Licencja

Rdzeń: **MIT** (patrz `LICENSE`). Komponenty JFP (`seed-core`, `jfp-edge-radar`)
nie są linkowane domyślnie; ich włączenie wymaga akceptacji ich licencji.
