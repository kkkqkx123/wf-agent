#!/usr/bin/env bash
# Uninstall the wf-server managed service (run as root).
#
# Stops and disables the unit, then removes it. Data and config are kept
# by default; binaries are never removed automatically.
#
# Usage: sudo ./uninstall-server.sh

set -euo pipefail

SERVICE_NAME="${SERVICE_NAME:-wf-server}"
CONFIG_DIR="${CONFIG_DIR:-/etc/wf}"
DATA_DIR="${DATA_DIR:-/var/lib/wf}"
LOG_DIR="${LOG_DIR:-/var/log/wf}"
PURGE_CONFIG="${PURGE_CONFIG:-no}"   # yes|no: also remove the managed config dir
PURGE_DATA="${PURGE_DATA:-no}"       # yes|no: also remove data and log dirs

UNIT_PATH="/etc/systemd/system/${SERVICE_NAME}.service"

if [[ "$(id -u)" -ne 0 ]]; then
    echo "ERROR: must run as root" >&2
    exit 1
fi

echo "Uninstalling wf-server service..."

if [[ -f "$UNIT_PATH" ]]; then
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "$UNIT_PATH"
    systemctl daemon-reload
fi
systemctl reset-failed "$SERVICE_NAME" 2>/dev/null || true

if [[ "$PURGE_CONFIG" == "yes" ]]; then
    rm -rf "$CONFIG_DIR"
    echo "Removed $CONFIG_DIR"
fi

if [[ "$PURGE_DATA" == "yes" ]]; then
    rm -rf "$DATA_DIR" "$LOG_DIR"
    echo "Removed data and log directories"
fi

echo "Done. Binaries left in place (remove manually if desired)."
