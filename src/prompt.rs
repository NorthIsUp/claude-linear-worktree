pub struct TicketContext<'a> {
    pub identifier: &'a str,
    pub title: &'a str,
    pub url: &'a str,
}

pub fn initial_prompt(ctx: &TicketContext<'_>) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_all_fields() {
        let p = initial_prompt(&TicketContext {
            identifier: "ABC-123",
            title: "Fix login",
            url: "https://linear.app/x/issue/ABC-123",
        });
        assert!(p.contains("ABC-123"));
        assert!(p.contains("Fix login"));
        assert!(p.contains("https://linear.app/x/issue/ABC-123"));
        assert!(p.contains("make a plan"));
    }

    #[test]
    fn quotes_title_inline() {
        let p = initial_prompt(&TicketContext {
            identifier: "X-1",
            title: "Do the thing",
            url: "u",
        });
        assert!(p.contains("\"Do the thing\""));
    }
}
