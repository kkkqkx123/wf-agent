#!/usr/bin/env bash
# Download the real LiteLLM model pricing table used to seed the embedded
# public pricing table in wf-pricing.
#
# Usage:
#   scripts/fetch-litellm-pricing.sh
#
# Network access is required. When behind a proxy, export it first, e.g.:
#   export https_proxy="http://localhost:7890"
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
URL="https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json"
OUT="$ROOT/crates/infra/wf-pricing/src/data/litellm_public.json"

mkdir -p "$(dirname "$OUT")"
echo "Downloading $URL"
curl -fL --retry 3 --retry-delay 2 -o "$OUT" "$URL"

python3 - "$OUT" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as f:
    data = json.load(f)
if not isinstance(data, dict) or not data:
    raise SystemExit("downloaded file is not a non-empty JSON object")
print(f"validated: {len(data)} model entries -> {path}")
PY
