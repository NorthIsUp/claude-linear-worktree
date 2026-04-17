# claude-lwt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `claude-lwt`, a Rust CLI that given a Linear ticket (or none) creates/reuses a git worktree for the ticket's branch and execs `claude` inside it with an initial prompt.

**Architecture:** Single binary, four modules (`cli`, `linear`, `git`, `prompt`) + `main.rs` glue. No Linear SDK — direct GraphQL via `graphql_client` + `reqwest`. `git2` for worktree ops. Synchronous (blocking) throughout.

**Tech Stack:** Rust 2021, `clap` derive, `reqwest` blocking + rustls, `graphql_client`, `git2` (libgit2), `serde`/`serde_json`, `anyhow`, `dialoguer`. Testing with `wiremock` (HTTP) and `tempfile` (git).

**Design doc:** `docs/superpowers/specs/2026-04-17-claude-lwt-design.md`

**Target file structure** (for reference across all tasks):

```
Cargo.toml
build.rs                       # (none needed — graphql_client uses derive)
src/
  main.rs                      # orchestration
  lib.rs                       # re-exports for test-only access
  cli.rs                       # clap args + passthrough parser
  prompt.rs                    # template rendering
  linear/
    mod.rs                     # public API (Client + IssueInfo)
    auth.rs                    # LINEAR_TOKEN + linear-cli fallback
    queries.rs                 # graphql_client GraphQLQuery derives
  git.rs                       # worktree path + git2 ops
queries/
  fetch_issue.graphql
  create_issue.graphql
  list_teams.graphql
linear-schema.graphql          # vendored
tests/
  cli_parse.rs                 # integration test binary
.github/workflows/
  ci.yml
  release.yml
README.md
LICENSE
.gitignore
```

---

## Task 1: Cargo scaffolding, mise config, and `.gitignore`

**Files:**
- Create: `Cargo.toml`
- Create: `mise.toml`
- Create: `src/main.rs`
- Create: `.gitignore`

- [ ] **Step 1a: Create `mise.toml`**

```toml
[tools]
rust = "latest"
"cargo:graphql_client_cli" = "latest"
```

Verify mise picks it up:

```bash
mise install
mise exec -- rustc --version
mise exec -- graphql-client --version
```

Expected: all three succeed. Rust compiles; `graphql-client` prints a version (this tool is used once in Task 6).

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "claude-lwt"
version = "0.1.0"
edition = "2021"
description = "Launch claude-code in a git worktree for a Linear ticket"
license = "MIT"
repository = "https://github.com/NorthIsUp/claude-linear-worktree"

[[bin]]
name = "claude-lwt"
path = "src/main.rs"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive", "env"] }
dialoguer = { version = "0.11", default-features = false }
git2 = { version = "0.19", default-features = false, features = ["vendored-libgit2"] }
graphql_client = "0.14"
reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
tokio = { version = "1", features = ["macros", "rt"] }   # wiremock requires a runtime even if our app is sync
```

- [ ] **Step 2: Create stub `src/main.rs`**

```rust
fn main() -> anyhow::Result<()> {
    eprintln!("claude-lwt: not yet implemented");
    Ok(())
}
```

- [ ] **Step 3: Create `.gitignore`**

```
/target
Cargo.lock.bak
*.swp
.DS_Store
.env
.env.local
```

- [ ] **Step 4: Verify build**

Run: `cargo build`
Expected: compiles cleanly, produces `target/debug/claude-lwt`.

- [ ] **Step 5: Commit**

```bash
git add mise.toml Cargo.toml Cargo.lock src/main.rs .gitignore
git commit -m "Scaffold Cargo project with mise tooling"
```

---

## Task 2: Copy and adapt CI workflows

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Download source workflows**

Run:
```bash
mkdir -p .github/workflows
gh api repos/NorthIsUp/tunnletops/contents/.github/workflows/ci.yml --jq .content | base64 -d > .github/workflows/ci.yml
gh api repos/NorthIsUp/tunnletops/contents/.github/workflows/release.yml --jq .content | base64 -d > .github/workflows/release.yml
```

- [ ] **Step 2: Open each file and adapt**

Read both files. For each:

1. Update any hardcoded binary names or paths referring to `tunneltops`/`tunnletops` to `claude-lwt`.
2. Replace any Rust-toolchain setup steps (e.g. `actions-rust-lang/setup-rust-toolchain`, `dtolnay/rust-toolchain`) with mise installation and `mise install`:

```yaml
      - name: Install mise
        uses: jdx/mise-action@v2
        with:
          experimental: true  # enable cargo: backends if needed
      - name: Install pinned tools
        run: mise install
```

Then in subsequent steps prefix cargo commands with `mise exec --` (e.g. `mise exec -- cargo test --all`) OR rely on mise's shim activation provided by `jdx/mise-action`.

3. Keep the overall job structure (test/clippy/fmt in CI; multi-target build + release on tag in release).

- [ ] **Step 3: Extend `release.yml` to emit `clt` symlink in tarballs**

In the archive-building step (look for a `tar czf` or similar command), add the step — BEFORE the archive is created — that creates a `clt` symlink next to `claude-lwt`:

```yaml
      - name: Create clt alias symlink
        if: runner.os != 'Windows'
        run: |
          cd "$ARCHIVE_DIR"  # whatever the existing variable is — keep consistent with the file
          ln -sf claude-lwt clt
```

If the workflow uses a different archive-dir variable name, match it. Do not assume — read the file.

- [ ] **Step 4: Validate workflow YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); yaml.safe_load(open('.github/workflows/release.yml'))"`
Expected: no output (both parse).

- [ ] **Step 5: Commit**

```bash
git add .github/workflows
git commit -m "Add CI and release workflows (adapted from NorthIsUp/tunnletops)"
```

---

## Task 3: CLI argument parsing with passthrough

**Files:**
- Create: `src/lib.rs`
- Create: `src/cli.rs`
- Create: `tests/cli_parse.rs`

Design note: clap does not natively support "collect every unknown arg as passthrough". We implement passthrough with a two-step parse: (1) split argv at the first occurrence of `--` if present — everything after is definitely passthrough. (2) Also allow stopping at the first non-recognized token. To keep things simple and predictable, we REQUIRE `--` as the passthrough separator for now. (Unrecognized flags before `--` will error.) This matches Unix conventions and avoids surprising behavior; we can relax it later.

- [ ] **Step 1: Create `src/lib.rs` exposing modules for tests**

```rust
pub mod cli;
```

- [ ] **Step 2: Write failing integration test**

Create `tests/cli_parse.rs`:

```rust
use claude_lwt::cli::Args;

#[test]
fn parses_ticket_id_only() {
    let a = Args::parse_from(["claude-lwt", "ABC-123"]);
    assert_eq!(a.ticket_id.as_deref(), Some("ABC-123"));
    assert!(a.claude_args.is_empty());
    assert_eq!(a.base, "main");
    assert!(!a.no_exec);
}

#[test]
fn forwards_args_after_double_dash() {
    let a = Args::parse_from([
        "claude-lwt", "ABC-1", "--", "--model", "opus",
    ]);
    assert_eq!(a.ticket_id.as_deref(), Some("ABC-1"));
    assert_eq!(a.claude_args, vec!["--model", "opus"]);
}

#[test]
fn flags_parsed_before_double_dash() {
    let a = Args::parse_from([
        "claude-lwt",
        "--base", "develop",
        "--no-exec",
        "ABC-1",
        "--",
        "--resume",
    ]);
    assert_eq!(a.base, "develop");
    assert!(a.no_exec);
    assert_eq!(a.claude_args, vec!["--resume"]);
}

#[test]
fn no_ticket_is_ok() {
    let a = Args::parse_from(["claude-lwt"]);
    assert_eq!(a.ticket_id, None);
}

#[test]
fn worktree_dir_flag_reads_path() {
    let a = Args::parse_from([
        "claude-lwt", "--worktree-dir", "/tmp/wt", "ABC-1",
    ]);
    assert_eq!(a.worktree_dir.as_deref().and_then(|p| p.to_str()), Some("/tmp/wt"));
}
```

- [ ] **Step 3: Run test to see it fail**

Run: `cargo test --test cli_parse`
Expected: FAIL (`cli` module / `Args` does not exist yet).

- [ ] **Step 4: Implement `src/cli.rs`**

```rust
use clap::Parser;
use std::path::PathBuf;

/// Launch claude-code in a git worktree for a Linear ticket.
///
/// Any args after `--` are forwarded verbatim to the `claude` binary.
#[derive(Parser, Debug)]
#[command(name = "claude-lwt", version, about)]
pub struct Args {
    /// Linear ticket identifier (e.g. ABC-123). If omitted, a new ticket is created.
    pub ticket_id: Option<String>,

    /// Base directory for worktrees. Default: <git_root>/../<repo>.worktrees/<branch>.
    #[arg(long, env = "CLAUDE_WORKTREE_DIR")]
    pub worktree_dir: Option<PathBuf>,

    /// Base branch for new worktrees.
    #[arg(long, default_value = "main")]
    pub base: String,

    /// Linear team key for new tickets (when ticket_id is not given).
    #[arg(long, env = "LINEAR_TEAM_ID")]
    pub team: Option<String>,

    /// Title for a new ticket. If omitted, prompts interactively.
    #[arg(long)]
    pub title: Option<String>,

    /// Set up the worktree but do not exec `claude`.
    #[arg(long)]
    pub no_exec: bool,

    /// Everything after `--` is passed to `claude`.
    #[arg(last = true)]
    pub claude_args: Vec<String>,
}

impl Args {
    pub fn parse_from<I, T>(it: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        <Self as Parser>::parse_from(it)
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test cli_parse`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/cli.rs tests/cli_parse.rs
git commit -m "Implement CLI argument parsing with passthrough"
```

---

## Task 4: Ticket ID normalization

**Files:**
- Modify: `src/cli.rs` (add `normalize_ticket_id`)
- Modify: `tests/cli_parse.rs` (add tests)

- [ ] **Step 1: Write failing test**

Append to `tests/cli_parse.rs`:

```rust
use claude_lwt::cli::normalize_ticket_id;

#[test]
fn normalizes_lowercase_id() {
    assert_eq!(normalize_ticket_id("abc-123"), "ABC-123");
}

#[test]
fn keeps_uppercase_id() {
    assert_eq!(normalize_ticket_id("ABC-123"), "ABC-123");
}

#[test]
fn trims_whitespace() {
    assert_eq!(normalize_ticket_id("  abc-1 "), "ABC-1");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cli_parse`
Expected: FAIL (function not found).

- [ ] **Step 3: Implement `normalize_ticket_id`**

Append to `src/cli.rs`:

```rust
pub fn normalize_ticket_id(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test cli_parse`
Expected: PASS (8 tests total).

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs tests/cli_parse.rs
git commit -m "Add ticket ID normalization"
```

---

## Task 5: Prompt template rendering

**Files:**
- Create: `src/prompt.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module to `src/lib.rs`**

Append:

```rust
pub mod prompt;
```

- [ ] **Step 2: Write prompt module with inline failing test**

Create `src/prompt.rs`:

```rust
pub struct TicketContext<'a> {
    pub identifier: &'a str,
    pub title: &'a str,
    pub url: &'a str,
}

pub fn initial_prompt(ctx: &TicketContext<'_>) -> String {
    format!(
        "You are working on Linear ticket {id}: \"{title}\"\n\
         URL: {url}\n\
         \n\
         Pull context from the ticket and make a plan. Frequently leave\n\
         comments on the ticket as updates on your progress.",
        id = ctx.identifier,
        title = ctx.title,
        url = ctx.url,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_all_fields() {
        let p = initial_prompt(&TicketContext {
            identifier: "ABC-123",
            title: "Fix login",
            url: "https://linear.app/x/issue/ABC-123",
        });
        assert!(p.contains("ABC-123"));
        assert!(p.contains("Fix login"));
        assert!(p.contains("https://linear.app/x/issue/ABC-123"));
        assert!(p.contains("make a plan"));
    }

    #[test]
    fn quotes_title_inline() {
        let p = initial_prompt(&TicketContext {
            identifier: "X-1",
            title: "Do the thing",
            url: "u",
        });
        assert!(p.contains("\"Do the thing\""));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test prompt`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add src/prompt.rs src/lib.rs
git commit -m "Add initial prompt template rendering"
```

---

## Task 6: Vendor Linear GraphQL schema and write query files

**Files:**
- Create: `linear-schema.graphql`
- Create: `queries/fetch_issue.graphql`
- Create: `queries/create_issue.graphql`
- Create: `queries/list_teams.graphql`

- [ ] **Step 1: Ensure `graphql-client` is available via mise**

Run: `mise exec -- graphql-client --version`
Expected: prints a version. If missing, `mise install` first (it's pinned in `mise.toml`).

- [ ] **Step 2: Introspect Linear schema**

The user must have `LINEAR_TOKEN` exported. Run:

```bash
mise exec -- graphql-client introspect-schema \
  https://api.linear.app/graphql \
  --header "Authorization: $LINEAR_TOKEN" \
  --output linear-schema.graphql
```

Expected: `linear-schema.graphql` created, several thousand lines long. Commit it as vendored input to codegen.

If introspection fails (auth, network), read the error and fix before continuing.

- [ ] **Step 3: Write `queries/fetch_issue.graphql`**

```graphql
query FetchIssue($id: String!) {
  issue(id: $id) {
    id
    identifier
    title
    url
    branchName
  }
}
```

- [ ] **Step 4: Write `queries/list_teams.graphql`**

```graphql
query ListTeams {
  teams(first: 50) {
    nodes {
      id
      key
      name
    }
  }
}
```

- [ ] **Step 5: Write `queries/create_issue.graphql`**

```graphql
mutation CreateIssue($teamId: String!, $title: String!) {
  issueCreate(input: { teamId: $teamId, title: $title }) {
    success
    issue {
      id
      identifier
      title
      url
      branchName
    }
  }
}
```

- [ ] **Step 6: Commit**

```bash
git add linear-schema.graphql queries/
git commit -m "Vendor Linear schema and query files"
```

---

## Task 7: Linear query structs via graphql_client derive

**Files:**
- Create: `src/linear/mod.rs`
- Create: `src/linear/queries.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module to `src/lib.rs`**

Append:

```rust
pub mod linear;
```

- [ ] **Step 2: Create `src/linear/mod.rs`**

```rust
pub mod auth;
pub mod queries;

/// Summarized issue data needed by the rest of the app.
#[derive(Debug, Clone)]
pub struct IssueInfo {
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub branch_name: String,
}
```

Note: `auth` doesn't exist yet — this file won't compile alone. We add `auth` in Task 8. If the subagent wants to commit incrementally, temporarily omit `pub mod auth;` in this step and add it in Task 8.

- [ ] **Step 3: Create `src/linear/queries.rs`**

```rust
use graphql_client::GraphQLQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "linear-schema.graphql",
    query_path = "queries/fetch_issue.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FetchIssue;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "linear-schema.graphql",
    query_path = "queries/list_teams.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ListTeams;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "linear-schema.graphql",
    query_path = "queries/create_issue.graphql",
    response_derives = "Debug, Clone"
)]
pub struct CreateIssue;
```

- [ ] **Step 4: Verify codegen compiles**

Run: `cargo build`
Expected: compiles. If errors mention unknown scalar types (common with Linear's custom scalars like `DateTime`, `TimelessDate`), add scalar mappings at the top of `src/linear/queries.rs`:

```rust
type DateTime = String;
type TimelessDate = String;
type JSON = serde_json::Value;
type JSONObject = serde_json::Value;
```

Re-run `cargo build` until it passes. Add scalars for any other custom scalars reported by the compiler.

- [ ] **Step 5: Commit**

```bash
git add src/linear/mod.rs src/linear/queries.rs src/lib.rs
git commit -m "Wire up graphql_client codegen for Linear queries"
```

---

## Task 8: Linear auth resolution

**Files:**
- Create: `src/linear/auth.rs`
- Modify: `src/linear/mod.rs` (uncomment `pub mod auth;` if needed)

Design: We prefer `LINEAR_TOKEN`. The `linear-cli` fallback is best-effort; as documented in the spec, we only attempt it if a plausible token-printing subcommand exists. For this first implementation, we implement ONLY the env-var path. A comment in `auth.rs` records the deferred behavior.

- [ ] **Step 1: Write failing test**

Create `src/linear/auth.rs`:

```rust
use anyhow::{bail, Result};

/// Resolve a Linear API token.
///
/// Order:
///   1. `LINEAR_TOKEN` environment variable (if non-empty).
///   2. (Deferred) `linear-cli`-based fallback — see spec section "Auth" item 2.
pub fn resolve_token() -> Result<String> {
    resolve_token_with_env(std::env::var("LINEAR_TOKEN").ok())
}

fn resolve_token_with_env(env_value: Option<String>) -> Result<String> {
    match env_value.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => Ok(v.to_string()),
        _ => bail!(
            "no Linear API key found; set LINEAR_TOKEN \
             (create one at https://linear.app/settings/account/security)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_set_env_value() {
        let t = resolve_token_with_env(Some("abc".to_string())).unwrap();
        assert_eq!(t, "abc");
    }

    #[test]
    fn trims_whitespace() {
        let t = resolve_token_with_env(Some("  xyz\n".to_string())).unwrap();
        assert_eq!(t, "xyz");
    }

    #[test]
    fn errors_on_missing() {
        let e = resolve_token_with_env(None).unwrap_err();
        assert!(e.to_string().contains("LINEAR_TOKEN"));
    }

    #[test]
    fn errors_on_empty() {
        let e = resolve_token_with_env(Some("   ".to_string())).unwrap_err();
        assert!(e.to_string().contains("LINEAR_TOKEN"));
    }
}
```

- [ ] **Step 2: Ensure `src/linear/mod.rs` has `pub mod auth;`**

Read the file; if the line is absent or commented out, add it.

- [ ] **Step 3: Run tests**

Run: `cargo test linear::auth`
Expected: PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add src/linear/auth.rs src/linear/mod.rs
git commit -m "Resolve Linear API token from LINEAR_TOKEN"
```

---

## Task 9: Linear client — fetch issue

**Files:**
- Modify: `src/linear/mod.rs` (add `Client` struct + `fetch_issue`)
- Create: `tests/linear_fetch.rs`

- [ ] **Step 1: Write failing integration test with wiremock**

Create `tests/linear_fetch.rs`:

```rust
use claude_lwt::linear::{Client, IssueInfo};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn fetch_issue_parses_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "lin_api_test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "issue": {
                    "id": "uuid-1",
                    "identifier": "ABC-123",
                    "title": "Fix login",
                    "url": "https://linear.app/x/issue/ABC-123",
                    "branchName": "adam/abc-123-fix-login"
                }
            }
        })))
        .mount(&server)
        .await;

    // Run the blocking call on a blocking thread so tokio::test doesn't panic.
    let endpoint = format!("{}/graphql", server.uri());
    let issue: IssueInfo = tokio::task::spawn_blocking(move || {
        let client = Client::with_endpoint("lin_api_test", &endpoint);
        client.fetch_issue("ABC-123")
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(issue.identifier, "ABC-123");
    assert_eq!(issue.title, "Fix login");
    assert_eq!(issue.url, "https://linear.app/x/issue/ABC-123");
    assert_eq!(issue.branch_name, "adam/abc-123-fix-login");
}

#[tokio::test(flavor = "current_thread")]
async fn fetch_issue_errors_on_null() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "issue": null }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let err = tokio::task::spawn_blocking(move || {
        Client::with_endpoint("t", &endpoint).fetch_issue("DOES-NOT-EXIST")
    })
    .await
    .unwrap()
    .unwrap_err();

    assert!(err.to_string().to_lowercase().contains("not found"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test linear_fetch`
Expected: FAIL (`Client` not defined).

- [ ] **Step 3: Implement `Client` in `src/linear/mod.rs`**

Replace the contents of `src/linear/mod.rs` with:

```rust
pub mod auth;
pub mod queries;

use anyhow::{anyhow, bail, Context, Result};
use graphql_client::{GraphQLQuery, Response};
use reqwest::blocking::Client as HttpClient;

use queries::{FetchIssue, fetch_issue};

const DEFAULT_ENDPOINT: &str = "https://api.linear.app/graphql";

#[derive(Debug, Clone)]
pub struct IssueInfo {
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub branch_name: String,
}

pub struct Client {
    http: HttpClient,
    endpoint: String,
    token: String,
}

impl Client {
    pub fn new(token: impl Into<String>) -> Self {
        Self::with_endpoint(token, DEFAULT_ENDPOINT)
    }

    pub fn with_endpoint(token: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(),
            endpoint: endpoint.into(),
            token: token.into(),
        }
    }

    fn post<Q: GraphQLQuery>(&self, variables: Q::Variables) -> Result<Q::ResponseData>
    where
        Q::Variables: serde::Serialize,
        Q::ResponseData: serde::de::DeserializeOwned,
    {
        let body = Q::build_query(variables);
        let resp: Response<Q::ResponseData> = self
            .http
            .post(&self.endpoint)
            .header("Authorization", &self.token)
            .json(&body)
            .send()
            .context("Linear HTTP request failed")?
            .error_for_status()
            .context("Linear returned non-2xx")?
            .json()
            .context("failed to decode Linear response JSON")?;

        if let Some(errors) = resp.errors {
            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ");
                bail!("Linear GraphQL error: {msg}");
            }
        }
        resp.data.ok_or_else(|| anyhow!("Linear response had no data"))
    }

    pub fn fetch_issue(&self, id: &str) -> Result<IssueInfo> {
        let data = self.post::<FetchIssue>(fetch_issue::Variables { id: id.to_string() })?;
        let issue = data
            .issue
            .ok_or_else(|| anyhow!("Linear ticket {id} not found"))?;
        Ok(IssueInfo {
            identifier: issue.identifier,
            title: issue.title,
            url: issue.url,
            branch_name: issue.branch_name,
        })
    }
}
```

Note on field names: `graphql_client` converts `branchName` → `branch_name`. If the compiler reports a different field name, adjust accordingly.

- [ ] **Step 4: Run test**

Run: `cargo test --test linear_fetch`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/linear/mod.rs tests/linear_fetch.rs
git commit -m "Add Linear fetch_issue client"
```

---

## Task 10: Linear client — list teams and create issue

**Files:**
- Modify: `src/linear/mod.rs`
- Create: `tests/linear_create.rs`

- [ ] **Step 1: Write failing test**

Create `tests/linear_create.rs`:

```rust
use claude_lwt::linear::{Client, TeamInfo};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn list_teams_returns_all() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "teams": { "nodes": [
                { "id": "t1", "key": "ENG", "name": "Engineering" },
                { "id": "t2", "key": "DES", "name": "Design" }
            ] } }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let teams: Vec<TeamInfo> = tokio::task::spawn_blocking(move || {
        Client::with_endpoint("t", &endpoint).list_teams()
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0].key, "ENG");
    assert_eq!(teams[1].name, "Design");
}

#[tokio::test(flavor = "current_thread")]
async fn create_issue_returns_new_issue() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "issueCreate": {
                "success": true,
                "issue": {
                    "id": "i1",
                    "identifier": "ENG-42",
                    "title": "New thing",
                    "url": "https://linear.app/x/issue/ENG-42",
                    "branchName": "adam/eng-42-new-thing"
                }
            } }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let issue = tokio::task::spawn_blocking(move || {
        Client::with_endpoint("t", &endpoint).create_issue("t1", "New thing")
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(issue.identifier, "ENG-42");
    assert_eq!(issue.branch_name, "adam/eng-42-new-thing");
}
```

- [ ] **Step 2: Run to see fail**

Run: `cargo test --test linear_create`
Expected: FAIL (types/methods not defined).

- [ ] **Step 3: Extend `src/linear/mod.rs`**

Add after `IssueInfo`:

```rust
#[derive(Debug, Clone)]
pub struct TeamInfo {
    pub id: String,
    pub key: String,
    pub name: String,
}
```

Add `use` additions at the top:

```rust
use queries::{CreateIssue, create_issue, ListTeams, list_teams};
```

Add inside `impl Client`:

```rust
    pub fn list_teams(&self) -> Result<Vec<TeamInfo>> {
        let data = self.post::<ListTeams>(list_teams::Variables {})?;
        Ok(data
            .teams
            .nodes
            .into_iter()
            .map(|n| TeamInfo { id: n.id, key: n.key, name: n.name })
            .collect())
    }

    pub fn create_issue(&self, team_id: &str, title: &str) -> Result<IssueInfo> {
        let data = self.post::<CreateIssue>(create_issue::Variables {
            team_id: team_id.to_string(),
            title: title.to_string(),
        })?;
        let payload = data.issue_create;
        if !payload.success {
            bail!("Linear issueCreate returned success=false");
        }
        let issue = payload
            .issue
            .ok_or_else(|| anyhow!("Linear issueCreate returned no issue"))?;
        Ok(IssueInfo {
            identifier: issue.identifier,
            title: issue.title,
            url: issue.url,
            branch_name: issue.branch_name,
        })
    }
```

Note: if `graphql_client` generates different variable field names (e.g. `teamId` vs `team_id`), adjust. `graphql_client` conventionally converts camelCase to snake_case in the generated `Variables` struct.

- [ ] **Step 4: Run tests**

Run: `cargo test --test linear_create`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/linear/mod.rs tests/linear_create.rs
git commit -m "Add Linear list_teams and create_issue client methods"
```

---

## Task 11: Worktree path resolution (pure function)

**Files:**
- Create: `src/git.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module to `src/lib.rs`**

Append: `pub mod git;`

- [ ] **Step 2: Create `src/git.rs` with failing tests for pure path resolution**

```rust
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// Resolve the worktree directory for a given branch.
///
/// Precedence:
///   1. `override_dir` (from `--worktree-dir` or `CLAUDE_WORKTREE_DIR`), if set, is used verbatim.
///   2. Otherwise: `<git_root>/../<repo_name>.worktrees/<sanitized_branch>`.
pub fn resolve_worktree_dir(
    git_root: &Path,
    branch_name: &str,
    override_dir: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(p) = override_dir {
        return Ok(p.to_path_buf());
    }

    let repo_name = git_root
        .file_name()
        .ok_or_else(|| anyhow!("git_root has no final component: {}", git_root.display()))?
        .to_string_lossy()
        .into_owned();

    let parent = git_root
        .parent()
        .ok_or_else(|| anyhow!("git_root has no parent: {}", git_root.display()))?;

    let safe_branch = sanitize_for_path(branch_name);

    Ok(parent
        .join(format!("{repo_name}.worktrees"))
        .join(safe_branch))
}

/// Replace filesystem-unfriendly characters in a branch name for use as a directory name.
fn sanitize_for_path(name: &str) -> String {
    name.replace('/', "__")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_override_verbatim() {
        let p = resolve_worktree_dir(
            Path::new("/repos/myrepo"),
            "feature/x",
            Some(Path::new("/tmp/custom")),
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/tmp/custom"));
    }

    #[test]
    fn default_uses_sibling_worktrees_dir() {
        let p = resolve_worktree_dir(
            Path::new("/repos/myrepo"),
            "main",
            None,
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/repos/myrepo.worktrees/main"));
    }

    #[test]
    fn default_sanitizes_slashes_in_branch() {
        let p = resolve_worktree_dir(
            Path::new("/repos/myrepo"),
            "adam/abc-123-fix",
            None,
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/repos/myrepo.worktrees/adam__abc-123-fix"));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test git::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/git.rs src/lib.rs
git commit -m "Add worktree path resolution"
```

---

## Task 12: Git operations — repo discovery and worktree creation

**Files:**
- Modify: `src/git.rs`
- Create: `tests/git_worktree.rs`

- [ ] **Step 1: Write failing integration test**

Create `tests/git_worktree.rs`:

```rust
use claude_lwt::git::{discover_git_root, ensure_worktree, WorktreeSetup};
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn run(cwd: &std::path::Path, args: &[&str]) {
    let status = Command::new("git").args(args).current_dir(cwd).status().unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo_with_commit() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let p = dir.path();
    run(p, &["init", "-b", "main"]);
    run(p, &["config", "user.email", "t@t"]);
    run(p, &["config", "user.name", "t"]);
    std::fs::write(p.join("README.md"), "hi").unwrap();
    run(p, &["add", "README.md"]);
    run(p, &["commit", "-m", "init"]);
    dir
}

#[test]
fn discovers_git_root_from_subdir() {
    let td = init_repo_with_commit();
    let sub = td.path().join("nested");
    std::fs::create_dir_all(&sub).unwrap();
    let root = discover_git_root(&sub).unwrap();
    assert_eq!(root.canonicalize().unwrap(), td.path().canonicalize().unwrap());
}

#[test]
fn creates_new_branch_worktree_off_base() {
    let td = init_repo_with_commit();
    let wt_dir: PathBuf = td.path().parent().unwrap()
        .join(format!("{}.worktrees", td.path().file_name().unwrap().to_string_lossy()))
        .join("feature-x");

    let setup = ensure_worktree(
        td.path(),
        "feature-x",
        "main",
        &wt_dir,
    ).unwrap();

    assert!(matches!(setup, WorktreeSetup::CreatedNewBranch));
    assert!(wt_dir.join(".git").exists() || wt_dir.join("README.md").exists());

    // Cleanup: git worktree remove to avoid leaking state into CI tmp.
    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}

#[test]
fn reuses_existing_worktree_if_path_is_same_branch() {
    let td = init_repo_with_commit();
    let wt_dir = td.path().parent().unwrap()
        .join(format!("{}.worktrees", td.path().file_name().unwrap().to_string_lossy()))
        .join("feature-y");

    ensure_worktree(td.path(), "feature-y", "main", &wt_dir).unwrap();
    let again = ensure_worktree(td.path(), "feature-y", "main", &wt_dir).unwrap();
    assert!(matches!(again, WorktreeSetup::ReusedExisting));

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test git_worktree`
Expected: FAIL (functions don't exist).

- [ ] **Step 3: Extend `src/git.rs`**

Add at the top (after the existing `use` lines):

```rust
use git2::{Repository, WorktreeAddOptions};
```

Add public API:

```rust
#[derive(Debug)]
pub enum WorktreeSetup {
    CreatedNewBranch,
    CheckedOutExistingRemoteBranch,
    ReusedExisting,
}

/// Discover the git repo root that contains `start_dir`.
pub fn discover_git_root(start_dir: &Path) -> Result<PathBuf> {
    let repo = Repository::discover(start_dir)
        .with_context(|| format!("not inside a git repository: {}", start_dir.display()))?;
    // repo.path() points to the .git directory; its parent is the worktree root.
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow!("bare repo unsupported"))?;
    Ok(workdir.to_path_buf())
}

/// Create or reuse a worktree for `branch_name`, checking out from origin if the
/// remote has it, otherwise creating a new branch off `base_branch`.
pub fn ensure_worktree(
    git_root: &Path,
    branch_name: &str,
    base_branch: &str,
    worktree_path: &Path,
) -> Result<WorktreeSetup> {
    let repo = Repository::open(git_root)
        .with_context(|| format!("failed to open repo at {}", git_root.display()))?;

    // 1. Reuse if path already has a worktree for this branch.
    if worktree_path.exists() {
        if is_worktree_for_branch(&repo, worktree_path, branch_name)? {
            return Ok(WorktreeSetup::ReusedExisting);
        }
        bail!(
            "path {} exists but is not a worktree for branch {branch_name}",
            worktree_path.display()
        );
    }

    // 2. Try to fetch origin (best effort — ignore if no remote).
    let remote_has_branch = fetch_and_check_remote_branch(&repo, branch_name)?;

    // 3. Create the worktree.
    std::fs::create_dir_all(worktree_path.parent().unwrap_or(Path::new(".")))?;

    let wt_name = worktree_path
        .file_name()
        .ok_or_else(|| anyhow!("worktree_path has no final component"))?
        .to_string_lossy()
        .into_owned();

    // Resolve the commit we want to base the new branch on.
    let target_oid = if remote_has_branch {
        let full = format!("refs/remotes/origin/{branch_name}");
        repo.refname_to_id(&full)?
    } else {
        // Ensure base_branch exists locally; try "main" → fall back to "master" only if caller asked.
        let full_local = format!("refs/heads/{base_branch}");
        repo.refname_to_id(&full_local)
            .with_context(|| format!("base branch {base_branch} not found locally"))?
    };

    let commit = repo.find_commit(target_oid)?;

    // Create the branch pointing at the target commit. If it exists locally, reuse.
    let branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(_) => repo.branch(branch_name, &commit, false)?,
    };
    let branch_ref = branch.into_reference();

    let mut opts = WorktreeAddOptions::new();
    opts.reference(Some(&branch_ref));
    repo.worktree(&wt_name, worktree_path, Some(&opts))
        .with_context(|| format!("failed to create worktree at {}", worktree_path.display()))?;

    if remote_has_branch {
        Ok(WorktreeSetup::CheckedOutExistingRemoteBranch)
    } else {
        Ok(WorktreeSetup::CreatedNewBranch)
    }
}

fn fetch_and_check_remote_branch(repo: &Repository, branch_name: &str) -> Result<bool> {
    let mut remote = match repo.find_remote("origin") {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };
    // Fetch with default options. Ignore failure (network may be down); we'll
    // just not see the remote branch and create a new one.
    if remote.fetch::<&str>(&[branch_name], None, None).is_err() {
        return Ok(false);
    }
    let full = format!("refs/remotes/origin/{branch_name}");
    Ok(repo.refname_to_id(&full).is_ok())
}

fn is_worktree_for_branch(repo: &Repository, path: &Path, branch_name: &str) -> Result<bool> {
    // Walk existing worktrees; if any has matching path AND HEAD points to this branch, it's a match.
    for name in repo.worktrees()?.iter().flatten() {
        let wt = repo.find_worktree(name)?;
        let wt_path = wt.path();
        if wt_path == path {
            // Read HEAD of that worktree to see if it's our branch.
            let head_path = wt.path().join(".git");
            // Worktrees have a .git FILE (not dir) pointing to the gitdir.
            let git_common = if head_path.is_file() {
                let contents = std::fs::read_to_string(&head_path)?;
                let gitdir = contents.trim().strip_prefix("gitdir: ").unwrap_or("").trim();
                PathBuf::from(gitdir)
            } else {
                head_path
            };
            let head_file = git_common.join("HEAD");
            if head_file.exists() {
                let head = std::fs::read_to_string(&head_file)?;
                let expected = format!("ref: refs/heads/{branch_name}");
                if head.trim() == expected {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test git_worktree`
Expected: PASS (3 tests).

If tests fail because `git2`'s `Worktree` API differs subtly from the above in the installed version, adjust per compiler errors. The API shape (`repo.worktree(name, path, Some(&opts))`) is stable in git2 0.19.

- [ ] **Step 5: Commit**

```bash
git add src/git.rs tests/git_worktree.rs
git commit -m "Add git repo discovery and worktree creation"
```

---

## Task 13: Base branch fallback (main → master)

**Files:**
- Modify: `src/git.rs`
- Modify: `tests/git_worktree.rs`

- [ ] **Step 1: Write failing test**

Append to `tests/git_worktree.rs`:

```rust
use claude_lwt::git::resolve_base_branch;

#[test]
fn resolve_base_branch_returns_configured_if_exists() {
    let td = init_repo_with_commit();
    let branch = resolve_base_branch(td.path(), "main").unwrap();
    assert_eq!(branch, "main");
}

#[test]
fn resolve_base_branch_falls_back_master_when_main_requested_missing() {
    let td = tempdir().unwrap();
    run(td.path(), &["init", "-b", "master"]);
    run(td.path(), &["config", "user.email", "t@t"]);
    run(td.path(), &["config", "user.name", "t"]);
    std::fs::write(td.path().join("R"), "").unwrap();
    run(td.path(), &["add", "R"]);
    run(td.path(), &["commit", "-m", "c"]);

    let branch = resolve_base_branch(td.path(), "main").unwrap();
    assert_eq!(branch, "master");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test git_worktree resolve_base_branch`
Expected: FAIL.

- [ ] **Step 3: Implement in `src/git.rs`**

```rust
pub fn resolve_base_branch(git_root: &Path, requested: &str) -> Result<String> {
    let repo = Repository::open(git_root)?;
    if repo.find_branch(requested, git2::BranchType::Local).is_ok() {
        return Ok(requested.to_string());
    }
    if requested == "main" && repo.find_branch("master", git2::BranchType::Local).is_ok() {
        eprintln!("warning: base branch 'main' not found; falling back to 'master'");
        return Ok("master".to_string());
    }
    bail!("base branch {requested} not found locally");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test git_worktree`
Expected: PASS (all 5 tests now).

- [ ] **Step 5: Commit**

```bash
git add src/git.rs tests/git_worktree.rs
git commit -m "Add main→master base branch fallback"
```

---

## Task 14: Main orchestration

**Files:**
- Modify: `src/main.rs`

No new tests here — this is orchestration glue tested via manual smoke test. The individual pieces are covered by unit + integration tests above.

- [ ] **Step 1: Implement `src/main.rs`**

Replace the contents of `src/main.rs`:

```rust
use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use claude_lwt::cli::{normalize_ticket_id, Args};
use claude_lwt::git::{discover_git_root, ensure_worktree, resolve_base_branch, resolve_worktree_dir};
use claude_lwt::linear::{auth, Client, IssueInfo};
use claude_lwt::prompt::{initial_prompt, TicketContext};

fn main() -> Result<()> {
    let args = Args::parse();

    let token = auth::resolve_token()?;
    let linear = Client::new(token);

    let cwd = env::current_dir()?;
    let git_root = discover_git_root(&cwd)?;

    let issue = match args.ticket_id.as_deref() {
        Some(raw) => {
            let id = normalize_ticket_id(raw);
            linear.fetch_issue(&id)?
        }
        None => create_new_ticket(&linear, args.team.as_deref(), args.title.as_deref())?,
    };

    let base = resolve_base_branch(&git_root, &args.base)?;
    let worktree_dir =
        resolve_worktree_dir(&git_root, &issue.branch_name, args.worktree_dir.as_deref())?;

    let setup = ensure_worktree(&git_root, &issue.branch_name, &base, &worktree_dir)?;
    eprintln!("worktree ready: {} ({:?})", worktree_dir.display(), setup);

    if args.no_exec {
        eprintln!("--no-exec set; stopping before claude launch");
        return Ok(());
    }

    launch_claude(&worktree_dir, &issue, &args.claude_args)
}

fn create_new_ticket(
    linear: &Client,
    team_override: Option<&str>,
    title_override: Option<&str>,
) -> Result<IssueInfo> {
    let teams = linear.list_teams()?;
    if teams.is_empty() {
        bail!("no Linear teams available to this account");
    }

    let team_id = match team_override {
        Some(key_or_id) => teams
            .iter()
            .find(|t| t.key.eq_ignore_ascii_case(key_or_id) || t.id == key_or_id)
            .ok_or_else(|| anyhow!("team '{key_or_id}' not found"))?
            .id
            .clone(),
        None => {
            if teams.len() == 1 {
                teams[0].id.clone()
            } else {
                let keys: Vec<&str> = teams.iter().map(|t| t.key.as_str()).collect();
                bail!(
                    "multiple teams available ({}); specify --team or set LINEAR_TEAM_ID",
                    keys.join(", ")
                );
            }
        }
    };

    let title = match title_override {
        Some(t) => t.to_string(),
        None => dialoguer::Input::<String>::new()
            .with_prompt("Ticket title")
            .interact_text()
            .context("failed to read title")?,
    };

    linear.create_issue(&team_id, &title)
}

fn launch_claude(
    worktree_dir: &std::path::Path,
    issue: &IssueInfo,
    passthrough: &[String],
) -> Result<()> {
    let prompt = initial_prompt(&TicketContext {
        identifier: &issue.identifier,
        title: &issue.title,
        url: &issue.url,
    });

    let mut cmd = Command::new("claude");
    cmd.current_dir(worktree_dir);
    for arg in passthrough {
        cmd.arg(arg);
    }
    cmd.arg(&prompt);

    // Replace this process with claude so it inherits the terminal cleanly.
    let err = cmd.exec();
    Err(anyhow!("failed to exec claude: {err}"))
}
```

- [ ] **Step 2: Verify build**

Run: `cargo build`
Expected: compiles cleanly. Fix any import paths flagged by the compiler.

- [ ] **Step 3: Manual smoke test (with --no-exec)**

Requires: `LINEAR_TOKEN` exported, a real Linear ticket ID you can fetch, and a cwd inside a git repo with `origin`.

Run:
```bash
cargo run -- --no-exec <YOUR-TICKET-ID>
```
Expected: prints a `worktree ready:` line and exits 0. A new worktree directory exists on disk.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "Wire up main orchestration"
```

---

## Task 15: README and LICENSE

**Files:**
- Create: `README.md`
- Create: `LICENSE`

- [ ] **Step 1: Write `README.md`**

```markdown
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

# Start a new feature — prompts for title, creates Linear ticket, launches claude
clt

# Pass extra flags to claude
clt ABC-123 -- --model opus --resume
```

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
```

- [ ] **Step 2: Write `LICENSE`**

```
MIT License

Copyright (c) 2026 Adam Hitchcock

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 3: Commit**

```bash
git add README.md LICENSE
git commit -m "Add README and MIT license"
```

---

## Task 16: Create GitHub repo and push

- [ ] **Step 1: Confirm remote does not already exist**

Run: `gh repo view NorthIsUp/claude-linear-worktree 2>&1 | head -5`
Expected: "Could not resolve to a Repository" OR if it already exists, skip to step 3.

- [ ] **Step 2: Create the repo**

Run:
```bash
gh repo create NorthIsUp/claude-linear-worktree --public \
  --description "Launch Claude Code in a git worktree for a Linear ticket" \
  --source . --remote origin
```
Expected: "✓ Created repository NorthIsUp/claude-linear-worktree on GitHub".

- [ ] **Step 3: Push main**

Run: `git push -u origin main`
Expected: branch published; CI workflow triggered.

- [ ] **Step 4: Verify CI**

Run: `gh run list --limit 1`
Expected: one run, eventually green. Address any failures (typically missing `rustfmt`/`clippy` components in the CI workflow — fix in-place and push a follow-up commit).

---

## Spec-Coverage Self-Check (done before handoff)

| Spec requirement | Covered by task |
|------------------|-----------------|
| Binary `claude-lwt` + `clt` alias | Task 1 (binary), Task 2 (tarball symlink), Task 15 (cargo-install symlink instruction) |
| Positional `TICKET_ID` case-insensitive | Tasks 3, 4 |
| `--worktree-dir` / `CLAUDE_WORKTREE_DIR` | Tasks 3, 11 |
| `--base` with main→master fallback | Tasks 3, 13 |
| `--team` / `LINEAR_TEAM_ID` | Tasks 3, 14 |
| `--title` + interactive prompt | Tasks 3, 14 |
| `--no-exec` | Tasks 3, 14 |
| Passthrough args after `--` | Task 3 |
| `LINEAR_TOKEN` env var auth | Task 8 |
| `linear-cli` fallback (deferred per spec) | Task 8 comment |
| Fetch issue via GraphQL | Tasks 6, 7, 9 |
| Create issue via GraphQL | Tasks 6, 7, 10 |
| List teams via GraphQL | Tasks 6, 7, 10 |
| Team auto-pick if 1, error if many | Task 14 |
| Interactive title prompt (dialoguer) | Task 14 |
| Worktree path resolution | Task 11 |
| Remote branch check + worktree create | Task 12 |
| Reuse existing worktree | Task 12 |
| Render initial prompt template | Task 5 |
| Exec claude via execvp | Task 14 |
| CI + release workflows | Task 2 |
| README / LICENSE | Task 15 |
| GitHub repo + push | Task 16 |
