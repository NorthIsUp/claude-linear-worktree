use anyhow::{anyhow, Result};
use git2::Repository;
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
        let p = resolve_worktree_dir(
            Path::new("/repos/myrepo"),
            "main",
            None,
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/repos/myrepo.worktrees/main"));
    }

    #[test]
    fn default_sanitizes_slashes_in_branch() {
        let p = resolve_worktree_dir(
            Path::new("/repos/myrepo"),
            "adam/abc-123-fix",
            None,
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/repos/myrepo.worktrees/adam__abc-123-fix"));
    }
}
