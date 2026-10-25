# ADR-008 — `bse-api` i `apps/ui`: hybrydowa architektura interfejsu (Web + Tauri)

- **Status:** accepted
- **Data:** 2026-10-25
- **Decydenci:** Jaro (właściciel produktu), implementacja: BALD
- **Zastępuje:** pierwotną koncepcję czystego Leptosa w oknie Tauri
- **Powiązane:** PLAN.md §4, §6.6, §11, §16 (D2, D12), ADR-005, ADR-007

---

## 1. Kontekst

W pierwotnym planie interfejs użytkownika miał opierać się wyłącznie na Leptos (Rust WASM) osadzonym w oknie natywnym Tauri 2. W praktyce napotkano dwie twarde bariery:

1. **Zależności systemowe Linuxa:** Budowanie aplikacji desktopowej Tauri na Linuxie wymaga pakietów `libwebkit2gtk-4.1-dev` oraz `libjavascriptcoregtk-4.1-dev`. Na wielu maszynach środowiskowych, serwerach czy kontenerach pakiety te nie są zainstalowane lub ich instalacja wymaga uprawnień root, co uniemożliwia uruchomienie GUI.
2. **Elastyczność i ergonomia pracy:** Użytkownik i analitycy badający źródła (np. BaldTV) często preferują pracę w swojej codziennej, zaufanej przeglądarce internetowej (Brave, Chrome, Firefox) z pełną obsługą zakładek i rozszerzeń.

Jednocześnie interfejs nie może być „wystawiony na świat” — BALD jest suwerennym narzędziem lokalnym o podwyższonym rygorze bezpieczeństwa.

---

## 2. Decyzja

Wdrażamy **architekturę hybrydową**: wspólny rdzeń API (`crates/bse-api`) zasilający równolegle:
- **Tryb Web:** serwowany przez komendę CLI `bald serve --bind 127.0.0.1:7333` bezpośrednio do przeglądarki,
- **Tryb Desktop:** natywne okno Tauri 2 korzystające z tego samego `Service` przez komendy IPC.

### 2.1 Stos technologiczny interfejsu (`apps/ui`)

Zgodnie z wytycznymi zaadaptowano lekki, sprawdzony stos z **Polyglot Lab**:
- **React 19 + TypeScript + Vite + Tailwind CSS + Lucide Icons**.
- Kompilacja produkuje statyczne assety HTML/CSS/JS (`apps/ui/dist`), które są wbudowywane bezpośrednio w binarkę `bald` za pomocą makra `include_dir`.
- Dostępny jest tryb deweloperski `--ui-dir apps/ui/dist` umożliwiający dynamiczne odświeżanie interfejsu bez rekompilacji binarnej.

### 2.2 Serwis API i bezpieczeństwo (`crates/bse-api`)

- **Bindowanie wyłącznie loopback:** Serwer Axum nasłuchuje domyślnie na `127.0.0.1:7333`.
- **Token sesji przeciwko CSRF/DNS-rebinding:** Przy starcie generowany jest losowy token sesyjny (32 bajty hex), wstrzykiwany do `<meta name="bald-token">` w serwowanym `index.html`. Każde zapytanie `/api/*` (z wyjątkiem `/api/health`) wymaga nagłówka `x-bald-token`.
- **Nagłówki bezpieczeństwa:**
  - `Content-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'`
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: no-referrer`
- **Bezpieczny widok ustawień:** Endpoint `/api/settings` maskuje tajne klucze API. Zwraca wyłącznie informację o stanie konfiguracji (`status: "env" | "config" | "none"`), zapobiegając odczytowi sekretów przez interfejs.
- **Zapis atomowy konfiguracji:** Zapis ustawień przez `/api/settings` tworzy plik tymczasowy, ustawia uprawnienia `0600` i atomowo podmienia docelowy `config.toml`.

### 2.3 Funkcjonalności UI

- **Claims Grid:** Siatka kafli twierdzeń z wyszukiwaniem, filtrowaniem po stance (`mentions`, `reports`, `endorses`, `debunks`) i sortowaniem po dacie/zaufaniu.
- **Evidence Drawer:** Wysuwany panel boczny prezentujący szczegóły twierdzenia, podmiot, link źródłowy, wiarygodność, model klasyfikujący oraz **dosłowny cytat z podświetleniem współrzędnych znakowych (span)**.
- **Settings View:** Panel konfiguracji providerów wyszukiwania, lokalnej Ollamy, zdalnych modeli OpenAI/OpenRouter, limitów tokenów i timeoutów.
- **Lokalizacja:** Pełna dwujęzyczność (PL/EN) z możliwością dynamicznego przełączania.

---

## 3. Konsekwencje

### Pozytywne
1. **Zero blokad środowiskowych:** `bald serve` działa natychmiast na każdym systemie Linux, macOS i Windows bez instalacji WebKitGTK.
2. **Brak duplikacji logiki:** Zarówno Tauri IPC, jak i serwer HTTP korzystają z identycznego traitu `Service`.
3. **Pojedynczy plik binarny:** Kompilacja release osadza cały frontend wewnątrz pliku wykonywalnego `bald`.

### Negatywne / Ograniczenia
1. W trybie przeglądarkowym wymagane jest podanie tokenu sesji (obsługiwane automatycznie przez `api.ts`).
2. Klucze zapisywane przez UI trafiają do pliku `0600`, a nie do systemowego keyringu (świadomy kompromis Fazy 3).

---

## 4. Weryfikacja

- `cargo test --workspace` — 103 testy przechodzące, w tym 10 dedykowanych testów `bse-api` (tokeny, traversal, walidacja zakresów, redakcja kluczy).
- Smoke test `curl http://127.0.0.1:7333/api/health` zwraca poprawny JSON statusu.
- Smoke test przeglądarkowy potwierdził renderowanie 8 kafelków claims, otwieranie drawera, poprawność przełączania języków PL/EN oraz brak błędów w konsoli.
