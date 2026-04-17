use anyhow::{bail, Result};

/// Resolve a Linear API token.
///
/// Order:
///   1. `LINEAR_TOKEN` environment variable (if non-empty).
///   2. (Deferred) `linear-cli`-based fallback — see spec section "Auth" item 2.
pub fn resolve_token() -> Result<String> {
    resolve_token_with_env(std::env::var("LINEAR_TOKEN").ok())
}

fn resolve_token_with_env(env_value: Option<String>) -> Result<String> {
    match env_value.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => Ok(v.to_string()),
        _ => bail!(
            "no Linear API key found; set LINEAR_TOKEN \
             (create one at https://linear.app/settings/account/security)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_set_env_value() {
        let t = resolve_token_with_env(Some("abc".to_string())).unwrap();
        assert_eq!(t, "abc");
    }

    #[test]
    fn trims_whitespace() {
        let t = resolve_token_with_env(Some("  xyz\n".to_string())).unwrap();
        assert_eq!(t, "xyz");
    }

    #[test]
    fn errors_on_missing() {
        let e = resolve_token_with_env(None).unwrap_err();
        assert!(e.to_string().contains("LINEAR_TOKEN"));
    }

    #[test]
    fn errors_on_empty() {
        let e = resolve_token_with_env(Some("   ".to_string())).unwrap_err();
        assert!(e.to_string().contains("LINEAR_TOKEN"));
    }
}
