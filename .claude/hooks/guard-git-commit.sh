#!/bin/sh
# Claude Code PreToolUse hook for Bash: blocks `git commit` until the repository's git hooks are
# installed, and blocks attempts to skip them. See docs/architecture/tooling.md "Commit gate".
# Exit code 2 blocks the tool call and shows stderr to the agent.

input=$(cat)

# Only `git commit` commands are of interest.
printf '%s' "$input" | grep -Eq 'git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+commit' || exit 0

if printf '%s' "$input" | grep -Eq -- '--no-verify|core\.hooksPath|[[:space:]]-[A-Za-z]*n[A-Za-z]*([[:space:]]|")'; then
    echo "Blocked: never bypass the pre-commit hook (--no-verify, -n, core.hooksPath). Fix what it reports instead." >&2
    exit 2
fi

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
hooks_dir=$(git rev-parse --git-path hooks 2>/dev/null) || exit 0
if ! grep -q 'cargo xtask precommit' "$hooks_dir/pre-commit" 2>/dev/null; then
    echo "Blocked: git hooks are not installed. Run \`cargo xtask install-hooks\` first." >&2
    exit 2
fi
exit 0
