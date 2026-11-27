#!/usr/bin/env bash
# Deterministic PTY child for the real child-process tests.
set -u

printf 'PTY_READY\n'
IFS= read -r line
printf 'PTY_ECHO:%s\n' "$line"
exec sleep 3600
