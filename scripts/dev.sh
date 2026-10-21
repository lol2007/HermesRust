#!/usr/bin/env bash
# HermesRust — development helper (Linux/macOS).
#   ./scripts/dev.sh            build + run with window
#   ./scripts/dev.sh --test     build + run functional test suite (mock upstream)
set -euo pipefail
cd "$(dirname "$0")/.."

case "${1:-run}" in
  --test|test)
    cargo build --release
    python3 tests/mock_upstream.py & MOCK=$!
    trap 'kill $MOCK 2>/dev/null || true' EXIT
    sleep 0.4
    ./target/release/hermes-rust --no-window & APP=$!
    trap 'kill $MOCK $APP 2>/dev/null || true' EXIT
    sleep 1.2
    # point at the mock upstream for the test, then restore
    python3 - <<'EOF'
import re
p = 'config/config.toml'
s = open(p).read()
s = re.sub(r'endpoint = "[^"]*"', 'endpoint = "http://127.0.0.1:9999/v1"', s)
open(p, 'w').write(s)
EOF
    bash tests/run_tests.sh
    ;;
  *)
    cargo build --release
    ./target/release/hermes-rust "$@"
    ;;
esac
