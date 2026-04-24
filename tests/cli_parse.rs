use claude_lwt::cli::Args;

#[test]
fn parses_ticket_id_only() {
    let a = Args::parse_from(["claude-lwt", "ABC-123"]);
    assert_eq!(a.ticket_input().as_deref(), Some("ABC-123"));
    assert!(a.claude_args.is_empty());
    assert_eq!(a.base, "main");
    assert!(!a.no_exec);
}

#[test]
fn forwards_args_after_double_dash() {
    let a = Args::parse_from(["claude-lwt", "ABC-1", "--", "--model", "opus"]);
    assert_eq!(a.ticket_input().as_deref(), Some("ABC-1"));
    assert_eq!(a.claude_args, vec!["--model", "opus"]);
}

#[test]
fn flags_parsed_before_double_dash() {
    let a = Args::parse_from([
        "claude-lwt",
        "--base",
        "develop",
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
    assert!(a.ticket_id.is_empty());
    assert_eq!(a.ticket_input(), None);
}

#[test]
fn worktree_dir_flag_reads_path() {
    let a = Args::parse_from(["claude-lwt", "--worktree-dir", "/tmp/wt", "ABC-1"]);
    assert_eq!(
        a.worktree_dir.as_deref().and_then(|p| p.to_str()),
        Some("/tmp/wt")
    );
}

#[test]
fn multi_word_positional_joined_as_sentence() {
    let a = Args::parse_from(["claude-lwt", "we", "need", "to", "speed", "the", "thing", "up"]);
    assert_eq!(
        a.ticket_input().as_deref(),
        Some("we need to speed the thing up")
    );
}

#[test]
fn quoted_sentence_positional_preserved() {
    let a = Args::parse_from(["claude-lwt", "fix login bug"]);
    assert_eq!(a.ticket_input().as_deref(), Some("fix login bug"));
}

#[test]
fn multi_word_passthrough_preserved() {
    let a = Args::parse_from([
        "claude-lwt",
        "make",
        "it",
        "faster",
        "--",
        "--model",
        "opus",
    ]);
    assert_eq!(a.ticket_input().as_deref(), Some("make it faster"));
    assert_eq!(a.claude_args, vec!["--model", "opus"]);
}

use claude_lwt::cli::{looks_like_linear_ticket, looks_like_sentence, normalize_ticket_id};

#[test]
fn linear_ticket_detected() {
    assert!(looks_like_linear_ticket("ABC-123"));
    assert!(looks_like_linear_ticket("abc-1"));
}

#[test]
fn branch_names_not_mistaken_for_ticket() {
    assert!(!looks_like_linear_ticket(
        "claude/update-deployment-notifications-DGf4n"
    ));
    assert!(!looks_like_linear_ticket("feature/abc-123-foo"));
    assert!(!looks_like_linear_ticket("main"));
    assert!(!looks_like_linear_ticket("ABC-12b"));
    assert!(!looks_like_linear_ticket("-123"));
    assert!(!looks_like_linear_ticket("ABC-"));
}

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

#[test]
fn extracts_id_from_linear_url() {
    assert_eq!(
        normalize_ticket_id("https://linear.app/clarahealthus/issue/CLA-588/block-list-on-did-it"),
        "CLA-588"
    );
}

#[test]
fn extracts_id_from_linear_url_without_slug() {
    assert_eq!(
        normalize_ticket_id("https://linear.app/acme/issue/abc-9"),
        "ABC-9"
    );
}

#[test]
fn extracts_id_from_linear_url_with_query_and_whitespace() {
    assert_eq!(
        normalize_ticket_id("  https://linear.app/acme/issue/abc-9?foo=bar  "),
        "ABC-9"
    );
}

#[test]
fn sentence_detected_from_inner_whitespace() {
    assert!(looks_like_sentence("we need to speed the thing up"));
    assert!(looks_like_sentence("fix login"));
    assert!(looks_like_sentence("  two  words  "));
}

#[test]
fn single_token_not_sentence() {
    assert!(!looks_like_sentence("ABC-123"));
    assert!(!looks_like_sentence("adam/cla-1005-foo"));
    assert!(!looks_like_sentence("https://linear.app/acme/issue/ABC-1"));
    assert!(!looks_like_sentence("  ABC-123  "));
    assert!(!looks_like_sentence(""));
}
