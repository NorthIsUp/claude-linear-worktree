use claude_lwt::git::{discover_git_root, ensure_worktree, WorktreeSetup};
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn run(cwd: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo_with_commit() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let p = dir.path();
    run(p, &["init", "-b", "main"]);
    run(p, &["config", "user.email", "t@t"]);
    run(p, &["config", "user.name", "t"]);
    run(p, &["config", "commit.gpgsign", "false"]);
    run(p, &["config", "tag.gpgsign", "false"]);
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
    assert_eq!(
        root.canonicalize().unwrap(),
        td.path().canonicalize().unwrap()
    );
}

#[test]
fn creates_new_branch_worktree_off_base() {
    let td = init_repo_with_commit();
    let wt_dir: PathBuf = td
        .path()
        .parent()
        .unwrap()
        .join(format!(
            "{}.worktrees",
            td.path().file_name().unwrap().to_string_lossy()
        ))
        .join("feature-x");

    let (actual, setup) = ensure_worktree(td.path(), "feature-x", "main", &wt_dir).unwrap();

    assert!(matches!(setup, WorktreeSetup::CreatedNewBranch));
    assert_eq!(actual, wt_dir);
    assert!(wt_dir.join(".git").exists() || wt_dir.join("README.md").exists());

    // Cleanup: git worktree remove to avoid leaking state into CI tmp.
    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}

#[test]
fn sets_upstream_config_for_new_branch_even_without_remote() {
    let td = init_repo_with_commit();
    let wt_dir = td
        .path()
        .parent()
        .unwrap()
        .join(format!(
            "{}.worktrees",
            td.path().file_name().unwrap().to_string_lossy()
        ))
        .join("feature-upstream");

    ensure_worktree(td.path(), "feature-upstream", "main", &wt_dir).unwrap();

    let remote = Command::new("git")
        .args(["config", "--get", "branch.feature-upstream.remote"])
        .current_dir(td.path())
        .output()
        .unwrap();
    assert!(remote.status.success(), "remote config not set");
    assert_eq!(
        String::from_utf8_lossy(&remote.stdout).trim(),
        "origin"
    );

    let merge = Command::new("git")
        .args(["config", "--get", "branch.feature-upstream.merge"])
        .current_dir(td.path())
        .output()
        .unwrap();
    assert!(merge.status.success(), "merge config not set");
    assert_eq!(
        String::from_utf8_lossy(&merge.stdout).trim(),
        "refs/heads/feature-upstream"
    );

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}

#[test]
fn reuses_existing_worktree_if_path_is_same_branch() {
    let td = init_repo_with_commit();
    let wt_dir = td
        .path()
        .parent()
        .unwrap()
        .join(format!(
            "{}.worktrees",
            td.path().file_name().unwrap().to_string_lossy()
        ))
        .join("feature-y");

    ensure_worktree(td.path(), "feature-y", "main", &wt_dir).unwrap();
    let (actual, again) = ensure_worktree(td.path(), "feature-y", "main", &wt_dir).unwrap();
    assert!(matches!(again, WorktreeSetup::ReusedExisting));
    assert_eq!(
        actual.canonicalize().unwrap(),
        wt_dir.canonicalize().unwrap()
    );

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}

#[test]
fn recovers_from_stale_worktree_directory() {
    let td = init_repo_with_commit();
    let parent = td.path().parent().unwrap();
    let repo_name = td.path().file_name().unwrap().to_string_lossy();
    let wt_dir = parent
        .join(format!("{}.worktrees", repo_name))
        .join("feature-stale");

    // Create a worktree, then forcibly delete just the registered gitdir to
    // simulate a stale leftover directory (a half-cleaned worktree).
    ensure_worktree(td.path(), "feature-stale", "main", &wt_dir).unwrap();
    let registered_gitdir = td.path().join(".git").join("worktrees").join("feature-stale");
    std::fs::remove_dir_all(&registered_gitdir).unwrap();

    // The directory still exists with its .git file pointing at the now-dead
    // gitdir. ensure_worktree should clean it up and create a fresh worktree.
    assert!(wt_dir.exists(), "stale dir should still be on disk");
    let (actual, setup) =
        ensure_worktree(td.path(), "feature-stale", "main", &wt_dir).unwrap();
    assert_eq!(actual, wt_dir);
    // After cleanup the branch already exists locally, so we re-bind to it
    // without going to origin — that's still a "new branch" outcome from the
    // caller's POV (no remote tracking involved).
    assert!(matches!(
        setup,
        WorktreeSetup::CreatedNewBranch | WorktreeSetup::CheckedOutExistingRemoteBranch
    ));
    assert!(wt_dir.join(".git").exists());

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}

#[test]
fn recovers_when_worktree_dir_was_removed_externally() {
    let td = init_repo_with_commit();
    let parent = td.path().parent().unwrap();
    let repo_name = td.path().file_name().unwrap().to_string_lossy();
    let wt_dir = parent
        .join(format!("{}.worktrees", repo_name))
        .join("feature-gone");

    ensure_worktree(td.path(), "feature-gone", "main", &wt_dir).unwrap();
    let registered_gitdir = td
        .path()
        .join(".git")
        .join("worktrees")
        .join("feature-gone");
    assert!(registered_gitdir.exists());

    // User did `rm -rf` on the working tree but left .git/worktrees/<name>/
    // behind — the exact state in adam's bug report.
    std::fs::remove_dir_all(&wt_dir).unwrap();
    assert!(registered_gitdir.exists(), "metadata should still be here");

    // ensure_worktree must auto-prune the stale metadata before retrying
    // worktree creation, otherwise libgit2 fails with "directory exists".
    let (actual, _setup) =
        ensure_worktree(td.path(), "feature-gone", "main", &wt_dir).unwrap();
    assert_eq!(actual, wt_dir);
    assert!(wt_dir.join(".git").exists());

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt_dir.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}

#[test]
fn errors_helpfully_when_path_holds_unrelated_directory() {
    let td = init_repo_with_commit();
    let parent = td.path().parent().unwrap();
    let repo_name = td.path().file_name().unwrap().to_string_lossy();
    let wt_dir = parent
        .join(format!("{}.worktrees", repo_name))
        .join("blocked");

    // Drop an unrelated directory at the path (no .git inside).
    std::fs::create_dir_all(&wt_dir).unwrap();
    std::fs::write(wt_dir.join("note.txt"), "hi").unwrap();

    let err = ensure_worktree(td.path(), "blocked", "main", &wt_dir).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("exists but is not a worktree"));
    assert!(
        msg.contains("rm -rf") || msg.contains("git worktree remove"),
        "expected actionable hint, got: {msg}"
    );
}

#[test]
fn reuses_existing_worktree_when_default_path_differs() {
    let td = init_repo_with_commit();
    let parent = td.path().parent().unwrap();
    let repo_name = td.path().file_name().unwrap().to_string_lossy();

    // Worktree already exists at a non-default path (e.g. legacy layout).
    let legacy = parent.join(format!("{}.legacy", repo_name)).join("feature-z");
    ensure_worktree(td.path(), "feature-z", "main", &legacy).unwrap();

    // Caller computes the default path, but we should still reuse the legacy one.
    let default_path = parent
        .join(format!("{}.worktrees", repo_name))
        .join("feature-z");
    assert!(!default_path.exists());

    let (actual, setup) = ensure_worktree(td.path(), "feature-z", "main", &default_path).unwrap();
    assert!(matches!(setup, WorktreeSetup::ReusedExisting));
    assert_eq!(
        actual.canonicalize().unwrap(),
        legacy.canonicalize().unwrap()
    );
    assert!(!default_path.exists(), "should not have created a new dir");

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", legacy.to_str().unwrap()])
        .current_dir(td.path())
        .status();
}
