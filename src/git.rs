use anyhow::{anyhow, Context, Result};
use git2::{BranchType, Repository, WorktreeAddOptions};
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

pub fn resolve_base_branch(git_root: &Path, requested: &str) -> Result<String> {
    let repo = Repository::open(git_root)?;
    if repo.find_branch(requested, git2::BranchType::Local).is_ok() {
        return Ok(requested.to_string());
    }
    if requested == "main" && repo.find_branch("master", git2::BranchType::Local).is_ok() {
        eprintln!("warning: base branch 'main' not found; falling back to 'master'");
        return Ok("master".to_string());
    }
    anyhow::bail!("base branch {requested} not found locally");
}

/// Outcome of [`ensure_worktree`].
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

    if worktree_path.exists() {
        if is_worktree_for_branch(&repo, worktree_path, branch_name)? {
            return Ok(WorktreeSetup::ReusedExisting);
        }
        anyhow::bail!(
            "path {} exists but is not a worktree for branch {branch_name}",
            worktree_path.display()
        );
    }

    let remote_has_branch = fetch_and_check_remote_branch(&repo, branch_name)?;

    std::fs::create_dir_all(worktree_path.parent().unwrap_or(Path::new(".")))?;

    let wt_name = worktree_path
        .file_name()
        .ok_or_else(|| anyhow!("worktree_path has no final component"))?
        .to_string_lossy()
        .into_owned();

    let target_oid = if remote_has_branch {
        let full = format!("refs/remotes/origin/{branch_name}");
        repo.refname_to_id(&full)?
    } else {
        let full_local = format!("refs/heads/{base_branch}");
        repo.refname_to_id(&full_local)
            .with_context(|| format!("base branch {base_branch} not found locally"))?
    };

    let commit = repo.find_commit(target_oid)?;

    let branch = match repo.find_branch(branch_name, BranchType::Local) {
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
    if remote.fetch::<&str>(&[branch_name], None, None).is_err() {
        return Ok(false);
    }
    let full = format!("refs/remotes/origin/{branch_name}");
    Ok(repo.refname_to_id(&full).is_ok())
}

fn is_worktree_for_branch(repo: &Repository, path: &Path, branch_name: &str) -> Result<bool> {
    let path_canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for name in repo.worktrees()?.iter().flatten() {
        let wt = repo.find_worktree(name)?;
        let wt_path = wt.path();
        let wt_canon = wt_path
            .canonicalize()
            .unwrap_or_else(|_| wt_path.to_path_buf());
        if wt_canon == path_canon {
            // Worktrees have a .git FILE (not dir) pointing to the gitdir.
            let head_path = wt_path.join(".git");
            let git_common = if head_path.is_file() {
                let contents = std::fs::read_to_string(&head_path)?;
                let gitdir = contents
                    .trim()
                    .strip_prefix("gitdir: ")
                    .unwrap_or("")
                    .trim();
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
        let p = resolve_worktree_dir(Path::new("/repos/myrepo"), "main", None).unwrap();
        assert_eq!(p, PathBuf::from("/repos/myrepo.worktrees/main"));
    }

    #[test]
    fn default_sanitizes_slashes_in_branch() {
        let p = resolve_worktree_dir(Path::new("/repos/myrepo"), "adam/abc-123-fix", None).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/repos/myrepo.worktrees/adam__abc-123-fix")
        );
    }
}
