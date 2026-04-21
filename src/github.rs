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
        bail!("gh pr view failed: {}", stderr.trim());
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
}
