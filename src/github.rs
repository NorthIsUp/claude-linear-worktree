use anyhow::{anyhow, bail, Context, Result};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub url: String,
    pub head_ref: String,
}

/// True if `s` looks like a GitHub PR URL (e.g. https://github.com/owner/repo/pull/123).
pub fn is_pr_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("https://github.com/") || s.starts_with("http://github.com/"))
        && s.contains("/pull/")
}

/// Shell out to `gh pr view <url>` to fetch the PR head branch and metadata.
pub fn fetch_pr(url: &str) -> Result<PrInfo> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            url,
            "--json",
            "number,title,body,url,headRefName",
        ])
        .output()
        .context("failed to invoke `gh` (is the GitHub CLI installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", format_gh_error(url, stderr.trim()));
    }

    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse gh pr view JSON")?;

    let body = v["body"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(PrInfo {
        number: v["number"]
            .as_u64()
            .ok_or_else(|| anyhow!("gh response missing number"))?,
        title: v["title"]
            .as_str()
            .ok_or_else(|| anyhow!("gh response missing title"))?
            .to_string(),
        body,
        url: v["url"]
            .as_str()
            .ok_or_else(|| anyhow!("gh response missing url"))?
            .to_string(),
        head_ref: v["headRefName"]
            .as_str()
            .ok_or_else(|| anyhow!("gh response missing headRefName"))?
            .to_string(),
    })
}

/// Pull `(owner, repo)` out of a GitHub URL we already accepted as a PR URL.
fn extract_owner_repo(url: &str) -> Option<(&str, &str)> {
    let rest = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("http://github.com/"))?;
    let mut parts = rest.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some((owner, repo))
    }
}

/// Build a message for a failed `gh pr view`. When `gh` returns the privacy-
/// preserving "Could not resolve to a Repository" GraphQL error, the most
/// common cause is SAML SSO not authorized for the org — hint at that.
pub(crate) fn format_gh_error(url: &str, stderr: &str) -> String {
    if stderr.contains("Could not resolve to a Repository") {
        let owner = extract_owner_repo(url).map(|(o, _)| o).unwrap_or("<org>");
        format!(
            "gh could not see this repo. GitHub returns this error both when \
             a repo doesn't exist and when your token can't see it.\n\n\
             gh said: {stderr}\n\n\
             Common fixes:\n  \
             • SAML SSO not authorized for `{owner}` — most common.\n      \
                 gh auth refresh -h github.com -s read:org\n      \
                 then visit https://github.com/orgs/{owner}/sso and authorize the token\n  \
             • Wrong account — check `gh auth status`; switch with `gh auth switch`.\n  \
             • Repo lives on GitHub Enterprise — set the host in `gh auth status`."
        )
    } else {
        format!("gh pr view failed: {stderr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_https_pr_url() {
        assert!(is_pr_url("https://github.com/teamclara/Clara_V1/pull/758"));
    }

    #[test]
    fn detects_pr_url_with_trailing_path() {
        assert!(is_pr_url("https://github.com/owner/repo/pull/1/files"));
    }

    #[test]
    fn rejects_issue_url() {
        assert!(!is_pr_url("https://github.com/owner/repo/issues/1"));
    }

    #[test]
    fn rejects_linear_ticket() {
        assert!(!is_pr_url("ABC-123"));
    }

    #[test]
    fn rejects_non_github_host() {
        assert!(!is_pr_url("https://gitlab.com/owner/repo/pull/1"));
    }

    #[test]
    fn extracts_owner_and_repo() {
        assert_eq!(
            extract_owner_repo("https://github.com/teamclara/Clara_V1/pull/854"),
            Some(("teamclara", "Clara_V1"))
        );
        assert_eq!(
            extract_owner_repo("https://github.com/owner/repo"),
            Some(("owner", "repo"))
        );
        assert_eq!(extract_owner_repo("https://github.com/"), None);
    }

    #[test]
    fn sso_hint_added_when_repo_not_resolved() {
        let stderr =
            "GraphQL: Could not resolve to a Repository with the name 'teamclara/Clara_V1'.";
        let msg = format_gh_error("https://github.com/teamclara/Clara_V1/pull/854", stderr);
        assert!(msg.contains("SAML SSO"));
        assert!(msg.contains("teamclara"));
        assert!(msg.contains("gh auth refresh"));
        assert!(msg.contains("orgs/teamclara/sso"));
        assert!(msg.contains(stderr));
    }

    #[test]
    fn other_errors_passed_through() {
        let msg = format_gh_error(
            "https://github.com/o/r/pull/1",
            "no PRs found matching that query",
        );
        assert!(msg.starts_with("gh pr view failed:"));
        assert!(!msg.contains("SAML"));
    }
}
