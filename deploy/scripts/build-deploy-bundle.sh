#!/usr/bin/env bash
# Build a single-machine deployment bundle: wf-server binary + web static dir
# + deploy templates. Output goes under target/deploy-bundle/<version>/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

bash "$ROOT/scripts/sync-deploy.sh" --check

VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}"
TARGET="${TARGET:-}"
OUT="${OUT:-$ROOT/target/deploy-bundle/wf-agent-$VERSION}"
BUILD_WEB="${BUILD_WEB:-yes}"

echo "Building wf-server release binary..."
if [[ -n "$TARGET" ]]; then
  cargo build --release -p wf-server --target "$TARGET"
  BIN="$ROOT/target/$TARGET/release/wf-server"
else
  cargo build --release -p wf-server
  BIN="$ROOT/target/release/wf-server"
fi

if [[ "$BUILD_WEB" == "yes" ]]; then
  echo "Building web frontend..."
  npm --prefix "$ROOT/apps/web-app" run build
fi

rm -rf "$OUT"
mkdir -p "$OUT/bin" "$OUT/web" "$OUT/deploy"

cp "$BIN" "$OUT/bin/wf-server"
chmod 0755 "$OUT/bin/wf-server"

if [[ -f "$ROOT/apps/web-app/build/index.html" ]]; then
  cp -r "$ROOT/apps/web-app/build/." "$OUT/web/"
else
  echo "WARNING: web build not found; bundle will be API-only" >&2
fi

cp -r "$ROOT/deploy/templates" "$OUT/deploy/"
cp -r "$ROOT/deploy/scripts" "$OUT/deploy/"
cp "$ROOT/deploy/systemd/wf-server.service.template" "$OUT/deploy/" 2>/dev/null || true

(
  cd "$OUT"
  find . -type f -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
)

echo "Bundle ready: $OUT"
echo "Static dir for --static-dir: $OUT/web"
