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
