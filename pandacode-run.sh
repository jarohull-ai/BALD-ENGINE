#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ ! -d "apps/ui/dist" ] || [ ! -f "apps/ui/dist/index.html" ]; then
    echo "==> Building apps/ui..."
    (cd apps/ui && npm run build)
fi

echo "==> Starting BALD web UI (http://127.0.0.1:7333)..."
exec cargo run -p bse-cli -- serve "$@"
