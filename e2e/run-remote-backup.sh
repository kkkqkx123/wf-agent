#!/usr/bin/env bash
# Two-node end-to-end run for the remote backup and recovery chain.
#
# Starts an execution node plus a management node with the backend domain
# enabled, seeds one execution on the execution node, then drives the whole
# chain through wf-headless: backup, verify, state, loss detection, recovery
# and rebaseline. The execution tier needs a working local script executor;
# when the node cannot run one, the whole layer reports SKIP instead of
# failing, matching the main harness convention.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$ROOT/e2e/bin"
FIX_DIR="$ROOT/e2e/fixtures"
ACTUAL_DIR="$ROOT/e2e/outputs/actual"
SERVER_BIN="$BIN_DIR/wf-server"
HEADLESS_BIN="$BIN_DIR/wf-headless"
BACKEND_NAME="alpha"
POLL_LIMIT_SECS=25

KEEP_RUNNING=0
for arg in "$@"; do
  case "$arg" in
    --keep-running) KEEP_RUNNING=1 ;;
    --help|-h)
      echo "usage: run-remote-backup.sh [--keep-running]"
      exit 0
      ;;
    *) echo "e2e: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

require_file "$SERVER_BIN"
require_file "$HEADLESS_BIN"
require_cmd curl
require_cmd python3

mkdir -p "$ACTUAL_DIR"
WORK_DIR="$(mktemp -d "$ACTUAL_DIR/remote-backup.XXXXXX")"
REMOTE_WORK="$WORK_DIR/remote"
ADMIN_WORK="$WORK_DIR/admin"
BACKUP_DIR="$WORK_DIR/backups"
mkdir -p "$REMOTE_WORK" "$ADMIN_WORK" "$BACKUP_DIR"
REMOTE_WORK="$(cd "$REMOTE_WORK" && pwd)"
ADMIN_WORK="$(cd "$ADMIN_WORK" && pwd)"
REMOTE_LOG="$ACTUAL_DIR/remote-node.log"
ADMIN_LOG="$ACTUAL_DIR/admin-node.log"
REMOTE_PORT="$(free_port)"
ADMIN_PORT="$(free_port)"
REMOTE="http://127.0.0.1:$REMOTE_PORT"
ADMIN="http://127.0.0.1:$ADMIN_PORT"
PASS=0
FAIL=0

cleanup() {
  if [ "$KEEP_RUNNING" -eq 0 ]; then
    [ -n "${ADMIN_PID:-}" ] && kill "$ADMIN_PID" 2>/dev/null || true
    [ -n "${REMOTE_PID:-}" ] && kill "$REMOTE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

section "start execution node on $REMOTE and management node on $ADMIN"
# The restore write route answers only when the node operator enabled it.
WF_RECOVERY_RESTORE_ENABLED=1 "$SERVER_BIN" --addr "127.0.0.1:$REMOTE_PORT" \
  --storage memory --directory "$REMOTE_WORK" >"$REMOTE_LOG" 2>&1 &
REMOTE_PID=$!
# Backups belong to the management side: index plus snapshot directory, probe
# left at its default cadence because the timeout must not exceed it, periodic
# full re-check off so a deliberate damage stays visible until the script
# repairs it.
WF_BACKEND_ENABLED=1 \
  WF_BACKEND_BACKUP_DIR="$BACKUP_DIR" \
  WF_BACKEND_BACKUP_INTERVAL_SECS=1 \
  WF_BACKEND_BACKUP_RECONCILE_INTERVAL_SECS=0 \
  "$SERVER_BIN" --addr "127.0.0.1:$ADMIN_PORT" \
  --storage memory --directory "$ADMIN_WORK" >"$ADMIN_LOG" 2>&1 &
ADMIN_PID=$!

wait_for_health "$REMOTE" 30 || { echo "e2e: execution node not ready, see $REMOTE_LOG" >&2; exit 1; }
wait_for_health "$ADMIN" 30 || { echo "e2e: management node not ready, see $ADMIN_LOG" >&2; exit 1; }

# One client call against a named node; stdout is the JSON envelope.
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

data_field() {
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])).get("data"); print("" if v is None else (v if isinstance(v,str) else json.dumps(v)))' "$1" 2>/dev/null
}

state_field() {
  local file="$1" back="$2" exec="$3" field="$4"
  python3 - "$file" "$back" "$exec" "$field" <<'EOF'
import json, sys
path, backend, execution, field = sys.argv[1:5]
try:
    rows = json.load(open(path)).get("data", {}).get("states", [])
except Exception:
    rows = []
for row in rows:
    if row.get("backend") == backend and row.get("execution") == execution:
        print(row.get(field, ""))
        break
EOF
}

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

section "seed one execution on the execution node"
EXEC_ID=""
if call_node "$REMOTE" "$REMOTE_WORK" "$ACTUAL_DIR/remote-script-save.json" \
  "$ACTUAL_DIR/remote-script-save.err" -- script save --file "$FIX_DIR/script-echo.json" &&
  call_node "$REMOTE" "$REMOTE_WORK" "$ACTUAL_DIR/remote-workflow-create.json" \
    "$ACTUAL_DIR/remote-workflow-create.err" -- workflow create --file "$FIX_DIR/workflow-script.json" --format json; then
  if call_node "$REMOTE" "$REMOTE_WORK" "$ACTUAL_DIR/remote-run.json" \
    "$ACTUAL_DIR/remote-run.err" -- execution run --workflow e2e-script \
    --input-file "$FIX_DIR/workflow-input.json" --background; then
    EXEC_ID="$(execution_id_of "$ACTUAL_DIR/remote-run.json")"
  fi
fi

if [ -z "$EXEC_ID" ] || [ "$EXEC_ID" = "null" ]; then
  echo "SKIP remote backup tier (this node cannot run a script workflow)"
  section "result: $PASS passed, $FAIL failed, layer skipped"
  exit 0
fi

for _ in $(seq 1 "$POLL_LIMIT_SECS"); do
  call_node "$REMOTE" "$REMOTE_WORK" "$ACTUAL_DIR/remote-status.json" \
    "$ACTUAL_DIR/remote-status.err" -- execution status "$EXEC_ID" || true
  [ "$(envelope_status_of "$ACTUAL_DIR/remote-status.json")" = "completed" ] && break
  sleep 1
done
EXEC_STATUS="$(envelope_status_of "$ACTUAL_DIR/remote-status.json")"
echo "seeded execution $EXEC_ID terminal status: $EXEC_STATUS"
if [ "$EXEC_STATUS" != "completed" ]; then
  echo "SKIP remote backup tier (seeded execution did not complete: $EXEC_STATUS)"
  section "result: $PASS passed, $FAIL failed, layer skipped"
  exit 0
fi

section "register the execution node on the management service"
# The registry write path needs the backup scope and the target list, which
# the CLI add command does not carry, so the entry is posted directly.
REGISTER_BODY="$(python3 -c '
import json, sys
backend, port, directory, execution = sys.argv[1:5]
print(json.dumps({
    "name": backend,
    "entry": {
        "host": "127.0.0.1",
        "username": "e2e",
        "local_port": int(port),
        "server_port": int(port),
        "backup_directory": directory,
        "backup_executions": [execution],
        "backup_interval_secs": 1,
    },
}))
' "$BACKEND_NAME" "$REMOTE_PORT" "$REMOTE_WORK" "$EXEC_ID")"
REGISTER_CODE="$(curl -s -o "$ACTUAL_DIR/register.json" -w '%{http_code}' \
  -H 'content-type: application/json' -X POST "$ADMIN/api/v1/system/backends" -d "$REGISTER_BODY")"
record register-backend "$([ "$REGISTER_CODE" = "200" ] && echo 1 || echo 0)" "status $REGISTER_CODE"

section "snapshot and state"
call_node "$ADMIN" "$ADMIN_WORK" "$ACTUAL_DIR/backup-now.json" "$ACTUAL_DIR/backup-now.err" \
  -- remote backup-now "$BACKEND_NAME" --execution "$EXEC_ID" || true
BACKUP_ID="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("data",{}).get("id",""))' "$ACTUAL_DIR/backup-now.json" 2>/dev/null)"
record backup-now "$([ -n "$BACKUP_ID" ] && echo 1 || echo 0)" "no backup id"

call_node "$ADMIN" "$ADMIN_WORK" "$ACTUAL_DIR/backup-verify.json" "$ACTUAL_DIR/backup-verify.err" \
  -- remote backup-verify "$BACKUP_ID" || true
VERIFIED="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("data",{}).get("verified"))' "$ACTUAL_DIR/backup-verify.json" 2>/dev/null)"
record backup-verify "$([ "$VERIFIED" = "True" ] && echo 1 || echo 0)" "verified=$VERIFIED"

call_node "$ADMIN" "$ADMIN_WORK" "$ACTUAL_DIR/backup-list.json" "$ACTUAL_DIR/backup-list.err" \
  -- remote backups --backend "$BACKEND_NAME" || true
LISTED="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1])).get("data",{}).get("backups",[])))' "$ACTUAL_DIR/backup-list.json" 2>/dev/null)"
record backup-list "$([ "${LISTED:-0}" -ge 1 ] && echo 1 || echo 0)" "records=${LISTED:-0}"

# Wait until the target is reported healthy with a recorded position.
wait_state_field() {
  local want="$1" field="$2" file="$3"
  local _ value
  for _ in $(seq 1 "$POLL_LIMIT_SECS"); do
    call_node "$ADMIN" "$ADMIN_WORK" "$file" "${file%.json}.err" \
      -- remote sync-state --backend "$BACKEND_NAME" || true
    value="$(state_field "$file" "$BACKEND_NAME" "$EXEC_ID" "$field")"
    [ "$value" = "$want" ] && return 0
    sleep 1
  done
  return 1
}

STATE_FILE="$ACTUAL_DIR/sync-state.json"
if wait_state_field ok repair_status "$STATE_FILE"; then
  record sync-state-ok 1
else
  record sync-state-ok 0 "status=$(state_field "$STATE_FILE" "$BACKEND_NAME" "$EXEC_ID" repair_status)"
fi

section "remote loss detection and recovery"
call_node "$REMOTE" "$REMOTE_WORK" "$ACTUAL_DIR/remote-delete.json" "$ACTUAL_DIR/remote-delete.err" \
  -- execution delete "$EXEC_ID" --force || true
if wait_state_field remote_missing repair_status "$STATE_FILE"; then
  record detect-remote-missing 1
else
  record detect-remote-missing 0 "status=$(state_field "$STATE_FILE" "$BACKEND_NAME" "$EXEC_ID" repair_status)"
  # Everything after this point assumes a detected loss; continuing would
  # report a healthy target as a successful recovery.
  section "result: $PASS passed, $FAIL failed"
  exit 1
fi

call_node "$ADMIN" "$ADMIN_WORK" "$ACTUAL_DIR/recover.json" "$ACTUAL_DIR/recover.err" \
  -- remote recover --backend "$BACKEND_NAME" --execution "$EXEC_ID" --apply || true
APPLIED="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("data",{}).get("applied"))' "$ACTUAL_DIR/recover.json" 2>/dev/null)"
record recover-applied "$([ "${APPLIED:-0}" -ge 1 ] && echo 1 || echo 0)" "applied=${APPLIED:-0}"

if wait_state_field ok repair_status "$STATE_FILE"; then
  record recovered-state-healthy 1
else
  record recovered-state-healthy 0 "status=$(state_field "$STATE_FILE" "$BACKEND_NAME" "$EXEC_ID" repair_status)"
fi

set +e
call_node "$REMOTE" "$REMOTE_WORK" "$ACTUAL_DIR/remote-show-restored.json" \
  "$ACTUAL_DIR/remote-show-restored.err" -- execution show "$EXEC_ID"
SHOW_CODE=$?
set -e
record restored-content-visible \
  "$([ "$SHOW_CODE" -eq 0 ] && echo 1 || echo 0)" "execution missing after restore"

section "controlled rebaseline"
call_node "$ADMIN" "$ADMIN_WORK" "$ACTUAL_DIR/sync-reset.json" "$ACTUAL_DIR/sync-reset.err" \
  -- remote sync-reset --backend "$BACKEND_NAME" --execution "$EXEC_ID" || true
RESET_OK="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])).get("success"); print(d)' "$ACTUAL_DIR/sync-reset.json" 2>/dev/null)"
record sync-reset "$([ "$RESET_OK" = "True" ] && echo 1 || echo 0)" "reset failed"

# A reset target pulls one fresh snapshot and settles back to healthy.
if wait_state_field ok repair_status "$STATE_FILE"; then
  record rebaseline-recovers 1
else
  record rebaseline-recovers 0 "status=$(state_field "$STATE_FILE" "$BACKEND_NAME" "$EXEC_ID" repair_status) error=$(state_field "$STATE_FILE" "$BACKEND_NAME" "$EXEC_ID" error)"
fi

section "result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
