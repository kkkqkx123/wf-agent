#!/usr/bin/env bash
# Minimal end-to-end validation harness for:
#   - Web terminal page running an installed TUI (for example lazygit)
#   - IDE page file/git operations
#   - Git page operations
#
# By default this uses already-built artifacts. Set WF_VALIDATE_BUILD=1 to
# rebuild them. Debug server settings are relaxed and written under
# WF_VALIDATE_SETTINGS_DIR.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SERVER_BIN="$ROOT/target/debug/wf-server"
FRONTEND_DIR="$ROOT/apps/web-app"
FRONTEND_BUILD="$FRONTEND_DIR/build"
REPO="${WF_VALIDATE_REPO:-/tmp/wf-agent-validate-repo}"
SETTINGS_DIR="${WF_VALIDATE_SETTINGS_DIR:-/tmp/wf-agent-validate-settings}"
PORT="${WF_VALIDATE_PORT:-3000}"

if [ "${WF_VALIDATE_BUILD:-0}" = "1" ]; then
    echo "Building backend and frontend artifacts..."
    cargo build -p wf-server
    npm --prefix "$FRONTEND_DIR" run build
fi

if [ ! -x "$SERVER_BIN" ]; then
    echo "Missing backend artifact: $SERVER_BIN" >&2
    echo "Run: cargo build -p wf-server" >&2
    exit 1
fi

if [ ! -d "$FRONTEND_BUILD" ]; then
    echo "Missing frontend artifact: $FRONTEND_BUILD" >&2
    echo "Run: npm --prefix $FRONTEND_DIR run build" >&2
    exit 1
fi

echo "Preparing validation git repository..."
if [ ! -d "$REPO/.git" ]; then
    mkdir -p "$REPO"
    git -C "$REPO" init -q
    printf 'hello\n' > "$REPO/README.md"
    git -C "$REPO" add README.md
    git -C "$REPO" commit -q -m "init"
fi

echo "Preparing relaxed debug server configuration..."
mkdir -p "$SETTINGS_DIR/server"
cat > "$SETTINGS_DIR/server/cors.toml" <<'CORS'
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allowed_headers = ["Content-Type", "Authorization", "x-api-key"]
CORS
cat > "$SETTINGS_DIR/server/auth.toml" <<'AUTH'
enabled = false
header_name = "x-api-key"
allow_query_param = true
query_param_name = "api_key"
excluded_paths = ["/health", "/api/v1/info", "/"]
AUTH
cat > "$SETTINGS_DIR/server/rate-limit.toml" <<'RATE'
enabled = false
window_ms = 60000
max_requests = 100
excluded_paths = ["/health", "/api/v1/info", "/"]
RATE

echo "Starting wf-server..."
echo "  Terminal : http://127.0.0.1:$PORT/terminal"
echo "  IDE      : http://127.0.0.1:$PORT/ide"
echo "  Git      : http://127.0.0.1:$PORT/git"
echo "  Repo     : $REPO"
echo "  Settings : $SETTINGS_DIR"
echo
echo "The web terminal's lazygit quick command uses the installed lazygit binary."

exec "$SERVER_BIN" \
    --storage memory \
    --repo-root "$REPO" \
    --settings-dir "$SETTINGS_DIR" \
    --static-dir "$FRONTEND_BUILD" \
    --addr "127.0.0.1:$PORT"
