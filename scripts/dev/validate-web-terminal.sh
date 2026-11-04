#!/usr/bin/env bash
# Minimal end-to-end validation harness for:
#   - Web terminal page running an installed TUI (for example lazygit)
#   - IDE page file/git operations
#   - Git page operations
#
# By default this uses already-built artifacts. Set WF_VALIDATE_BUILD=1 to
# rebuild them. Debug server settings are relaxed and written under
# WF_VALIDATE_SETTINGS_DIR.
#
# The validation repository is seeded with a small project, several commits,
# branches, a tag, a stash entry and uncommitted changes so that the IDE and
# Git pages have representative data on first load. Set WF_VALIDATE_SEED=0 to
# disable seeding, or WF_VALIDATE_RESET_REPO=1 to recreate the repository.

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

if ! command -v git >/dev/null 2>&1; then
    echo "Missing required tool: git" >&2
    exit 1
fi

if ! command -v lazygit >/dev/null 2>&1; then
    echo "Warning: lazygit is not installed; the terminal quick command will be unavailable." >&2
fi

needs_repo_seed=0
if [ ! -d "$REPO/.git" ]; then
    needs_repo_seed=1
fi
if [ "${WF_VALIDATE_RESET_REPO:-0}" = "1" ]; then
    needs_repo_seed=1
fi
if [ "${WF_VALIDATE_SEED:-1}" != "0" ] && [ -d "$REPO/.git" ] && [ ! -f "$REPO/.git/wf-validate-seeded" ]; then
    needs_repo_seed=1
fi

if [ "$needs_repo_seed" = "1" ]; then
    if [ -e "$REPO" ]; then
        case "$REPO" in
            /tmp/*) rm -rf "$REPO" ;;
            *)
                echo "Refusing to recreate repository outside /tmp: $REPO" >&2
                echo "Set WF_VALIDATE_REPO to a path under /tmp or remove it manually." >&2
                exit 1
                ;;
        esac
    fi

    if [ "${WF_VALIDATE_SEED:-1}" = "0" ]; then
        echo "Preparing minimal validation git repository..."
        mkdir -p "$REPO"
        git -C "$REPO" init -q
        git -C "$REPO" checkout -q -b main 2>/dev/null || git -C "$REPO" branch -M main
        git -C "$REPO" config user.email "validate@example.com"
        git -C "$REPO" config user.name "Validate"
        printf 'hello\n' > "$REPO/README.md"
        git -C "$REPO" add README.md
        git -C "$REPO" commit -q -m "init"
        touch "$REPO/.git/wf-validate-seeded"
    else
        echo "Preparing seeded validation git repository..."
        mkdir -p "$REPO"
        git -C "$REPO" init -q
        git -C "$REPO" checkout -q -b main 2>/dev/null || git -C "$REPO" branch -M main
        git -C "$REPO" config user.email "validate@example.com"
        git -C "$REPO" config user.name "Validate"

        mkdir -p "$REPO/src" "$REPO/src/config" "$REPO/docs" "$REPO/tests" "$REPO/assets"

        cat > "$REPO/README.md" <<'EOF'
# Validation Repository

This repository supports end-to-end validation of the web terminal, IDE and Git pages.

VALIDATE_SEARCH_NEEDLE: readme overview
EOF

        cat > "$REPO/src/main.rs" <<'EOF'
fn main() {
    println!("hello validation");
    // VALIDATE_SEARCH_NEEDLE: main entry
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
EOF

        cat > "$REPO/src/lib.rs" <<'EOF'
pub fn answer() -> u32 {
    42
}

// VALIDATE_SEARCH_NEEDLE: library helper
EOF

        cat > "$REPO/src/config/app.toml" <<'EOF'
[server]
port = 3000

# VALIDATE_SEARCH_NEEDLE: app config
EOF

        cat > "$REPO/docs/guide.md" <<'EOF'
# Guide

Validation guide content.

VALIDATE_SEARCH_NEEDLE: guide section
EOF

        cat > "$REPO/docs/notes.txt" <<'EOF'
Scratch notes for validation.
VALIDATE_SEARCH_NEEDLE: notes file
EOF

        cat > "$REPO/tests/smoke_test.rs" <<'EOF'
#[test]
fn smoke() {
    assert_eq!(2 + 2, 4);
}
EOF

        cat > "$REPO/assets/data.json" <<'EOF'
{
  "name": "validate",
  "value": 7,
  "needle": "VALIDATE_SEARCH_NEEDLE: json data"
}
EOF

        printf 'binary\0content\0for\0validation\n' > "$REPO/assets/logo.bin"

        git -C "$REPO" add -A
        git -C "$REPO" commit -q -m "feat: initial validation project"

        printf '\nUpdated during validation setup.\nVALIDATE_SEARCH_NEEDLE: readme update\n' >> "$REPO/README.md"
        cat > "$REPO/src/main.rs" <<'EOF'
fn main() {
    println!("hello validation updated");
    // VALIDATE_SEARCH_NEEDLE: main entry updated
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert!(true);
    }
}
EOF
        git -C "$REPO" add README.md src/main.rs
        git -C "$REPO" commit -q -m "docs: update readme and main entry"

        git -C "$REPO" checkout -q -b feature/demo
        mkdir -p "$REPO/src/features"
        cat > "$REPO/src/features/demo.rs" <<'EOF'
pub fn demo() -> &'static str {
    "demo"
}

// VALIDATE_SEARCH_NEEDLE: feature demo
EOF
        git -C "$REPO" add src/features/demo.rs
        git -C "$REPO" commit -q -m "feat: add demo feature"

        git -C "$REPO" checkout -q main
        git -C "$REPO" tag -a v0.1.0 -m "validation release 0.1.0"

        printf '\n// temporary stash change\n' >> "$REPO/src/lib.rs"
        git -C "$REPO" stash push -q -m "wip: library tweak"

        printf '\n// unstaged working tree change\n' >> "$REPO/src/main.rs"
        printf '\n## Staged validation update\n' >> "$REPO/docs/guide.md"
        git -C "$REPO" add docs/guide.md
        printf 'temporary scratchpad\nVALIDATE_SEARCH_NEEDLE: scratchpad\n' > "$REPO/scratchpad.txt"

        touch "$REPO/.git/wf-validate-seeded"
    fi
fi

if [ ! -d "$REPO/.git" ]; then
    echo "Missing git repository: $REPO" >&2
    exit 1
fi

echo "Preparing relaxed debug server configuration..."
mkdir -p "$SETTINGS_DIR/server"
cat > "$SETTINGS_DIR/server/cors.toml" <<'CORS'
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
allowed_headers = ["Content-Type", "Authorization", "x-api-key", "x-wf-ticket"]
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
echo "Seeded repo: branches main/feature/demo, tag v0.1.0, one stash, dirty working tree."

exec "$SERVER_BIN" \
    --storage memory \
    --repo-root "$REPO" \
    --settings-dir "$SETTINGS_DIR" \
    --static-dir "$FRONTEND_BUILD" \
    --addr "127.0.0.1:$PORT"
