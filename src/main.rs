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
