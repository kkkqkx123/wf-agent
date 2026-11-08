#!/usr/bin/env bash
# Show wf-server service status for quick diagnosis (no root required
# for reads, though journal access may need group membership).
#
# Usage: ./service-status.sh

set -euo pipefail

SERVICE_NAME="${SERVICE_NAME:-wf-server}"

echo "== systemd unit =="
systemctl status "$SERVICE_NAME" --no-pager || true

echo
echo "== enabled state =="
systemctl is-enabled "$SERVICE_NAME" 2>&1 || true

echo
echo "== active state =="
systemctl is-active "$SERVICE_NAME" 2>&1 || true

echo
echo "== recent logs =="
journalctl -u "$SERVICE_NAME" -n 50 --no-pager 2>&1 || true
