# BALD Desktop (Tauri 2) — faza 0

Ten crate jest **poza workspace** BALD (`[workspace]` w `Cargo.toml`), bo wymaga
systemowego WebKitGTK. Rdzeń, CLI i testy budują się bez niego.

## Co jest w środku

- `tauri.conf.json` — konfiguracja Tauri 2, `frontendDist: dist`.
- `dist/` — statyczny shell UI (HTML/CSS/JS) z offline fixtures.
- `src/main.rs` — minimalne okno Tauri.

**To nie jest jeszcze produkt.** UI jest mockiem: nie ma IPC, nie woła CLI,
nie ma Leptosa. Ma pokazać układ ekranu wyszukiwania, gate'u i kart dowodów.
Prawdziwe Leptos + `invoke` wchodzi w fazie 3.

## Wymagania systemowe (Linux)

```bash
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

W tym środowisku `pkg-config --modversion webkit2gtk-4.1` zwraca brak — dopóki
nie zostanie zainstalowany, ten crate się nie zbuduje (celowo nie jest częścią
`cargo test`).

## Uruchomienie

```bash
# z katalogu apps/desktop
cargo install tauri-cli --version "^2"   # jednorazowo
cargo tauri dev
```

Albo bez CLI:

```bash
cd apps/desktop
cargo run
```

## Podgląd samego UI bez Tauri

Otwórz `dist/index.html` w przeglądarce. Formularz działa lokalnie (mock guarda),
dane pochodzą z `app.js`, żadne zapytanie nie wychodzi do sieci.
