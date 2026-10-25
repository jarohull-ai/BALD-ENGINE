#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Wybierz najświeższy plik binarny (release ma pierwszeństwo)
if [ -f "$DIR/target/release/bald" ]; then
    BIN="$DIR/target/release/bald"
elif [ -f "$DIR/target/debug/bald" ]; then
    BIN="$DIR/target/debug/bald"
else
    echo "Kompilacja BALD..."
    (cd "$DIR" && cargo build --release -p bse-cli)
    BIN="$DIR/target/release/bald"
fi

# Jeśli serwer już działa, zrestartuj go, aby załadować najnowszą wersję silnika i UI
if pgrep -f "bald serve" >/dev/null 2>&1; then
    echo "Przeładowywanie aktywnej sesji BALD..."
    pkill -f "bald serve" || true
    sleep 0.8
fi

echo "Uruchamianie BALD Search Engine na http://127.0.0.1:7333 ..."
"$BIN" serve --bind 127.0.0.1:7333 &
SERVER_PID=$!

sleep 1.2

# Otwórz przeglądarkę
xdg-open "http://127.0.0.1:7333" 2>/dev/null || sensible-browser "http://127.0.0.1:7333" || true

wait $SERVER_PID
