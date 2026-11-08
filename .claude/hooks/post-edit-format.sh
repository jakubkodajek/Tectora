#!/usr/bin/env bash
# PostToolUse hook for Edit|Write: runs rustfmt on an edited Rust file.
# Never blocks; a formatting failure is left for `make lint` to report.
set -uo pipefail
# shellcheck source=.claude/hooks/lib.sh
. "$(dirname "$0")/lib.sh"

file=$(json_field '.tool_input.file_path')
{ [ -z "$file" ] || [ ! -f "$file" ]; } && exit 0

case "$file" in
  *.rs)
    command -v rustfmt >/dev/null 2>&1 && rustfmt --edition 2024 --quiet "$file" ;;
esac

exit 0
