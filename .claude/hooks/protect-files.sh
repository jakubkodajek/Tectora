#!/usr/bin/env bash
# PreToolUse hook for Edit|Write: blocks edits of secrets, lockfiles,
# generated files and DACAD material.
set -euo pipefail
# shellcheck source=.claude/hooks/lib.sh
. "$(dirname "$0")/lib.sh"

file=$(json_field '.tool_input.file_path')
[ -z "$file" ] && exit 0
rel=${file#"${CLAUDE_PROJECT_DIR:-$PWD}/"}

block() {
  echo "Blocked by .claude/hooks/protect-files.sh: $rel - $1" >&2
  exit 2
}

case "$rel" in
  .env|.env.*|*/.env|*/.env.*)
    [[ "$rel" == *.env.example ]] || block "secret file" ;;
  *.pem|*.key|*id_rsa*|*.p12|*.p8)
    block "private key material" ;;
  Cargo.lock|app/package-lock.json|*/package-lock.json)
    block "lockfile; change it with cargo or npm, not by hand" ;;
  docs/tectora.schema.json)
    block "generated; run TECTORA_WRITE_SCHEMA=1 cargo test -p tectora-core --test schema" ;;
  .github/agents/*|.codex/agents/*|.agents/skills/*)
    block "generated; edit .claude/agents or .claude/skills and run python3 scripts/sync_agents.py" ;;
  *.DAD|*.dad|*DACAD*.exe|*.HLP|*.hlp|*.dll)
    block "DACAD material must never enter the repository" ;;
  .git/*)
    block "git internals" ;;
esac

exit 0
