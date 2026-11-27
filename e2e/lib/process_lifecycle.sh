#!/usr/bin/env bash
# Helpers for the process lifecycle end-to-end runs. Source common.sh first.
# This file performs no actions when sourced.

set -u

pl_die() {
  echo "e2e: $*" >&2
  exit 1
}

pl_pid_alive() {
  kill -0 "$1" 2>/dev/null
}

pl_wait_pid_gone() {
  local pid="$1" limit="${2:-100}" i
  for ((i = 0; i < limit; i++)); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
  done
  return 1
}

pl_pick_port() {
  local candidate
  while :; do
    candidate="$(free_port)"
    case " ${PL_USED_PORTS:-} " in
      *" $candidate "*) continue ;;
      *) break ;;
    esac
  done
  PL_USED_PORTS="${PL_USED_PORTS:-} $candidate"
  PL_PORT="$candidate"
}

pl_json_field() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

value = json.load(open(sys.argv[1]))
for key in sys.argv[2].split("."):
    if isinstance(value, dict):
        value = value.get(key)
    elif isinstance(value, list) and key.isdigit():
        index = int(key)
        value = value[index] if index < len(value) else None
    else:
        value = None
    if value is None:
        break
if value is None:
    print("")
elif isinstance(value, str):
    print(value)
else:
    print(json.dumps(value, separators=(",", ":")))
PY
}

pl_backend_field() {
  python3 - "$1" "$2" "$3" <<'PY'
import json
import sys

rows = json.load(open(sys.argv[1])).get("data", {}).get("backends", [])
selected = next((row for row in rows if row.get("name") == sys.argv[2]), None)
value = selected
for key in sys.argv[3].split("."):
    if isinstance(value, dict):
        value = value.get(key)
    else:
        value = None
    if value is None:
        break
if value is None:
    print("")
elif isinstance(value, str):
    print(value)
else:
    print(json.dumps(value, separators=(",", ":")))
PY
}

pl_curl_json() {
  local method="$1" url="$2" out="$3" body="${4:-}" header="${5:-}"
  local -a args=(-sS -o "$out" -w '%{http_code}' -X "$method" "$url")
  if [ -n "$header" ]; then
    args+=(-H "$header")
  fi
  if [ -n "$body" ]; then
    args+=(-H 'content-type: application/json' --data "$body")
  fi
  curl "${args[@]}"
}

pl_record() {
  printf '%s=%s\n' "$1" "$2" >>"$3"
}

pl_project_log() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

seen = set()
lines = []
with open(sys.argv[1], encoding="utf-8", errors="replace") as source:
    for raw in source:
        try:
            entry = json.loads(raw)
        except ValueError:
            continue
        fields = entry.get("fields") if isinstance(entry.get("fields"), dict) else {}
        message = fields.get("message") or entry.get("message") or entry.get("msg") or ""
        if not isinstance(message, str):
            continue
        if "SIGTERM received" in message and "log_signal" not in seen:
            seen.add("log_signal")
            lines.append("log_signal=" + message)
        if "stopped PTY sessions" in message and "log_pty" not in seen:
            seen.add("log_pty")
            lines.append("log_pty=" + message)
with open(sys.argv[2], "w", encoding="utf-8") as target:
    target.write("\n".join(lines) + ("\n" if lines else ""))
PY
}

pl_project_argv() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

parts = list(json.load(open(sys.argv[1])).get("data", {}).get("argv", []))
for index, part in enumerate(parts):
    if part == "-L" and index + 1 < len(parts):
        pieces = parts[index + 1].split(":")
        if len(pieces) >= 3:
            pieces[0] = "<LOCAL>"
            pieces[-1] = "<REMOTE>"
            parts[index + 1] = ":".join(pieces)
    elif part.startswith("ControlPath="):
        parts[index] = "ControlPath=<CONTROL_PATH>"
    elif part == "-p" and index + 1 < len(parts):
        parts[index + 1] = "<SSH_PORT>"
if parts:
    parts[-1] = "<USER>@<HOST>"
with open(sys.argv[2], "w", encoding="utf-8") as target:
    target.write("argv=" + " ".join(parts) + "\n")
PY
}

pl_promote_report() {
  local source="$1" destination="$2"
  [ -f "$source" ] || pl_die "missing report candidate: $source"
  mkdir -p "$(dirname "$destination")"
  cp "$source" "$destination"
}
