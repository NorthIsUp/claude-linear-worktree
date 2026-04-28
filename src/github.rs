use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client as HttpClient;
use reqwest::StatusCode;

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub url: String,
    pub head_ref: String,
}

const USER_AGENT: &str = concat!("claude-lwt/", env!("CARGO_PKG_VERSION"));

/// True if `s` looks like a GitHub PR URL (e.g. https://github.com/owner/repo/pull/123).
pub fn is_pr_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("https://github.com/") || s.starts_with("http://github.com/"))
        && s.contains("/pull/")
}

/// Fetch a PR's metadata from the GitHub REST API.
pub fn fetch_pr(url: &str) -> Result<PrInfo> {
    let (owner, repo, number) = parse_pr_url(url)?;
    let token = resolve_token(owner)?;

    let api_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let resp = HttpClient::new()
        .get(&api_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", USER_AGENT)
        .send()
        .context("GitHub HTTP request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        bail!("{}", format_github_error(owner, status, &body));
    }

    let v: serde_json::Value = resp.json().context("failed to parse GitHub response JSON")?;

    let body = v["body"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(PrInfo {
        number: v["number"]
            .as_u64()
            .ok_or_else(|| anyhow!("GitHub response missing number"))?,
        title: v["title"]
            .as_str()
            .ok_or_else(|| anyhow!("GitHub response missing title"))?
            .to_string(),
        body,
        url: v["html_url"]
            .as_str()
            .ok_or_else(|| anyhow!("GitHub response missing html_url"))?
            .to_string(),
        head_ref: v["head"]["ref"]
            .as_str()
            .ok_or_else(|| anyhow!("GitHub response missing head.ref"))?
            .to_string(),
    })
}

/// Pull `(owner, repo, number)` out of a GitHub PR URL.
fn parse_pr_url(url: &str) -> Result<(&str, &str, u64)> {
    let trimmed = url.trim();
    let rest = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .ok_or_else(|| anyhow!("not a GitHub URL: {trimmed}"))?;

    let mut parts = rest.split('/');
    let owner = parts.next().filter(|s| !s.is_empty());
    let repo = parts.next().filter(|s| !s.is_empty());
    let pull = parts.next();
    let number_seg = parts.next();

    match (owner, repo, pull, number_seg) {
        (Some(owner), Some(repo), Some("pull"), Some(num)) => {
            let number: u64 = num
                .split(['?', '#'])
                .next()
                .unwrap_or(num)
                .parse()
                .with_context(|| format!("PR number '{num}' is not a positive integer"))?;
            Ok((owner, repo, number))
        }
        _ => bail!("expected https://github.com/<owner>/<repo>/pull/<number>: {trimmed}"),
    }
}

/// Read a GitHub token. Prefers `GITHUB_TOKEN`, falls back to `GH_TOKEN`.
fn resolve_token(owner: &str) -> Result<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(v) = std::env::var(var) {
            let t = v.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
    }
    bail!(
        "no GitHub token found; set GITHUB_TOKEN (or GH_TOKEN) to a token with \
         access to `{owner}`. If you already use the `gh` CLI: \
         `export GITHUB_TOKEN=$(gh auth token)`"
    )
}

/// Render a useful error for a non-2xx response from the GitHub API.
pub(crate) fn format_github_error(owner: &str, status: StatusCode, body: &str) -> String {
    let snippet = body.trim();
    let snippet = if snippet.is_empty() {
        "<empty body>".to_string()
    } else if snippet.len() > 400 {
        format!("{}…", &snippet[..400])
    } else {
        snippet.to_string()
    };

    match status.as_u16() {
        401 => format!(
            "GitHub returned 401 Unauthorized — GITHUB_TOKEN is missing, expired, or wrong.\n\n\
             Body: {snippet}"
        ),
        403 => format!(
            "GitHub returned 403 Forbidden for `{owner}`.\n\n\
             Body: {snippet}\n\n\
             Common fixes:\n  \
             • SAML SSO not authorized for `{owner}` — visit https://github.com/orgs/{owner}/sso and authorize the token.\n  \
             • Token lacks the `repo` scope (private repos)."
        ),
        404 => format!(
            "GitHub returned 404 — repo or PR not visible to this token. The API \
             returns 404 both when something doesn't exist and when your token \
             can't see it.\n\n\
             Body: {snippet}\n\n\
             Common fixes:\n  \
             • SAML SSO not authorized for `{owner}` — most common.\n      \
                 visit https://github.com/orgs/{owner}/sso and authorize the token\n  \
             • Token lacks `repo` scope (private repo).\n  \
             • Repo lives on GitHub Enterprise — point a token at that host instead."
        ),
        _ => format!("GitHub returned HTTP {status}: {snippet}"),
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
    fn parses_basic_pr_url() {
        let (o, r, n) = parse_pr_url("https://github.com/teamclara/Clara_V1/pull/854").unwrap();
        assert_eq!((o, r, n), ("teamclara", "Clara_V1", 854));
    }

    #[test]
    fn parses_pr_url_with_trailing_segment() {
        let (o, r, n) = parse_pr_url("https://github.com/owner/repo/pull/12/files").unwrap();
        assert_eq!((o, r, n), ("owner", "repo", 12));
    }

    #[test]
    fn parses_pr_url_with_query() {
        let (o, r, n) = parse_pr_url("https://github.com/owner/repo/pull/12?w=1").unwrap();
        assert_eq!((o, r, n), ("owner", "repo", 12));
    }

    #[test]
    fn rejects_issue_url_in_parser() {
        let e = parse_pr_url("https://github.com/owner/repo/issues/1").unwrap_err();
        assert!(e.to_string().contains("expected"));
    }

    #[test]
    fn sso_hint_added_for_404() {
        let msg = format_github_error("teamclara", StatusCode::NOT_FOUND, "{\"message\":\"Not Found\"}");
        assert!(msg.contains("404"));
        assert!(msg.contains("SAML SSO"));
        assert!(msg.contains("teamclara"));
        assert!(msg.contains("orgs/teamclara/sso"));
    }

    #[test]
    fn sso_hint_added_for_403() {
        let msg = format_github_error("teamclara", StatusCode::FORBIDDEN, "blocked");
        assert!(msg.contains("403"));
        assert!(msg.contains("teamclara"));
        assert!(msg.contains("SAML SSO"));
    }

    #[test]
    fn unauthorized_calls_out_token() {
        let msg = format_github_error("o", StatusCode::UNAUTHORIZED, "{}");
        assert!(msg.contains("401"));
        assert!(msg.contains("GITHUB_TOKEN"));
    }

    #[test]
    fn other_status_passed_through() {
        let msg = format_github_error("o", StatusCode::BAD_GATEWAY, "upstream down");
        assert!(msg.contains("502"));
        assert!(msg.contains("upstream down"));
    }
}
