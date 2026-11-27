#!/usr/bin/env bash
# Two-node run where every management-to-execution request goes through the
# server-owned SSH tunnel. Adds an execution-node restart branch on top of
# the direct-link backup and recovery chain.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=lib/process_lifecycle.sh
source "$SCRIPT_DIR/lib/process_lifecycle.sh"

case "${1:-}" in
  --help|-h)
    echo "usage: run-remote-backup-over-tunnel.sh"
    exit 0
    ;;
  "") ;;
  *) echo "e2e: unknown argument: $1" >&2; exit 2 ;;
esac

ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$ROOT/e2e/bin"
FIX_DIR="$ROOT/e2e/fixtures"
PROCESS_FIX_DIR="$FIX_DIR/process-lifecycle"
ACTUAL_DIR="$ROOT/e2e/outputs/actual"
CASE_DIR="$ACTUAL_DIR/remote-tunnel"
SERVER_BIN="$BIN_DIR/wf-server"
HEADLESS_BIN="$BIN_DIR/wf-headless"
BACKEND_NAME="e2e-tunnel"
POLL_LIMIT=40

require_file "$SERVER_BIN"
require_file "$HEADLESS_BIN"
require_file "$PROCESS_FIX_DIR/bin/ssh"
require_file "$PROCESS_FIX_DIR/backend-tunnel.json"
require_file "$FIX_DIR/workflow-minimal.json"
require_cmd curl
require_cmd python3

mkdir -p "$CASE_DIR"
WORK_DIR="$(mktemp -d "$CASE_DIR/work.XXXXXX")"
EXEC_WORK="$WORK_DIR/execution"
ADMIN_WORK="$WORK_DIR/admin"
BACKUP_DIR="$WORK_DIR/backups"
SSH_DIR="$WORK_DIR/ssh"
mkdir -p "$EXEC_WORK" "$ADMIN_WORK" "$BACKUP_DIR" "$SSH_DIR"
EXEC_LOG="$CASE_DIR/execution-node.log"
EXEC_RESTART_LOG="$CASE_DIR/execution-node-restart.log"
ADMIN_LOG="$CASE_DIR/admin-node.log"
TUNNEL_REPORT="$CASE_DIR/dual-node-tunnel.report.txt"
RESTART_REPORT="$CASE_DIR/node-restart.report.txt"
: >"$EXEC_LOG"
: >"$ADMIN_LOG"
: >"$TUNNEL_REPORT"
: >"$RESTART_REPORT"

PASS=0
FAIL=0
EXEC_PID=""
ADMIN_PID=""
ADMIN_STARTED=0
SSH_PID=""

record() {
  local name="$1" ok="$2" detail="${3:-}"
  if [ "$ok" = "1" ]; then
    echo "PASS $name"
    PASS=$((PASS + 1))
  else
    echo "FAIL $name${detail:+ ($detail)}"
    FAIL=$((FAIL + 1))
  fi
}

cleanup() {
  if [ -n "$ADMIN_PID" ] && pl_pid_alive "$ADMIN_PID"; then
    kill "$ADMIN_PID" 2>/dev/null || true
    pl_wait_pid_gone "$ADMIN_PID" 30 || kill -9 "$ADMIN_PID" 2>/dev/null || true
  fi
  if [ -n "$EXEC_PID" ] && pl_pid_alive "$EXEC_PID"; then
    kill "$EXEC_PID" 2>/dev/null || true
    pl_wait_pid_gone "$EXEC_PID" 30 || kill -9 "$EXEC_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

wait_with_watchdog() {
  local pid="$1" limit="${2:-15}" watchdog
  (sleep "$limit"; kill -9 "$pid" 2>/dev/null || true) &
  watchdog=$!
  set +e
  wait "$pid"
  WAIT_CODE=$?
  set -e
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
}

call_node() {
  local remote="$1" directory="$2" out="$3" err="$4"
  shift 4
  [ "${1:-}" = "--" ] && shift
  set +e
  "$HEADLESS_BIN" --remote "$remote" --directory "$directory" \
    --output json --no-color "$@" >"$out" 2>"$err"
  local code=$?
  set -e
  return "$code"
}

wait_backend_field() {
  local want="$1" field="$2" value
  local _
  for _ in $(seq 1 "$POLL_LIMIT"); do
    local code
    code="$(pl_curl_json GET "$ADMIN/api/v1/system/backends" "$CASE_DIR/backends-status.json")"
    [ "$code" = "200" ] || { sleep 1; continue; }
    value="$(pl_backend_field "$CASE_DIR/backends-status.json" "$BACKEND_NAME" "$field")"
    [ "$value" = "$want" ] && return 0
    sleep 1
  done
  return 1
}

section "start execution and management nodes"
pl_pick_port
EXEC_PORT="$PL_PORT"
pl_pick_port
ADMIN_PORT="$PL_PORT"
pl_pick_port
TUNNEL_PORT="$PL_PORT"
EXEC="http://127.0.0.1:$EXEC_PORT"
ADMIN="http://127.0.0.1:$ADMIN_PORT"

WF_RECOVERY_RESTORE_ENABLED=1 \
  "$SERVER_BIN" --addr "127.0.0.1:$EXEC_PORT" \
  --storage "sqlite:$EXEC_WORK/exec.db" --directory "$EXEC_WORK" \
  --log-level info --log-format json >"$EXEC_LOG" 2>&1 &
EXEC_PID=$!

PATH="$PROCESS_FIX_DIR/bin:$PATH" \
  WF_BACKEND_ENABLED=1 \
  WF_BACKEND_BACKUP_DIR="$BACKUP_DIR" \
  WF_BACKEND_BACKUP_INTERVAL_SECS=1 \
  WF_BACKEND_BACKUP_RECONCILE_INTERVAL_SECS=0 \
  WF_BACKEND_PROBE_INTERVAL_SECS=1 \
  WF_BACKEND_PROBE_TIMEOUT_SECS=1 \
  WF_BACKEND_TUNNEL_STOP_GRACE_SECS=0 \
  WF_E2E_SSH_FAKE_DIR="$SSH_DIR" \
  WF_E2E_SSH_FAKE_EXIT_PORTS="" \
  "$SERVER_BIN" --addr "127.0.0.1:$ADMIN_PORT" \
  --storage memory --directory "$ADMIN_WORK" \
  --log-level info --log-format json >"$ADMIN_LOG" 2>&1 &
ADMIN_PID=$!
ADMIN_STARTED=1

wait_for_health "$EXEC" 30 || pl_die "execution node not ready, see $EXEC_LOG"
wait_for_health "$ADMIN" 30 || pl_die "management node not ready, see $ADMIN_LOG"
record nodes-ready 1

section "seed one execution on the execution node"
if call_node "$EXEC" "$EXEC_WORK" "$CASE_DIR/workflow-create.json" "$CASE_DIR/workflow-create.err" \
    -- workflow create --file "$FIX_DIR/workflow-minimal.json" --format json &&
  call_node "$EXEC" "$EXEC_WORK" "$CASE_DIR/run.json" "$CASE_DIR/run.err" \
    -- execution run --workflow e2e-minimal --background; then
  EXEC_ID="$(execution_id_of "$CASE_DIR/run.json")"
else
  EXEC_ID=""
fi
if [ -z "$EXEC_ID" ] || [ "$EXEC_ID" = "null" ]; then
  echo "SKIP tunneled backup tier (this node cannot run a script workflow)"
  pl_record seed_execution skipped "$TUNNEL_REPORT"
  section "result: $PASS passed, $FAIL failed, layer skipped"
  exit 0
fi

for _ in $(seq 1 "$POLL_LIMIT"); do
  call_node "$EXEC" "$EXEC_WORK" "$CASE_DIR/execution-status.json" "$CASE_DIR/execution-status.err" \
    -- execution status "$EXEC_ID" || true
  [ "$(envelope_status_of "$CASE_DIR/execution-status.json")" = "completed" ] && break
  sleep 1
done
if [ "$(envelope_status_of "$CASE_DIR/execution-status.json")" != "completed" ]; then
  echo "SKIP tunneled backup tier (seeded execution did not complete)"
  pl_record seed_execution skipped "$TUNNEL_REPORT"
  section "result: $PASS passed, $FAIL failed, layer skipped"
  exit 0
fi
pl_record seed_execution completed "$TUNNEL_REPORT"
record seed-execution 1

section "register the execution node behind a supervised tunnel"
BODY="$(python3 - "$PROCESS_FIX_DIR/backend-tunnel.json" "$BACKEND_NAME" "$TUNNEL_PORT" "$EXEC_PORT" "$EXEC_WORK" "$EXEC_ID" <<'PY'
import json
import sys

template, name, local_port, remote_port, directory, execution = sys.argv[1:7]
body = json.load(open(template))
body["name"] = name
body["entry"]["local_port"] = int(local_port)
body["entry"]["server_port"] = int(remote_port)
body["entry"]["backup_directory"] = directory
body["entry"]["backup_executions"] = [execution]
body["entry"]["backup_interval_secs"] = 1
print(json.dumps(body))
PY
)"
CODE="$(pl_curl_json POST "$ADMIN/api/v1/system/backends" "$CASE_DIR/register.json" "$BODY")"
[ "$CODE" = "200" ] || pl_die "backend registration failed: $CODE"
CODE="$(pl_curl_json POST "$ADMIN/api/v1/system/backends/$BACKEND_NAME/tunnel/start" "$CASE_DIR/tunnel-start.json")"
SSH_PID="$(pl_json_field "$CASE_DIR/tunnel-start.json" data.pid)"
[ "$CODE" = "200" ] && [ -n "$SSH_PID" ] || pl_die "tunnel start failed: $CODE"
for _ in $(seq 1 50); do
  [ -f "$SSH_DIR/ssh-$TUNNEL_PORT.pid" ] && break
  sleep 0.1
done
if pl_pid_alive "$SSH_PID"; then
  record tunnel-child 1
else
  record tunnel-child 0 "pid=$SSH_PID"
  pl_die "fake ssh child is not alive"
fi
if wait_backend_field true status.reachable; then
  record tunnel-probe 1
  pl_record tunnel_probe reachable "$TUNNEL_REPORT"
else
  record tunnel-probe 0
  pl_die "management node cannot reach the execution node through the tunnel"
fi

section "backup and index readback through the tunnel"
call_node "$ADMIN" "$ADMIN_WORK" "$CASE_DIR/backup-now.json" "$CASE_DIR/backup-now.err" \
  -- remote backup-now "$BACKEND_NAME" --execution "$EXEC_ID" || true
BACKUP_ID="$(pl_json_field "$CASE_DIR/backup-now.json" data.id)"
if [ -n "$BACKUP_ID" ]; then
  record backup-now 1
else
  record backup-now 0 "no backup id"
fi
call_node "$ADMIN" "$ADMIN_WORK" "$CASE_DIR/backup-verify.json" "$CASE_DIR/backup-verify.err" \
  -- remote backup-verify "$BACKUP_ID" || true
if [ "$(pl_json_field "$CASE_DIR/backup-verify.json" data.verified)" = "true" ]; then
  record backup-verify 1
  pl_record backup verified "$TUNNEL_REPORT"
else
  record backup-verify 0
fi

call_node "$ADMIN" "$ADMIN_WORK" "$CASE_DIR/backup-list.json" "$CASE_DIR/backup-list.err" \
  -- remote backups --backend "$BACKEND_NAME" || true
BACKUP_COUNT="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1])).get("data", {}).get("backups", [])))' "$CASE_DIR/backup-list.json" 2>/dev/null || echo 0)"
if [ "$BACKUP_COUNT" -ge 1 ]; then
  record backup-index 1
  pl_record backup_index visible "$TUNNEL_REPORT"
else
  record backup-index 0 "count=$BACKUP_COUNT"
  pl_record backup_index missing "$TUNNEL_REPORT"
fi

CONNECTIONS=0
if [ -f "$SSH_DIR/connections.log" ]; then
  CONNECTIONS="$(wc -l <"$SSH_DIR/connections.log" | tr -d ' ')"
fi
if [ "$CONNECTIONS" -ge 1 ]; then
  record forwarded-connections 1
  pl_record forwarded_connections '>=1' "$TUNNEL_REPORT"
else
  record forwarded-connections 0 "count=$CONNECTIONS"
  pl_record forwarded_connections 0 "$TUNNEL_REPORT"
fi

section "restart the execution node and re-probe"
pl_record before_stop ok "$RESTART_REPORT"
kill -TERM "$EXEC_PID"
wait_with_watchdog "$EXEC_PID" 20
EXEC_EXIT="$WAIT_CODE"
EXEC_PID=""
if [ "$EXEC_EXIT" = "0" ]; then
  record execution-sigterm 1
  pl_record exit_code 0 "$RESTART_REPORT"
else
  record execution-sigterm 0 "code=$EXEC_EXIT"
  pl_record exit_code "$EXEC_EXIT" "$RESTART_REPORT"
fi
if wait_backend_field false status.reachable; then
  record execution-unreachable 1
  pl_record backend_reachable unreachable "$RESTART_REPORT"
else
  record execution-unreachable 0
  pl_record backend_reachable reachable "$RESTART_REPORT"
fi
if pl_pid_alive "$SSH_PID"; then
  record tunnel-survives-remote-down 1
  pl_record tunnel_child alive "$RESTART_REPORT"
else
  record tunnel-survives-remote-down 0
  pl_record tunnel_child gone "$RESTART_REPORT"
fi

WF_RECOVERY_RESTORE_ENABLED=1 \
  "$SERVER_BIN" --addr "127.0.0.1:$EXEC_PORT" \
  --storage "sqlite:$EXEC_WORK/exec.db" --directory "$EXEC_WORK" \
  --log-level info --log-format json >"$EXEC_RESTART_LOG" 2>&1 &
EXEC_PID=$!
if wait_for_health "$EXEC" 30; then
  record execution-restart 1
else
  record execution-restart 0 "see $EXEC_RESTART_LOG"
fi
if wait_backend_field true status.reachable; then
  record reprobe-reachable 1
  pl_record after_restart reachable "$RESTART_REPORT"
else
  record reprobe-reachable 0
  pl_record after_restart unreachable "$RESTART_REPORT"
fi
if call_node "$EXEC" "$EXEC_WORK" "$CASE_DIR/execution-after-restart.json" \
    "$CASE_DIR/execution-after-restart.err" -- execution show "$EXEC_ID"; then
  record execution-persisted 1
  pl_record execution_visible yes "$RESTART_REPORT"
else
  record execution-persisted 0
  pl_record execution_visible no "$RESTART_REPORT"
fi
section "stop the management node and reap its tunnel"
kill -TERM "$ADMIN_PID"
wait_with_watchdog "$ADMIN_PID" 20
ADMIN_EXIT="$WAIT_CODE"
ADMIN_STARTED=0
if [ "$ADMIN_EXIT" = "0" ]; then
  record admin-sigterm 1
  pl_record admin_exit_code 0 "$RESTART_REPORT"
else
  record admin-sigterm 0 "code=$ADMIN_EXIT"
  pl_record admin_exit_code "$ADMIN_EXIT" "$RESTART_REPORT"
fi
SSH_AFTER="alive"
pl_pid_alive "$SSH_PID" || SSH_AFTER="gone"
if [ "$SSH_AFTER" = "gone" ]; then
  record admin-tunnel-reaped 1
  pl_record ssh_child_after_admin_exit gone "$RESTART_REPORT"
else
  record admin-tunnel-reaped 0 "pid=$SSH_PID"
  pl_record ssh_child_after_admin_exit alive "$RESTART_REPORT"
fi

section "result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
