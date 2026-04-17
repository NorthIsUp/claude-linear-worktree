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
