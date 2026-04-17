use claude_lwt::linear::{Client, IssueInfo};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn fetch_issue_parses_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "lin_api_test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "issue": {
                    "id": "uuid-1",
                    "identifier": "ABC-123",
                    "title": "Fix login",
                    "description": "Steps to reproduce: ...",
                    "url": "https://linear.app/x/issue/ABC-123",
                    "branchName": "adam/abc-123-fix-login"
                }
            }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let issue: IssueInfo = tokio::task::spawn_blocking(move || {
        let client = Client::with_endpoint("lin_api_test", &endpoint);
        client.fetch_issue("ABC-123")
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(issue.identifier, "ABC-123");
    assert_eq!(issue.title, "Fix login");
    assert_eq!(
        issue.description.as_deref(),
        Some("Steps to reproduce: ...")
    );
    assert_eq!(issue.url, "https://linear.app/x/issue/ABC-123");
    assert_eq!(issue.branch_name, "adam/abc-123-fix-login");
}

#[tokio::test(flavor = "current_thread")]
async fn fetch_issue_errors_on_null() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "issue": null }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let err = tokio::task::spawn_blocking(move || {
        Client::with_endpoint("t", &endpoint).fetch_issue("DOES-NOT-EXIST")
    })
    .await
    .unwrap()
    .unwrap_err();

    assert!(err.to_string().to_lowercase().contains("not found"));
}
