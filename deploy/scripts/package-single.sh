#!/usr/bin/env bash
# Assemble the single-host bundle: one server binary plus one static dir.
#
# Layout of the bundle:
#   dist/wf-single/
#     wf-server          # server binary (serves API + pages on one port)
#     web/               # frontend static bundle (apps/web-app/build content)
#     settings/server/server.toml  # loopback bind + static_dir example
#     run-single.sh      # local start helper
#
# Usage:
#   deploy/scripts/package-single.sh [--debug] [--skip-frontend-install] [--preview]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="$ROOT/dist/wf-single"
WEB="$DIST/web"

MODE="--release"
FRONT_ARGS=()
for arg in "$@"; do
    case "$arg" in
        --debug) MODE="" ;;
        --skip-frontend-install) FRONT_ARGS+=("--skip-install") ;;
        --preview) FRONT_ARGS+=("--preview") ;;
        -h|--help|help)
            sed -n '2,15p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

echo "==> Building backend ..."
# shellcheck disable=SC2086
cargo build $MODE -p wf-server
if [[ -n "$MODE" ]]; then
    BIN="$ROOT/target/release/wf-server"
else
    BIN="$ROOT/target/debug/wf-server"
fi
if [[ ! -x "$BIN" ]]; then
    echo "Error: missing backend binary $BIN" >&2
    exit 1
fi

echo "==> Building frontend ..."
bash "$ROOT/deploy/scripts/build-frontend.sh" "${FRONT_ARGS[@]}"
if [[ ! -f "$ROOT/apps/web-app/build/index.html" ]]; then
    echo "Error: missing frontend bundle $ROOT/apps/web-app/build/index.html" >&2
    exit 1
fi

echo "==> Assembling single-host bundle ..."
rm -rf "$DIST"
mkdir -p "$WEB" "$DIST/settings/server"
install -m 755 "$BIN" "$DIST/wf-server"
cp -r "$ROOT/apps/web-app/build/." "$WEB/"
chmod -R a+r "$WEB"

cat > "$DIST/settings/server/server.toml" <<'EOF'
# Single-host bundle example: loopback API + static on one port.
# The relative static_dir resolves against the server executable, so
# this file works wherever the bundle directory is moved as a whole.
bind_addr = "127.0.0.1:3000"
static_dir = "web"
EOF

cat > "$DIST/run-single.sh" <<'EOF'
#!/usr/bin/env bash
# Start the single-host bundle from its own directory.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"
exec "$HERE/wf-server" --settings-dir "$HERE/settings" "$@"
EOF
chmod 755 "$DIST/run-single.sh"

echo "==> Bundle ready: $DIST"
ls "$DIST"
echo "    start: $DIST/run-single.sh"
echo "    open:  http://127.0.0.1:3000/"
echo "    health: curl http://127.0.0.1:3000/api/v1/info"
