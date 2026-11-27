#!/usr/bin/env bash
# Real process lifecycle run: SIGTERM, supervised SSH tunnel children and
# PTY child convergence. Uses a loopback-only fake ssh so no remote host is
# required. Reports are normalized text candidates for manual review.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=lib/process_lifecycle.sh
source "$SCRIPT_DIR/lib/process_lifecycle.sh"

case "${1:-}" in
  --help|-h)
    echo "usage: run-process-lifecycle.sh"
    exit 0
    ;;
  "") ;;
  *) echo "e2e: unknown argument: $1" >&2; exit 2 ;;
esac

ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$ROOT/e2e/bin"
FIX_DIR="$ROOT/e2e/fixtures/process-lifecycle"
ACTUAL_DIR="$ROOT/e2e/outputs/actual"
CASE_DIR="$ACTUAL_DIR/process-lifecycle"
SERVER_BIN="$BIN_DIR/wf-server"

require_file "$SERVER_BIN"
require_file "$FIX_DIR/bin/ssh"
require_file "$FIX_DIR/pty-worker.sh"
require_file "$FIX_DIR/backend-tunnel.json"
require_file "$FIX_DIR/pty-create.json"
require_cmd curl
require_cmd python3
require_cmd sleep

mkdir -p "$CASE_DIR"
WORK_DIR="$(mktemp -d "$CASE_DIR/work.XXXXXX")"
SSH_DIR="$WORK_DIR/ssh"
mkdir -p "$SSH_DIR"
SERVER_LOG="$CASE_DIR/server.log"
SIGNAL_REPORT="$CASE_DIR/sigterm.report.txt"
TUNNEL_REPORT="$CASE_DIR/ssh-tunnel.report.txt"
PTY_REPORT="$CASE_DIR/pty-liveness.report.txt"
: >"$SERVER_LOG"
: >"$SIGNAL_REPORT"
: >"$TUNNEL_REPORT"
: >"$PTY_REPORT"

PASS=0
FAIL=0
SERVER_PID=""
SERVER_STARTED=0

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
  if [ "$SERVER_STARTED" -eq 1 ] && pl_pid_alive "$SERVER_PID"; then
    kill "$SERVER_PID" 2>/dev/null || true
    pl_wait_pid_gone "$SERVER_PID" 30 || kill -9 "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

wait_for_live_pid() {
  local file="$1" previous="${2:-}" candidate i
  for ((i = 0; i < 50; i++)); do
    if [ -f "$file" ]; then
      candidate="$(cat "$file")"
      if [ -n "$candidate" ] && [ "$candidate" != "$previous" ] && pl_pid_alive "$candidate"; then
        printf '%s' "$candidate"
        return 0
      fi
    fi
    sleep 0.1
  done
  return 1
}

render_backend_body() {
  python3 - "$FIX_DIR/backend-tunnel.json" "$1" "$2" "$3" "$4" <<'PY'
import json
import sys

template, name, local_port, server_port, directory = sys.argv[1:6]
body = json.load(open(template))
body["name"] = name
body["entry"]["local_port"] = int(local_port)
body["entry"]["server_port"] = int(server_port)
body["entry"]["backup_directory"] = directory
print(json.dumps(body))
PY
}

render_pty_body() {
  python3 - "$FIX_DIR/pty-create.json" "$1" "$2" <<'PY'
import json
import sys

template, command, directory = sys.argv[1:4]
body = json.load(open(template))
body["command"] = command
body["cwd"] = directory
print(json.dumps(body))
PY
}

section "start server with fake ssh on PATH"
pl_pick_port
SERVER_PORT="$PL_PORT"
pl_pick_port
TUNNEL_PORT="$PL_PORT"
pl_pick_port
REMOTE_PORT="$PL_PORT"
pl_pick_port
EXIT_TUNNEL_PORT="$PL_PORT"
BASE="http://127.0.0.1:$SERVER_PORT"

PATH="$FIX_DIR/bin:$PATH" \
  WF_BACKEND_ENABLED=1 \
  WF_BACKEND_PROBE_INTERVAL_SECS=1 \
  WF_BACKEND_PROBE_TIMEOUT_SECS=1 \
  WF_BACKEND_TUNNEL_STOP_GRACE_SECS=0 \
  WF_BACKEND_BACKUP_RECONCILE_INTERVAL_SECS=1 \
  WF_E2E_SSH_FAKE_DIR="$SSH_DIR" \
  WF_E2E_SSH_FAKE_EXIT_PORTS="$EXIT_TUNNEL_PORT" \
  "$SERVER_BIN" --addr "127.0.0.1:$SERVER_PORT" --storage memory \
  --directory "$WORK_DIR" --log-level info --log-format json >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
SERVER_STARTED=1

if wait_for_health "$BASE" 30; then
  record listen-ready 1
  pl_record listen ok "$SIGNAL_REPORT"
else
  record listen-ready 0 "see $SERVER_LOG"
  pl_die "server did not become ready"
fi

CODE="$(pl_curl_json GET "$BASE/api/v1/server/status" "$CASE_DIR/server-status.json")"
STATUS_PID="$(pl_json_field "$CASE_DIR/server-status.json" data.pid)"
if [ "$CODE" = "200" ] && [ "$STATUS_PID" = "$SERVER_PID" ]; then
  record status-pid 1
  pl_record status_pid matched "$SIGNAL_REPORT"
else
  record status-pid 0 "status=$CODE pid=$STATUS_PID expected=$SERVER_PID"
  pl_die "server status did not identify the launched process"
fi

section "start a supervised tunnel"
BODY="$(render_backend_body e2e-tunnel "$TUNNEL_PORT" "$REMOTE_PORT" "$WORK_DIR")"
CODE="$(pl_curl_json POST "$BASE/api/v1/system/backends" "$CASE_DIR/backend-register.json" "$BODY")"
if [ "$CODE" = "200" ]; then
  record register-backend 1
else
  record register-backend 0 "status=$CODE"
  pl_die "backend registration failed"
fi

CODE="$(pl_curl_json POST "$BASE/api/v1/system/backends/e2e-tunnel/tunnel/start" "$CASE_DIR/tunnel-start.json")"
TUNNEL_PID="$(pl_json_field "$CASE_DIR/tunnel-start.json" data.pid)"
if [ "$CODE" = "200" ] && [ -n "$TUNNEL_PID" ]; then
  record tunnel-start 1
else
  record tunnel-start 0 "status=$CODE pid=$TUNNEL_PID"
  pl_die "tunnel start failed"
fi
pl_project_argv "$CASE_DIR/tunnel-start.json" "$CASE_DIR/argv.projection"
cat "$CASE_DIR/argv.projection" >>"$TUNNEL_REPORT"

SSH_PID="$(wait_for_live_pid "$SSH_DIR/ssh-$TUNNEL_PORT.pid" "")" || pl_die "fake ssh pid file missing"
if [ "$SSH_PID" = "$TUNNEL_PID" ] && pl_pid_alive "$SSH_PID"; then
  record tunnel-child 1
  pl_record tunnel_state running "$TUNNEL_REPORT"
  pl_record pid_file alive "$TUNNEL_REPORT"
else
  record tunnel-child 0 "view=$TUNNEL_PID marker=$SSH_PID"
  pl_die "tunnel child did not match the supervised pid"
fi

section "create a persistent PTY child"
PTY_BODY="$(render_pty_body "$FIX_DIR/pty-worker.sh" "$WORK_DIR")"
CODE="$(pl_curl_json POST "$BASE/api/v1/pty" "$CASE_DIR/pty-create.json" "$PTY_BODY" "x-wf-directory: $WORK_DIR")"
PTY_ID="$(pl_json_field "$CASE_DIR/pty-create.json" data.id)"
PTY_PID="$(pl_json_field "$CASE_DIR/pty-create.json" data.pid)"
if [ "$CODE" = "200" ] && [ -n "$PTY_ID" ] && [ -n "$PTY_PID" ] && pl_pid_alive "$PTY_PID"; then
  record pty-create 1
  pl_record create ok "$PTY_REPORT"
  pl_record status running "$PTY_REPORT"
  pl_record child_alive yes "$PTY_REPORT"
else
  record pty-create 0 "status=$CODE id=$PTY_ID pid=$PTY_PID"
  pl_die "PTY child did not start"
fi

section "stop one tunnel explicitly"
CODE="$(pl_curl_json POST "$BASE/api/v1/system/backends/e2e-tunnel/tunnel/stop" "$CASE_DIR/tunnel-stop.json")"
if [ "$CODE" = "200" ] && [ "$(pl_json_field "$CASE_DIR/tunnel-stop.json" data.stopped)" = "true" ]; then
  record tunnel-stop 1
  pl_record stop stopped=true "$TUNNEL_REPORT"
else
  record tunnel-stop 0 "status=$CODE"
fi
if pl_wait_pid_gone "$SSH_PID" 30; then
  record tunnel-child-reaped 1
  pl_record child_after_stop gone "$TUNNEL_REPORT"
else
  record tunnel-child-reaped 0 "pid=$SSH_PID still alive"
  pl_record child_after_stop alive "$TUNNEL_REPORT"
fi

section "observe an abnormal tunnel exit"
BODY="$(render_backend_body e2e-tunnel-exit "$EXIT_TUNNEL_PORT" "$REMOTE_PORT" "$WORK_DIR")"
CODE="$(pl_curl_json POST "$BASE/api/v1/system/backends" "$CASE_DIR/backend-exit-register.json" "$BODY")"
[ "$CODE" = "200" ] || pl_die "exit-later backend registration failed: $CODE"
CODE="$(pl_curl_json POST "$BASE/api/v1/system/backends/e2e-tunnel-exit/tunnel/start" "$CASE_DIR/tunnel-exit-start.json")"
[ "$CODE" = "200" ] || pl_die "exit-later tunnel start failed: $CODE"

INTERRUPTED=""
for _ in $(seq 1 40); do
  CODE="$(pl_curl_json GET "$BASE/api/v1/system/backends" "$CASE_DIR/backends-after-exit.json")"
  [ "$CODE" = "200" ] || pl_die "backend list failed: $CODE"
  INTERRUPTED="$(pl_backend_field "$CASE_DIR/backends-after-exit.json" e2e-tunnel-exit tunnel.interrupted_reason)"
  [ -n "$INTERRUPTED" ] && break
  sleep 0.5
done
if [ -n "$INTERRUPTED" ] && [[ "$INTERRUPTED" == tunnel\ exited* ]]; then
  record tunnel-interrupted 1
  NORMALIZED="$(printf '%s' "$INTERRUPTED" | sed -E 's/exit status: [0-9]+/exit status: <CODE>/')"
  pl_record abnormal_interrupted "$NORMALIZED" "$TUNNEL_REPORT"
else
  record tunnel-interrupted 0 "reason=$INTERRUPTED"
  pl_record abnormal_interrupted missing "$TUNNEL_REPORT"
fi

section "restart the forward before process shutdown"
CODE="$(pl_curl_json POST "$BASE/api/v1/system/backends/e2e-tunnel/tunnel/start" "$CASE_DIR/tunnel-restart.json")"
[ "$CODE" = "200" ] || pl_die "tunnel restart failed: $CODE"
SSH_PID="$(wait_for_live_pid "$SSH_DIR/ssh-$TUNNEL_PORT.pid" "$SSH_PID")" || pl_die "restarted tunnel child missing"

section "send SIGTERM to the server process"
kill -TERM "$SERVER_PID"
set +e
wait "$SERVER_PID"
EXIT_CODE=$?
set -e
SERVER_STARTED=0

PROCESS_AFTER="alive"
pl_pid_alive "$SERVER_PID" || PROCESS_AFTER="gone"
LISTENER_AFTER="open"
if curl -fs --max-time 2 -o /dev/null "$BASE/health" 2>/dev/null; then
  :
else
  LISTENER_AFTER="closed"
fi
SSH_AFTER="alive"
pl_pid_alive "$SSH_PID" || SSH_AFTER="gone"
PTY_AFTER="alive"
pl_pid_alive "$PTY_PID" || PTY_AFTER="gone"

record sigterm-exit-code "$([ "$EXIT_CODE" = "0" ] && echo 1 || echo 0)" "code=$EXIT_CODE"
record sigterm-process-gone "$([ "$PROCESS_AFTER" = "gone" ] && echo 1 || echo 0)"
record sigterm-listener-closed "$([ "$LISTENER_AFTER" = "closed" ] && echo 1 || echo 0)"
record sigterm-ssh-child-gone "$([ "$SSH_AFTER" = "gone" ] && echo 1 || echo 0)"
record sigterm-pty-child-gone "$([ "$PTY_AFTER" = "gone" ] && echo 1 || echo 0)"

pl_record signal SIGTERM "$SIGNAL_REPORT"
pl_record exit_code "$EXIT_CODE" "$SIGNAL_REPORT"
pl_record listener_after "$LISTENER_AFTER" "$SIGNAL_REPORT"
pl_record process_after "$PROCESS_AFTER" "$SIGNAL_REPORT"
pl_record ssh_child_alive "$SSH_AFTER" "$SIGNAL_REPORT"
pl_record pty_child_alive "$PTY_AFTER" "$SIGNAL_REPORT"
pl_record child_after_shutdown "$PTY_AFTER" "$PTY_REPORT"
if [ "$PTY_AFTER" = "gone" ]; then
  pl_record session_after_shutdown absent "$PTY_REPORT"
else
  pl_record session_after_shutdown present "$PTY_REPORT"
fi

pl_project_log "$SERVER_LOG" "$CASE_DIR/log.projection"
for key in log_signal log_pty; do
  VALUE="$(grep "^$key=" "$CASE_DIR/log.projection" 2>/dev/null | head -1 || true)"
  [ -n "$VALUE" ] || VALUE="$key=missing"
  printf '%s\n' "$VALUE" >>"$SIGNAL_REPORT"
  case "$VALUE" in
    "$key=missing") record "$key" 0 ;;
    *) record "$key" 1 ;;
  esac
done

section "result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
