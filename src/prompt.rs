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
             The PR branch is checked out in this worktree. Read the PR body and\n\
             review the existing diff against the base branch.\n\
             \n\
             Put your remaining-work plan in the PR description as a markdown\n\
             checklist (`- [ ]` items). As each item is finished, edit the PR\n\
             description to mark it `- [x]`. Update frequently — the user is\n\
             watching the PR live and uses the checklist to follow progress.",
            num = ctx.number,
            title = ctx.title,
            url = ctx.url,
        )
    } else {
        format!(
            "You are continuing work on GitHub PR #{num}: \"{title}\"\n\
             URL: {url}\n\
             \n\
             The PR has no description yet. Review the existing diff against the\n\
             base branch to understand what's been done, then write the plan for\n\
             remaining work into the PR description as a markdown checklist\n\
             (`- [ ]` items). As each item is finished, edit the PR description\n\
             to mark it `- [x]`. Update frequently — the user is watching the\n\
             PR live and uses the checklist to follow progress.",
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
             Pull context from the ticket and make a plan. Put the plan in the\n\
             Linear ticket description as a markdown checklist (`- [ ]` items).\n\
             As each item is finished, edit the ticket to mark it `- [x]`.\n\
             Update frequently — the user is watching the ticket live and uses\n\
             the checklist to follow progress.",
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
             Plan the work, then put the plan in the ticket description as a\n\
             markdown checklist (`- [ ]` items). As each item is finished, edit\n\
             the ticket to mark it `- [x]`. Update frequently — the user is\n\
             watching the ticket live and uses the checklist to follow progress.",
            id = ctx.identifier,
            title = ctx.title,
            url = ctx.url,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_live_checklist_instruction(p: &str) {
        assert!(p.contains("`- [ ]`"), "missing unchecked checklist marker");
        assert!(p.contains("`- [x]`"), "missing checked checklist marker");
        assert!(p.contains("watching"), "missing live-progress framing");
    }

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
        assert_live_checklist_instruction(&p);
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
        assert_live_checklist_instruction(&p);
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

    #[test]
    fn pr_prompt_with_body_includes_checklist_instruction() {
        let p = pr_initial_prompt(&PrContext {
            number: 42,
            title: "Refactor X",
            url: "https://github.com/o/r/pull/42",
            has_context: true,
        });
        assert!(p.contains("PR #42"));
        assert!(p.contains("Refactor X"));
        assert!(p.contains("PR description"));
        assert_live_checklist_instruction(&p);
    }

    #[test]
    fn pr_prompt_without_body_includes_checklist_instruction() {
        let p = pr_initial_prompt(&PrContext {
            number: 7,
            title: "WIP",
            url: "u",
            has_context: false,
        });
        assert!(p.contains("no description yet"));
        assert!(p.contains("PR description"));
        assert_live_checklist_instruction(&p);
    }
}
