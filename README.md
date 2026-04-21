# claude-lwt

Launch Claude Code in a git worktree for a Linear ticket.

## Install

```bash
cargo install --git https://github.com/NorthIsUp/claude-linear-worktree
ln -sf "$(which claude-lwt)" ~/.local/bin/clt
```

Or download a release tarball from
https://github.com/NorthIsUp/claude-linear-worktree/releases — the tarball
includes a `clt` symlink.

## Usage

```bash
# Work on an existing ticket
clt ABC-123

# Resume work on a GitHub PR (uses the PR head branch for the worktree)
clt https://github.com/owner/repo/pull/123

# Start a new feature — prompts for title, creates Linear ticket, launches claude
clt

# Pass extra flags to claude
clt ABC-123 -- --model opus --resume
```

PR mode requires the `gh` CLI to be installed and authenticated. It does not
require `LINEAR_TOKEN`.

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
