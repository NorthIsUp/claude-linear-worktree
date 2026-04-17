pub mod auth;
pub mod queries;

/// Summarized issue data needed by the rest of the app.
#[derive(Debug, Clone)]
pub struct IssueInfo {
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub branch_name: String,
}
