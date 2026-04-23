use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use claude_lwt::activate::{self, sh_quote};
use claude_lwt::cli::{looks_like_linear_ticket, normalize_ticket_id, Args};
use claude_lwt::git::{
    discover_git_root, ensure_worktree, resolve_base_branch, resolve_worktree_dir,
};
use claude_lwt::github::{self, PrInfo};
use claude_lwt::linear::{auth, Client, IssueInfo};
use claude_lwt::prompt::{initial_prompt, pr_initial_prompt, PrContext, TicketContext};

enum Source {
    Linear(IssueInfo),
    GitHubPr(PrInfo),
    Branch(String),
}

impl Source {
    fn branch_name(&self) -> &str {
        match self {
            Source::Linear(i) => &i.branch_name,
            Source::GitHubPr(p) => &p.head_ref,
            Source::Branch(b) => b,
        }
    }
}

fn main() -> Result<()> {
    let raw: Vec<std::ffi::OsString> = env::args_os().collect();
    if raw.get(1).and_then(|s| s.to_str()) == Some("activate") {
        return activate::run(&raw[2..]);
    }

    let args = Args::parse();

    let cwd = env::current_dir()?;
    let git_root = discover_git_root(&cwd)?;

    let source = resolve_source(&args)?;

    let base = resolve_base_branch(&git_root, &args.base)?;
    let worktree_dir = resolve_worktree_dir(
        &git_root,
        source.branch_name(),
        args.worktree_dir.as_deref(),
    )?;

    let setup = ensure_worktree(&git_root, source.branch_name(), &base, &worktree_dir)?;
    eprintln!("worktree ready: {} ({:?})", worktree_dir.display(), setup);

    if args.no_exec {
        eprintln!("--no-exec set; stopping before claude launch");
        if args.emit_shell {
            println!("cd {}", sh_quote(&worktree_dir.display().to_string()));
        }
        return Ok(());
    }

    if args.emit_shell {
        emit_shell_launch(&worktree_dir, &source, &args.claude_args);
        return Ok(());
    }

    launch_claude(&worktree_dir, &source, &args.claude_args)
}

fn emit_shell_launch(
    worktree_dir: &std::path::Path,
    source: &Source,
    passthrough: &[String],
) {
    let prompt = build_prompt(source);
    let mut claude_cmd = String::from("exec claude");
    for a in passthrough {
        claude_cmd.push(' ');
        claude_cmd.push_str(&sh_quote(a));
    }
    if let Some(p) = prompt {
        claude_cmd.push(' ');
        claude_cmd.push_str(&sh_quote(&p));
    }
    println!(
        "cd {} && {}",
        sh_quote(&worktree_dir.display().to_string()),
        claude_cmd
    );
}

fn build_prompt(source: &Source) -> Option<String> {
    match source {
        Source::Branch(_) => None,
        Source::Linear(issue) => Some({
            let has_context = issue
                .description
                .as_deref()
                .map(|d| !d.trim().is_empty())
                .unwrap_or(false);
            initial_prompt(&TicketContext {
                identifier: &issue.identifier,
                title: &issue.title,
                url: &issue.url,
                has_context,
            })
        }),
        Source::GitHubPr(pr) => Some({
            let has_context = pr
                .body
                .as_deref()
                .map(|b| !b.trim().is_empty())
                .unwrap_or(false);
            pr_initial_prompt(&PrContext {
                number: pr.number,
                title: &pr.title,
                url: &pr.url,
                has_context,
            })
        }),
    }
}

fn resolve_source(args: &Args) -> Result<Source> {
    match args.ticket_id.as_deref() {
        Some(raw) if github::is_pr_url(raw) => Ok(Source::GitHubPr(github::fetch_pr(raw.trim())?)),
        Some(raw) if !is_linear_url(raw) && !looks_like_linear_ticket(raw) => {
            Ok(Source::Branch(raw.trim().to_string()))
        }
        Some(raw) => {
            let token = auth::resolve_token()?;
            let linear = Client::new(token);
            let id = normalize_ticket_id(raw);
            Ok(Source::Linear(linear.fetch_issue(&id)?))
        }
        None => {
            let token = auth::resolve_token()?;
            let linear = Client::new(token);
            Ok(Source::Linear(create_new_ticket(
                &linear,
                args.team.as_deref(),
                args.title.as_deref(),
            )?))
        }
    }
}

fn is_linear_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("https://linear.app/")
        || s.starts_with("http://linear.app/")
        || s.starts_with("linear.app/")
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

    let (title, description) = match title_override {
        Some(t) => (t.to_string(), None),
        None => prompt_for_title_and_body()?,
    };

    linear.create_issue(&team_id, &title, description.as_deref())
}

fn prompt_for_title_and_body() -> Result<(String, Option<String>)> {
    eprintln!("Creating a new Linear ticket.");
    let one_liner: String = dialoguer::Input::<String>::new()
        .with_prompt("Title (leave empty to open editor for title + body)")
        .allow_empty(true)
        .interact_text()
        .context("failed to read title")?;

    if !one_liner.trim().is_empty() {
        return Ok((one_liner.trim().to_string(), None));
    }

    let template = "\n\n\
        # First non-comment line is the ticket TITLE.\n\
        # Everything below it becomes the ticket BODY (may be empty).\n\
        # Lines starting with `#` are stripped.\n";
    let edited = dialoguer::Editor::new()
        .edit(template)
        .context("failed to open editor")?
        .ok_or_else(|| anyhow!("editor exited without saving"))?;

    parse_title_and_body(&edited)
}

/// Strip `#`-comment lines, take the first non-empty line as the title, and
/// the remainder (trimmed) as an optional body.
fn parse_title_and_body(raw: &str) -> Result<(String, Option<String>)> {
    let mut lines = raw
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(|l| l.to_string())
        .collect::<Vec<_>>();

    // Drop leading blank lines.
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }

    let title = lines
        .first()
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    if title.is_empty() {
        bail!("no title entered");
    }

    let body_raw = lines.iter().skip(1).cloned().collect::<Vec<_>>().join("\n");
    let body_trimmed = body_raw.trim();
    let body = if body_trimmed.is_empty() {
        None
    } else {
        Some(body_trimmed.to_string())
    };

    Ok((title, body))
}

fn launch_claude(
    worktree_dir: &std::path::Path,
    source: &Source,
    passthrough: &[String],
) -> Result<()> {
    let prompt = build_prompt(source);

    let mut cmd = Command::new("claude");
    cmd.current_dir(worktree_dir);
    for arg in passthrough {
        cmd.arg(arg);
    }
    if let Some(p) = prompt {
        cmd.arg(p);
    }

    // Replace this process with claude so it inherits the terminal cleanly.
    let err = cmd.exec();
    Err(anyhow!("failed to exec claude: {err}"))
}

#[cfg(test)]
mod tests {
    use super::parse_title_and_body;

    #[test]
    fn title_only_no_body() {
        let (t, b) = parse_title_and_body("Fix login\n").unwrap();
        assert_eq!(t, "Fix login");
        assert_eq!(b, None);
    }

    #[test]
    fn title_and_body_separated() {
        let (t, b) = parse_title_and_body("Fix login\n\nSteps: 1, 2, 3\n").unwrap();
        assert_eq!(t, "Fix login");
        assert_eq!(b.as_deref(), Some("Steps: 1, 2, 3"));
    }

    #[test]
    fn comments_stripped() {
        let raw = "# comment\n  # indented comment\nActual title\nBody here\n";
        let (t, b) = parse_title_and_body(raw).unwrap();
        assert_eq!(t, "Actual title");
        assert_eq!(b.as_deref(), Some("Body here"));
    }

    #[test]
    fn blank_lines_before_title_ignored() {
        let raw = "\n\n\nTitle\nBody\n";
        let (t, b) = parse_title_and_body(raw).unwrap();
        assert_eq!(t, "Title");
        assert_eq!(b.as_deref(), Some("Body"));
    }

    #[test]
    fn empty_input_is_error() {
        let e = parse_title_and_body("# just a comment\n\n").unwrap_err();
        assert!(e.to_string().contains("no title"));
    }
}
