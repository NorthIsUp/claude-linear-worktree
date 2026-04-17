use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use claude_lwt::cli::{normalize_ticket_id, Args};
use claude_lwt::git::{
    discover_git_root, ensure_worktree, resolve_base_branch, resolve_worktree_dir,
};
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
    issue: &IssueInfo,
    passthrough: &[String],
) -> Result<()> {
    let has_context = issue
        .description
        .as_deref()
        .map(|d| !d.trim().is_empty())
        .unwrap_or(false);

    let prompt = initial_prompt(&TicketContext {
        identifier: &issue.identifier,
        title: &issue.title,
        url: &issue.url,
        has_context,
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
