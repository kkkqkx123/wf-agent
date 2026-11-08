#!/usr/bin/env bash
# Install wf-server as a managed systemd service (run as root).
#
# Pipeline: fetch package (optional) -> move artifacts -> render managed
# config -> pin path anchors in the env file -> render + install unit.
#
# Configuration is initialized exclusively through render-config.sh
# (--to managed); this script never writes business values itself.
# Only a newly rendered env file gets path anchors filled in
# (WF_STORAGE_SQLITE_DB_PATH, WF_OUTPUT_DIR); an existing env file is
# never touched, and secret keys stay empty for ops to fill in.
#
# Usage: sudo ./install-server.sh (see env knobs below)

set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
CONFIG_DIR="${CONFIG_DIR:-/etc/wf}"
DATA_DIR="${DATA_DIR:-/var/lib/wf}"
LOG_DIR="${LOG_DIR:-/var/log/wf}"
STATIC_DIR="${STATIC_DIR:-/usr/share/wf/web}"
SERVICE_NAME="${SERVICE_NAME:-wf-server}"
SERVICE_USER="${SERVICE_USER:-wf}"
SERVICE_GROUP="${SERVICE_GROUP:-wf}"

# Empty PROJECT_ROOT keeps the fail-loud behavior: the rendered unit runs
# without --config and directory-less workspace requests keep failing.
PROJECT_ROOT="${PROJECT_ROOT:-}"

ENABLE_SERVICE="${ENABLE_SERVICE:-no}"   # yes|no: force enable/disable
START_SERVICE="${START_SERVICE:-no}"     # yes|no: start after install

FETCH="${FETCH:-no}"                     # yes: download via fetch-package.sh first
PACKAGE_VERSION="${PACKAGE_VERSION:-}"   # default: workspace version
PACKAGE_MODULE="${PACKAGE_MODULE:-server}"
BIN_SRC_DIR="${BIN_SRC_DIR:-}"           # artifact dir (local build or fetched)
STATIC_SRC="${STATIC_SRC:-}"             # web build dir; empty lets the script probe
INSTALL_CLI="${INSTALL_CLI:-no}"        # yes: also install wf/wf-mini/wf-headless/wf-debug
INSTALL_WEB="${INSTALL_WEB:-yes}"        # yes: install apps/web-app/build as the static dir
SKIP_CONFIG="${SKIP_CONFIG:-no}"         # yes: skip managed config rendering
SKIP_UNIT="${SKIP_UNIT:-no}"             # yes: skip unit installation

normalize() {
    local p="${1%/}"
    printf '%s\n' "${p:-/}"
}
PREFIX="$(normalize "$PREFIX")"
CONFIG_DIR="$(normalize "$CONFIG_DIR")"
DATA_DIR="$(normalize "$DATA_DIR")"
LOG_DIR="$(normalize "$LOG_DIR")"
STATIC_DIR="$(normalize "$STATIC_DIR")"
BIN_PATH="${PREFIX}/bin/wf-server"

if [[ "$(id -u)" -ne 0 ]]; then
    echo "ERROR: must run as root" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$DEPLOY_ROOT/.." && pwd)"
RENDER="$SCRIPT_DIR/render-config.sh"
UNIT_TEMPLATE="$DEPLOY_ROOT/systemd/wf-server.service.template"
UNIT_PATH="/etc/systemd/system/${SERVICE_NAME}.service"

if [[ ! -f "$UNIT_TEMPLATE" ]]; then
    echo "ERROR: unit template not found: $UNIT_TEMPLATE" >&2
    exit 1
fi

CLI_BINS=(wf wf-mini wf-headless wf-debug)

# Optionally fetch the distribution package first; otherwise artifacts are
# resolved from local build output below.
FETCH_DEST=""
if [[ "$FETCH" == "yes" ]]; then
    FETCH_DEST="$(mktemp -d)"
    trap 'rm -rf "$FETCH_DEST"' EXIT
    version_args=()
    if [[ -n "$PACKAGE_VERSION" ]]; then
        version_args=(--version "$PACKAGE_VERSION")
    fi
    if [[ "$INSTALL_CLI" == "yes" && "$PACKAGE_MODULE" == "server" ]]; then
        PACKAGE_MODULE="all"
    fi
    "$SCRIPT_DIR/fetch-package.sh" --module "$PACKAGE_MODULE" \
        --dest "$FETCH_DEST" "${version_args[@]}"
    BIN_SRC_DIR="$FETCH_DEST"
fi

umask 027
echo "Installing wf-server service files..."

if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    groupadd --system "$SERVICE_GROUP"
fi
if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
    useradd --system --gid "$SERVICE_GROUP" --home-dir "$DATA_DIR" \
        --create-home --shell /usr/sbin/nologin "$SERVICE_USER"
fi

install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0750 \
    "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR"

# Resolve the artifact source: explicit dir, ./bin, repo target dir, cwd.
if [[ -z "$BIN_SRC_DIR" ]]; then
    for candidate in "$REPO_ROOT/bin" "$REPO_ROOT/target/release" "$REPO_ROOT" "./bin" "."; do
        if [[ -x "$candidate/wf-server" ]]; then
            BIN_SRC_DIR="$candidate"
            break
        fi
    done
fi

install_bin() {
    local name="$1"
    local src="$BIN_SRC_DIR/$name"
    local dst="$PREFIX/bin/$name"
    if [[ -x "$src" ]]; then
        install -d -m 0755 "$PREFIX/bin"
        install -m 0755 "$src" "$dst"
        echo "Installed binary from $src"
    elif [[ -x "$dst" ]]; then
        echo "Keeping existing binary: $dst"
    else
        echo "WARNING: $name not found in $BIN_SRC_DIR; install it manually" >&2
        return 1
    fi
    return 0
}

missing=0
install_bin "wf-server" || missing=1
if [[ "$INSTALL_CLI" == "yes" ]]; then
    for bin in "${CLI_BINS[@]}"; do
        install_bin "$bin" || missing=1
    done
fi
if [[ "$START_SERVICE" == "yes" && "$missing" == "1" ]]; then
    echo "ERROR: START_SERVICE=yes but binaries are missing" >&2
    exit 1
fi

# Frontend static dir: probe the repo build output when not given.
if [[ "$INSTALL_WEB" == "yes" ]]; then
    if [[ -z "$STATIC_SRC" && -f "$REPO_ROOT/apps/web-app/build/index.html" ]]; then
        STATIC_SRC="$REPO_ROOT/apps/web-app/build"
    fi
    if [[ -n "$STATIC_SRC" && -d "$STATIC_SRC" ]]; then
        mkdir -p "$STATIC_DIR"
        cp -r "$STATIC_SRC/." "$STATIC_DIR/"
        chown -R "root:root" "$STATIC_DIR"
        chmod -R a+rX "$STATIC_DIR"
        echo "Installed web static dir from $STATIC_SRC"
    elif [[ -d "$STATIC_DIR" ]]; then
        echo "Keeping existing web static dir: $STATIC_DIR"
    else
        echo "WARNING: web static dir not found; continuing without --static-dir" >&2
        STATIC_DIR=""
    fi
else
    STATIC_DIR=""
fi

if [[ "$SKIP_CONFIG" != "yes" ]]; then
    # Only a newly rendered env file gets path anchors filled in;
    # an existing env file holds live secrets and is never rewritten.
    if [[ -f "$CONFIG_DIR/wf-server.env" ]]; then
        env_is_new=0
    else
        env_is_new=1
    fi
    "$RENDER" --to managed --managed-dir "$CONFIG_DIR"
    if [[ ! -f "$CONFIG_DIR/wf-server.env" ]]; then
        echo "error: render did not produce $CONFIG_DIR/wf-server.env" >&2
        exit 1
    fi
    if [[ "$env_is_new" == "1" ]]; then
        sed -i "s|^WF_STORAGE_SQLITE_DB_PATH=$|WF_STORAGE_SQLITE_DB_PATH=$DATA_DIR/workflow.db|; s|^WF_OUTPUT_DIR=$|WF_OUTPUT_DIR=$LOG_DIR|" "$CONFIG_DIR/wf-server.env"
        echo "Pinned path anchors in $CONFIG_DIR/wf-server.env"
    fi
fi

if [[ "$SKIP_UNIT" != "yes" ]]; then
    tmp="$(mktemp)"
    trap 'rm -rf "$FETCH_DEST" "$tmp"' EXIT
    sed -e "s|@@MANAGED_DIR@@|$CONFIG_DIR|g" \
        -e "s|/usr/local/bin/wf-server|$BIN_PATH|g" \
        "$UNIT_TEMPLATE" > "$tmp"
    if [[ -n "$PROJECT_ROOT" ]]; then
        sed -i "s|@@PROJECT_ROOT@@|$PROJECT_ROOT|g; s|ReadWritePaths=\(.*\)|ReadWritePaths=\1 $DATA_DIR $LOG_DIR|" "$tmp"
    else
        # No default workspace: run without --config so directory-less
        # requests keep failing loud instead of binding somewhere silently.
        sed -i "s| --config @@PROJECT_ROOT@@||; s|ReadWritePaths=\(.*\) @@PROJECT_ROOT@@|ReadWritePaths=\1 $DATA_DIR $LOG_DIR|" "$tmp"
    fi
    if [[ -n "$STATIC_DIR" ]]; then
        sed -i "s|--settings-dir $CONFIG_DIR|--settings-dir $CONFIG_DIR --static-dir $STATIC_DIR|" "$tmp"
    fi
    if grep -q "@@" "$tmp"; then
        echo "error: unrendered placeholders remain in the unit" >&2
        exit 1
    fi
    install -m 0644 "$tmp" "$UNIT_PATH"
    rm -f "$tmp"
    systemctl daemon-reload
    echo "Installed $UNIT_PATH"
fi

if [[ "$ENABLE_SERVICE" == "yes" ]]; then
    systemctl enable "$SERVICE_NAME"
    echo "Enabled $SERVICE_NAME (autostart on boot)"
elif [[ "$ENABLE_SERVICE" == "no" ]]; then
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true
fi

if [[ "$START_SERVICE" == "yes" ]]; then
    systemctl restart "$SERVICE_NAME"
    systemctl --no-pager --full status "$SERVICE_NAME" || true
fi

echo "Done."
echo "Next: fill secrets in $CONFIG_DIR/wf-server.env, then manage with:"
echo "  systemctl {start|stop|restart|status|enable|disable} $SERVICE_NAME"
