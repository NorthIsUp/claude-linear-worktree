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
