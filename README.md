# claude-lwt

Launch Claude Code in a git worktree for a Linear ticket.

## Install

```bash
cargo install --git https://github.com/NorthIsUp/claude-linear-worktree
ln -sf "$(which claude-lwt)" ~/.local/bin/clw
```

Or download a release tarball from
https://github.com/NorthIsUp/claude-linear-worktree/releases — the tarball
includes a `clw` symlink.

## Usage

```bash
# Work on an existing ticket
clw ABC-123

# Resume work on a GitHub PR (uses the PR head branch for the worktree)
clw https://github.com/owner/repo/pull/123

# Attach to / create a worktree for a specific branch
clw adam/cla-1005-async-more-things-middleware

# Create a new ticket from a one-liner — no prompt, no quotes needed
clw we need to speed the thing up

# Start a new feature — prompts for title, creates Linear ticket, launches claude
clw

# Pass extra flags to claude
clw ABC-123 -- --model opus --resume
```

PR mode requires the `gh` CLI to be installed and authenticated. It does not
require `LINEAR_TOKEN`.

## Shell activation

By default the binary execs `claude` directly, which can't change the parent
shell's working directory. Install the `clw` shell function if you want your
shell to land inside the worktree when claude exits:

```bash
# bash/zsh — add to ~/.bashrc or ~/.zshrc
eval "$(clw activate --shell $SHELL)"
```

The function runs the real binary with `--emit-shell`, then `eval`s its
`cd … && exec claude …` output.

## Environment

- `LINEAR_TOKEN` — required. Create one at
  https://linear.app/settings/account/security.
- `LINEAR_TEAM_ID` — optional default Linear team for new tickets.
- `CLAUDE_WORKTREE_DIR` — optional override of the worktree base path.

## How it works

1. Resolves or creates a Linear ticket.
2. Fetches the ticket's auto-generated `branchName`.
3. Creates a git worktree tracking `origin/<branchName>` if the remote
   branch exists, otherwise a new branch off `main` (or `master`).
4. Default worktree location: `<git_root>/../<repo>.worktrees/<branchName>`.
5. Execs `claude` in the worktree with an initial prompt pointing at the
   Linear ticket.
