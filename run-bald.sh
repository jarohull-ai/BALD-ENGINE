#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$DIR/target/debug/bald"

if [ ! -f "$BIN" ]; then
    echo "Kompilacja BALD..."
    (cd "$DIR" && cargo build -p bse-cli)
fi

# Sprawdź, czy serwer już działa na 127.0.0.1:7333
if curl -s http://127.0.0.1:7333/api/health >/dev/null 2>&1; then
    echo "BALD już działa. Otwieram przeglądarkę..."
    xdg-open "http://127.0.0.1:7333" 2>/dev/null || sensible-browser "http://127.0.0.1:7333" || true
    exit 0
fi

echo "Uruchamianie BALD Search Engine na http://127.0.0.1:7333 ..."
"$BIN" serve --bind 127.0.0.1:7333 &
SERVER_PID=$!

# Czekaj chwilę na start serwera
sleep 1.2

# Otwórz domyślną przeglądarkę
xdg-open "http://127.0.0.1:7333" 2>/dev/null || sensible-browser "http://127.0.0.1:7333" || true

wait $SERVER_PID
