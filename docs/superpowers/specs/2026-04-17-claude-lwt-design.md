# claude-lwt — Design

**Date:** 2026-04-17
**Status:** Approved (brainstorming phase)
**Repo:** `NorthIsUp/claude-linear-worktree`

## Summary

`claude-lwt` is a Rust CLI that bridges Linear tickets and Claude Code. Given a
Linear ticket ID (or nothing), it creates or reuses a git worktree for the
ticket's branch and launches `claude` inside it with an initial prompt
instructing the agent to work the ticket and post progress comments back to
Linear.

The tool is distributed as a single static binary (plus a `clt` alias symlink).

## Goals / Non-goals

**Goals**
- One-command "start working on ticket X" flow.
- Portable single-binary install (macOS + Linux, x86_64 + arm64).
- No dependency on a Linear Rust SDK (existing ones are stale).
- Pass-through of arbitrary flags to `claude` so it stays compatible with
  future Claude Code flags.

**Non-goals**
- Management of ticket lifecycle (status transitions, assignees). The launched
  Claude agent handles that through Linear MCP or direct API calls — out of
  scope for this tool.
- PR creation, merge, or review. The worktree is the boundary.
- OAuth flow for Linear. Personal API key only (with `linear-cli` fallback).

## CLI Surface

```
claude-lwt [TICKET_ID] [OPTIONS] [-- <claude args>]
clt [TICKET_ID] [OPTIONS] [-- <claude args>]         # alias symlink
```

**Positional**
- `TICKET_ID` — e.g. `ABC-123`. Case-insensitive, normalized to uppercase.
  Optional; if omitted, a new ticket is created.

**Options**
- `--worktree-dir <PATH>` (env `CLAUDE_WORKTREE_DIR`) — base dir for worktrees.
- `--base <BRANCH>` — base branch for new worktrees. Default: `main`, falling
  back to `master` if `main` is absent.
- `--team <TEAM_KEY>` (env `LINEAR_TEAM_ID`) — Linear team for new tickets.
- `--title <STR>` — title for a new ticket (skips the interactive prompt).
- `--no-exec` — set up the worktree but do not launch `claude`. For testing and
  scripting.
- All unknown arguments and anything after `--` are forwarded to `claude`.

**Environment**
- `LINEAR_TOKEN` — Linear personal API key. Required unless `linear-cli` is
  on `$PATH` and already authenticated.

## Architecture

Single binary, four Rust modules:

| Module       | Responsibility                                                                                 |
|--------------|------------------------------------------------------------------------------------------------|
| `cli.rs`     | `clap` arg parsing, trailing-arg passthrough.                                                 |
| `linear.rs`  | Linear GraphQL client using `graphql_client` + `reqwest` (blocking). Handles auth resolution. |
| `git.rs`     | Repo discovery, remote fetch, branch existence check, worktree creation (via `git2`).         |
| `prompt.rs`  | Renders the initial prompt template from ticket metadata.                                     |
| `main.rs`    | Orchestrates parse → resolve ticket → setup worktree → exec `claude`.                         |

**Dependencies**
- `clap` (derive) — argument parsing
- `reqwest` (blocking, `rustls-tls`) — HTTP
- `graphql_client` — typed GraphQL queries from `.graphql` files against a
  vendored Linear schema
- `serde`, `serde_json`
- `git2` — libgit2 bindings for worktree and branch ops
- `anyhow` — error plumbing
- `dialoguer` — interactive title prompt

No async runtime — blocking `reqwest` keeps the binary small and the control
flow linear.

## Data Flow

```
parse args
  │
  ├─ ticket_id? ── no ──▶ list teams (auth'd user)
  │                        │
  │                        ▶ team resolution:
  │                            1. --team or LINEAR_TEAM_ID set → use it
  │                            2. exactly one team → auto-pick
  │                            3. otherwise → hard error
  │                        │
  │                        ▶ title: use --title, else prompt via dialoguer
  │                        │
  │                        ▶ Linear: createIssue mutation
  │                        │
  │                        ▶ ticket_id := created.identifier
  │
  ▼
Linear: fetch issue { identifier, title, url, branchName }
  │
  ▼
resolve worktree_dir:
  --worktree-dir  >  CLAUDE_WORKTREE_DIR  >  <git_root>/../<repo>.worktrees/<branchName>
  │
  ▼
git2: open repo, find "origin" remote, fetch
  │
  ▼
origin/<branchName> exists?
  │
  ├─ yes ─▶ worktree add <dir> tracking origin/<branchName>
  └─ no  ─▶ worktree add <dir> with new branch <branchName> off <base>
  │
  ▼
render initial prompt (see Prompt Template)
  │
  ▼
chdir <worktree_dir> ─▶ execvp claude [passthrough args] "<prompt>"
```

## Initial Prompt Template

A single template is used for both paths. By the time we invoke `claude`, a
ticket always exists (either provided or just created).

```
You are working on Linear ticket {IDENTIFIER}: "{TITLE}"
URL: {URL}

Pull context from the ticket and make a plan. Frequently leave
comments on the ticket as updates on your progress.
```

## Auth

1. If `LINEAR_TOKEN` is set, use it as bearer against
   `https://api.linear.app/graphql`.
2. Otherwise, attempt to retrieve a token via `linear-cli`. The implementation
   task must: (a) inspect `linear-cli --help` and its docs to find a
   token-printing subcommand (e.g. `linear-cli auth print-token` or reading
   `linear-cli`'s config file directly); (b) if no such mechanism exists,
   remove this fallback and treat `LINEAR_TOKEN` as strictly required. The
   fallback is best-effort convenience, not a hard requirement of this tool.
3. If neither works, hard error with instructions to create a key at
   `https://linear.app/settings/account/security`.

## Error Handling

**Hard errors (exit 1):**
- No usable Linear auth.
- Not inside a git repo.
- Linear GraphQL error (auth, not-found, network).
- `git2` errors (fetch failure, base branch missing, worktree path occupied
  by unrelated content).
- `claude` not on `$PATH`.

**Warnings (continue):**
- Worktree path exists and matches the expected branch → reuse, log
  `reusing existing worktree at <path>`.
- `main` absent but `master` present → use `master`, warn once.

**Input normalization:**
- `TICKET_ID` uppercased before query.

## Tooling

The project uses **mise** (https://mise.jdx.dev) to pin the Rust toolchain and
install dev-time tools. This gives contributors a one-command setup (`mise
install`) and lets CI install the exact same versions with `mise install` on
the runner.

- `mise.toml` pins:
  - `rust` (e.g. `latest`, or a specific `1.x` if we hit an MSRV issue)
  - `graphql_client_cli` (used once for schema introspection in Task 6;
    re-runnable by any contributor who wants to refresh the vendored schema)
- CI workflows install mise first, then run `mise install`, then delegate to
  standard `cargo` commands. This replaces any hardcoded `actions-rust-lang/*`
  setup action.

## Repo Layout

```
claude-linear-worktree/
├── mise.toml                  # pins Rust + graphql_client_cli
├── Cargo.toml                 # binary "claude-lwt"
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── linear.rs
│   ├── git.rs
│   └── prompt.rs
├── queries/
│   ├── fetch_issue.graphql
│   ├── create_issue.graphql
│   └── list_teams.graphql
├── linear-schema.graphql      # vendored Linear GraphQL schema for codegen
├── docs/superpowers/specs/    # design docs (this file)
├── .github/workflows/
│   ├── ci.yml                 # adapted from NorthIsUp/tunnletops
│   └── release.yml            # adapted from NorthIsUp/tunnletops
│                              # (extended to emit `clt` symlink inside tarball)
├── README.md
├── LICENSE
└── .gitignore
```

## Distribution

- `cargo install --git https://github.com/NorthIsUp/claude-linear-worktree`
  for source installs.
- Tagged releases produce multi-platform binary tarballs (via `release.yml`).
  Each tarball includes both `claude-lwt` and a `clt` symlink.
- Cargo-install users are instructed in the README to
  `ln -s "$(which claude-lwt)" ~/.local/bin/clt`.

## Testing

In scope:
- Unit tests for `cli.rs` (arg parsing, passthrough boundary).
- Unit tests for `prompt.rs` (template rendering).
- Unit tests for worktree path resolution logic in `git.rs` (pure function,
  no filesystem).
- Unit tests for ticket ID normalization.

Out of scope for automated CI:
- Integration tests that hit real Linear or create real worktrees. Manual
  smoke-test steps documented in the README.

## Open Items (intentionally deferred)

- OAuth flow.
- Windows support (libgit2 works, but we're not shipping a Windows target from
  `release.yml` initially).
- Homebrew tap.
- `--resume <ticket>` semantics distinct from default (currently default
  behavior handles both cases by checking remote branch existence).
