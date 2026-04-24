use clap::Parser;
use std::path::PathBuf;

/// Launch claude-code in a git worktree for a Linear ticket or GitHub PR.
///
/// Any args after `--` are forwarded verbatim to the `claude` binary.
#[derive(Parser, Debug)]
#[command(name = "claude-lwt", version, about)]
pub struct Args {
    /// One of: Linear ticket id (`ABC-123`), Linear issue URL, GitHub PR URL,
    /// a git branch name, or — if multiple words / contains whitespace — a
    /// short description that becomes the title of a newly created Linear
    /// ticket (e.g. `clw we need to speed the thing up`).
    /// If omitted entirely, prompts interactively for a new ticket.
    #[arg(num_args = 0..)]
    pub ticket_id: Vec<String>,

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

    /// Instead of exec'ing claude, print shell commands (cd + exec claude)
    /// to stdout for a wrapper shell function to eval. See `clw activate`.
    #[arg(long, hide = true)]
    pub emit_shell: bool,

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

    /// Join the positional tokens back into a single string. Returns `None`
    /// when no positional was supplied.
    pub fn ticket_input(&self) -> Option<String> {
        if self.ticket_id.is_empty() {
            None
        } else {
            Some(self.ticket_id.join(" "))
        }
    }
}

/// True when `s` contains inner whitespace after trimming — i.e. it's a
/// multi-word description rather than a branch/id/URL token.
pub fn looks_like_sentence(s: &str) -> bool {
    s.trim().contains(char::is_whitespace)
}

pub fn normalize_ticket_id(raw: &str) -> String {
    let trimmed = raw.trim();
    let id = extract_linear_id_from_url(trimmed).unwrap_or(trimmed);
    id.to_ascii_uppercase()
}

/// A Linear ticket id looks like `ABC-123` (letters, dash, digits). Anything
/// else that isn't a URL is treated as a git branch name.
pub fn looks_like_linear_ticket(s: &str) -> bool {
    let s = s.trim();
    let Some((prefix, suffix)) = s.split_once('-') else {
        return false;
    };
    !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_alphabetic())
        && !suffix.is_empty()
        && suffix.chars().all(|c| c.is_ascii_digit())
}

/// If `input` looks like a Linear issue URL, return the ticket identifier
/// segment (e.g. `CLA-588` from `https://linear.app/acme/issue/CLA-588/slug`).
fn extract_linear_id_from_url(input: &str) -> Option<&str> {
    let rest = input
        .strip_prefix("https://linear.app/")
        .or_else(|| input.strip_prefix("http://linear.app/"))
        .or_else(|| input.strip_prefix("linear.app/"))?;
    let after_issue = rest.split_once("/issue/")?.1;
    let id = after_issue.split(['/', '?', '#']).next()?;
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}
