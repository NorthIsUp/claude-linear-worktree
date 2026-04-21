pub struct TicketContext<'a> {
    pub identifier: &'a str,
    pub title: &'a str,
    pub url: &'a str,
    /// True when the ticket has a non-empty description to pull context from.
    pub has_context: bool,
}

pub struct PrContext<'a> {
    pub number: u64,
    pub title: &'a str,
    pub url: &'a str,
    /// True when the PR has a non-empty body to pull context from.
    pub has_context: bool,
}

pub fn pr_initial_prompt(ctx: &PrContext<'_>) -> String {
    if ctx.has_context {
        format!(
            "You are continuing work on GitHub PR #{num}: \"{title}\"\n\
             URL: {url}\n\
             \n\
             The PR branch is checked out in this worktree. Read the PR body,\n\
             review the existing diff against the base branch, and continue\n\
             the work. Leave comments on the PR as progress updates.",
            num = ctx.number,
            title = ctx.title,
            url = ctx.url,
        )
    } else {
        format!(
            "You are continuing work on GitHub PR #{num}: \"{title}\"\n\
             URL: {url}\n\
             \n\
             The PR has no description yet. Review the existing diff against\n\
             the base branch to understand what's been done, then continue\n\
             the work and leave PR comments as progress updates.",
            num = ctx.number,
            title = ctx.title,
            url = ctx.url,
        )
    }
}

pub fn initial_prompt(ctx: &TicketContext<'_>) -> String {
    if ctx.has_context {
        format!(
            "You are working on Linear ticket {id}: \"{title}\"\n\
             URL: {url}\n\
             \n\
             Pull context from the ticket and make a plan. Frequently leave\n\
             comments on the ticket as updates on your progress.",
            id = ctx.identifier,
            title = ctx.title,
            url = ctx.url,
        )
    } else {
        format!(
            "You are starting work on a new Linear feature, ticket {id}: \"{title}\"\n\
             URL: {url}\n\
             \n\
             This ticket has no body yet, so there is no prior context to read.\n\
             Start planning the work and frequently update the ticket with\n\
             comments as the plan evolves and you make progress.",
            id = ctx.identifier,
            title = ctx.title,
            url = ctx.url,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_context_prompt_when_body_present() {
        let p = initial_prompt(&TicketContext {
            identifier: "ABC-123",
            title: "Fix login",
            url: "https://linear.app/x/issue/ABC-123",
            has_context: true,
        });
        assert!(p.contains("ABC-123"));
        assert!(p.contains("Fix login"));
        assert!(p.contains("https://linear.app/x/issue/ABC-123"));
        assert!(p.contains("Pull context"));
    }

    #[test]
    fn renders_new_feature_prompt_when_no_body() {
        let p = initial_prompt(&TicketContext {
            identifier: "X-1",
            title: "New thing",
            url: "u",
            has_context: false,
        });
        assert!(p.contains("starting work on a new Linear feature"));
        assert!(p.contains("no body yet"));
        assert!(!p.contains("Pull context"));
    }

    #[test]
    fn quotes_title_inline() {
        let p = initial_prompt(&TicketContext {
            identifier: "X-1",
            title: "Do the thing",
            url: "u",
            has_context: true,
        });
        assert!(p.contains("\"Do the thing\""));
    }
}
