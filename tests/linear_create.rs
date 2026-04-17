use claude_lwt::linear::{Client, TeamInfo};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn list_teams_returns_all() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "teams": { "nodes": [
                { "id": "t1", "key": "ENG", "name": "Engineering" },
                { "id": "t2", "key": "DES", "name": "Design" }
            ] } }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let teams: Vec<TeamInfo> = tokio::task::spawn_blocking(move || {
        Client::with_endpoint("t", &endpoint).list_teams()
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0].key, "ENG");
    assert_eq!(teams[1].name, "Design");
}

#[tokio::test(flavor = "current_thread")]
async fn create_issue_returns_new_issue() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "issueCreate": {
                "success": true,
                "issue": {
                    "id": "i1",
                    "identifier": "ENG-42",
                    "title": "New thing",
                    "url": "https://linear.app/x/issue/ENG-42",
                    "branchName": "adam/eng-42-new-thing"
                }
            } }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/graphql", server.uri());
    let issue = tokio::task::spawn_blocking(move || {
        Client::with_endpoint("t", &endpoint).create_issue("t1", "New thing")
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(issue.identifier, "ENG-42");
    assert_eq!(issue.branch_name, "adam/eng-42-new-thing");
}
