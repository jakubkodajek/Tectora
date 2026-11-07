#!/usr/bin/env bash
# Shared helpers for the Claude Code hooks. Sourced, not run.

# json_field <expr>: read the hook's JSON input from stdin and print a field.
# Uses jq when present (not installed by default on macOS), else python3.
json_field() {
  local expr=$1
  if command -v jq >/dev/null 2>&1; then
    jq -r "$expr // \"\""
  else
    python3 -c '
import json, sys
data = json.load(sys.stdin)
for key in sys.argv[1].lstrip(".").split("."):
    data = data.get(key) if isinstance(data, dict) else None
print(data if isinstance(data, str) else "")
' "$expr"
  fi
}
