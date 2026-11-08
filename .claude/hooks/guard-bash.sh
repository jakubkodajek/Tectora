#!/usr/bin/env bash
# PreToolUse hook for Bash: blocks destructive, history-rewriting or
# secret-printing commands, and staging DACAD material.
# Exit code 2 blocks the tool call and shows stderr to the agent.
set -euo pipefail
# shellcheck source=.claude/hooks/lib.sh
. "$(dirname "$0")/lib.sh"

command=$(json_field '.tool_input.command')

block() {
  echo "Blocked by .claude/hooks/guard-bash.sh: $1" >&2
  echo "If this is really needed, ask the human to run it." >&2
  exit 2
}

# Normalise whitespace for matching.
cmd=$(printf '%s' "$command" | tr -s '[:space:]' ' ')

case "$cmd" in
  *"rm -rf /"|*"rm -rf / "*|*"rm -rf /*"*|*"rm -rf ~"|*"rm -rf ~/"|*"rm -rf \$HOME"*|*"rm -rf ."|*"rm -rf . "*|*"rm -rf *"*|*"rm -rf .."*)
    block "recursive delete of a root, home or whole working directory" ;;
  *"git push --force"*|*"git push -f "*|*"git push -f"|*"git push"*" +"*)
    block "force push" ;;
  *"git reset --hard origin"*|*"git clean -fdx"*|*"git clean -xfd"*|*"git checkout -- ."*)
    block "discarding all local work" ;;
  *"git commit"*"--no-verify"*|*"git push"*"--no-verify"*)
    block "skipping git hooks" ;;
  *"git add -f"*|*"git add --force"*)
    block "force-adding ignored files (DACAD material is ignored on purpose)" ;;
  *"git add"*".DAD"*|*"git add"*".dad"*|*"git add"*"DACAD"*".exe"*|*"git add"*".HLP"*|*"git add"*".hlp"*)
    block "staging DACAD material; it must never be committed" ;;
  *"curl "*"| sh"*|*"curl "*"| bash"*|*"wget "*"| sh"*|*"wget "*"| bash"*)
    block "piping a remote script into a shell" ;;
  *"chmod -R 777"*|*"mkfs"*|*"dd if="*" of=/dev/"*)
    block "dangerous system command" ;;
  *"cat .env"*|*"printenv"*|*"echo \$"*TOKEN*|*"echo \$"*KEY*|*"echo \$"*SECRET*)
    block "printing secrets" ;;
esac

exit 0
