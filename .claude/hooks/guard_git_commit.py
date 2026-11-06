#!/usr/bin/env python3
"""Claude Code PreToolUse hook for Bash: guards `git commit`.

Blocks (exit code 2, reason on stderr) when a command would:
  - run `git commit` before the repository's pre-commit hook is installed;
  - skip the hook: `git commit --no-verify` or `-n`;
  - redirect hooks: `git -c core.hooksPath=...`, or `git config ... core.hooksPath`.

Only real commands are inspected: the command line is tokenised with shell quoting rules and
split on control operators, and heredoc bodies are dropped, so `git commit` mentioned inside a
quoted string, a commit message, or a PR body is ignored. `sh -c` / `bash -c` / `zsh -c` strings
are inspected recursively.

See docs/architecture/tooling.md "Commit gate". Standard library only; Python 3.9+.
Run `python3 guard_git_commit.py --self-test` to check its behaviour.
"""

import json
import os
import re
import shlex
import subprocess
import sys

HOOK_MARKER = "cargo xtask precommit"

# `git` options before the subcommand that consume the next token as their value.
GIT_OPTIONS_WITH_VALUE = {"-c", "-C", "--git-dir", "--work-tree", "--namespace", "--config-env"}

# `git commit` short options that consume the rest of their cluster, or the next token.
COMMIT_SHORT_WITH_VALUE = set("mFCctS")

# `git commit` long options that consume the next token when written without `=`.
COMMIT_LONG_WITH_VALUE = {
    "--message", "--file", "--reuse-message", "--reedit-message", "--fixup", "--squash",
    "--author", "--date", "--template", "--cleanup", "--trailer", "--pathspec-from-file",
}

SHELLS = {"sh", "bash", "zsh", "dash"}


class Block(Exception):
    """The command must not run; the message says why."""


def strip_heredocs(command):
    """Removes heredoc bodies, keeping the line that introduces each one."""
    lines = command.split("\n")
    out = []
    pending = []
    for line in lines:
        if pending:
            if line.strip() == pending[0]:
                pending.pop(0)
            continue
        out.append(line)
        for match in re.finditer(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1", line):
            pending.append(match.group(2))
    return "\n".join(out)


def simple_commands(command):
    """Yields the token list of each simple command, honouring quotes."""
    # A newline separates commands like `;`. Inside quotes the `;` stays literal text.
    text = strip_heredocs(command).replace("\n", ";")
    lexer = shlex.shlex(text, posix=True, punctuation_chars=";&|()")
    lexer.whitespace_split = True
    lexer.commenters = "#"
    current = []
    try:
        tokens = list(lexer)
    except ValueError:
        # Unbalanced quotes: fall back to a plain split so obvious commits are still caught.
        tokens = text.split()
    for token in tokens:
        if set(token) <= set(";&|()"):
            if current:
                yield current
            current = []
        else:
            current.append(token)
    if current:
        yield current


def check_git(args):
    """Checks one `git ...` invocation (args after `git`). Returns True if it is a commit."""
    index = 0
    while index < len(args) and args[index].startswith("-"):
        option = args[index]
        name, has_inline, value = option.partition("=")
        # `-c`/`-C` always take the next token; long options take it unless written `--x=value`.
        if option in ("-c", "-C") or (name in GIT_OPTIONS_WITH_VALUE and not has_inline):
            value = args[index + 1] if index + 1 < len(args) else ""
            index += 1
        if name in ("-c", "--config-env") and value.lower().startswith("core.hookspath"):
            raise Block("git -c core.hooksPath redirects the hooks")
        index += 1
    if index >= len(args):
        return False
    subcommand, rest = args[index], args[index + 1:]
    if subcommand == "config" and any(a.lower() == "core.hookspath" for a in rest):
        raise Block("changing core.hooksPath disables the pre-commit hook")
    if subcommand != "commit":
        return False
    skip_next = False
    for arg in rest:
        if skip_next:
            skip_next = False
            continue
        if arg == "--":
            break
        if arg == "--no-verify":
            raise Block("git commit --no-verify skips the pre-commit hook")
        if arg.startswith("--"):
            if arg in COMMIT_LONG_WITH_VALUE:
                skip_next = True
            continue
        if arg.startswith("-") and len(arg) > 1:
            for position, flag in enumerate(arg[1:]):
                if flag == "n":
                    raise Block("git commit -n skips the pre-commit hook")
                if flag in COMMIT_SHORT_WITH_VALUE:
                    skip_next = position == len(arg) - 2
                    break
    return True


def commits_in(command, depth=0):
    """Returns whether `command` runs `git commit`; raises Block for a bypass."""
    found = False
    for tokens in simple_commands(command):
        while tokens and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", tokens[0]):
            tokens = tokens[1:]
        while tokens and tokens[0] in ("env", "command", "exec", "time", "nohup"):
            tokens = tokens[1:]
        if not tokens:
            continue
        program = os.path.basename(tokens[0])
        if program == "git":
            found = check_git(tokens[1:]) or found
        elif program in SHELLS and depth < 3 and "-c" in tokens[1:]:
            script_index = tokens.index("-c") + 1
            if script_index < len(tokens):
                found = commits_in(tokens[script_index], depth + 1) or found
    return found


def hooks_installed(project_dir):
    try:
        hooks = subprocess.run(
            ["git", "rev-parse", "--git-path", "hooks"],
            cwd=project_dir, capture_output=True, text=True, check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return True  # Not a git checkout: nothing to guard.
    path = os.path.join(project_dir, hooks, "pre-commit")
    try:
        with open(path, encoding="utf-8") as hook:
            return HOOK_MARKER in hook.read()
    except OSError:
        return False


def verdict(command, project_dir):
    """Returns None to allow, or the reason to block."""
    try:
        if commits_in(command) and not hooks_installed(project_dir):
            return "git hooks are not installed. Run `cargo xtask install-hooks` first."
    except Block as block:
        return f"never bypass the pre-commit hook ({block}). Fix what it reports instead."
    return None


CASES = [
    # (command, expected to run a commit, expected to be blocked as a bypass)
    ('git commit -m "x"', True, False),
    ("git commit -am x", True, False),
    ("git commit --amend --no-edit", True, False),
    ('git commit -m "use -n to skip"', True, False),
    ("git commit -mn", True, False),
    ("git commit -F msg.txt -- -n", True, False),
    ("cd repo && . ~/.profile && git add -A && git commit -q -F - <<'EOF'\nmsg --no-verify -n\nEOF", True, False),
    ("git commit --no-verify -m x", True, True),
    ("git commit -nm x", True, True),
    ("git commit -an", True, True),
    ("git -c core.hooksPath=/dev/null commit -m x", True, True),
    ("git config core.hooksPath /dev/null", False, True),
    ("bash -c 'git commit --no-verify'", True, True),
    ("FOO=1 git -C sub commit -q", True, False),
    ("git status && grep -n x file", False, False),
    ("gh pr create --body 'run git commit --no-verify'", False, False),
    ("echo git commit -n", False, False),
    ("ls -n", False, False),
    ("git status\ngit commit --no-verify", True, True),
    ("git log -1\necho done", False, False),
    ("git commit -m 'line one\nline two -n'", True, False),
    ("git --git-dir=.git commit -q", True, False),
    ("(cd sub; git commit -n)", True, True),
    ("gh pr create --body-file - <<'EOF'\ngit commit --no-verify\nEOF", False, False),
]


def self_test():
    failures = 0
    for command, commits, blocked in CASES:
        try:
            got_commit, got_block = commits_in(command), False
        except Block:
            got_commit, got_block = commits, True
        if (got_commit, got_block) != (commits, blocked):
            failures += 1
            print(f"FAIL {command!r}: commit={got_commit} block={got_block}, "
                  f"expected commit={commits} block={blocked}")
    print(f"{len(CASES) - failures}/{len(CASES)} cases passed")
    return 1 if failures else 0


def main():
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    try:
        command = json.load(sys.stdin).get("tool_input", {}).get("command", "")
    except (ValueError, AttributeError):
        return 0
    reason = verdict(command, os.environ.get("CLAUDE_PROJECT_DIR", os.getcwd()))
    if reason:
        print(f"Blocked: {reason}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
