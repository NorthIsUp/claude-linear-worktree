pub mod auth;
pub mod queries;

use anyhow::{anyhow, bail, Context, Result};
use graphql_client::{GraphQLQuery, Response};
use reqwest::blocking::Client as HttpClient;

use queries::{create_issue, fetch_issue, list_teams, CreateIssue, FetchIssue, ListTeams};

const DEFAULT_ENDPOINT: &str = "https://api.linear.app/graphql";

/// Team data returned by list_teams.
#[derive(Debug, Clone)]
pub struct TeamInfo {
    pub id: String,
    pub key: String,
    pub name: String,
}

/// Summarized issue data needed by the rest of the app.
#[derive(Debug, Clone)]
pub struct IssueInfo {
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub branch_name: String,
}

pub struct Client {
    http: HttpClient,
    endpoint: String,
    token: String,
}

impl Client {
    pub fn new(token: impl Into<String>) -> Self {
        Self::with_endpoint(token, DEFAULT_ENDPOINT)
    }

    pub fn with_endpoint(token: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(),
            endpoint: endpoint.into(),
            token: token.into(),
        }
    }

    fn post<Q: GraphQLQuery>(&self, variables: Q::Variables) -> Result<Q::ResponseData>
    where
        Q::Variables: serde::Serialize,
        Q::ResponseData: serde::de::DeserializeOwned,
    {
        let body = Q::build_query(variables);
        let resp: Response<Q::ResponseData> = self
            .http
            .post(&self.endpoint)
            .header("Authorization", &self.token)
            .json(&body)
            .send()
            .context("Linear HTTP request failed")?
            .error_for_status()
            .context("Linear returned non-2xx")?
            .json()
            .context("failed to decode Linear response JSON")?;

        if let Some(errors) = resp.errors {
            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ");
                bail!("Linear GraphQL error: {msg}");
            }
        }
        resp.data
            .ok_or_else(|| anyhow!("Linear response had no data"))
    }

    pub fn list_teams(&self) -> Result<Vec<TeamInfo>> {
        let data = self.post::<ListTeams>(list_teams::Variables {})?;
        Ok(data
            .teams
            .nodes
            .into_iter()
            .map(|n| TeamInfo {
                id: n.id,
                key: n.key,
                name: n.name,
            })
            .collect())
    }

    pub fn create_issue(
        &self,
        team_id: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<IssueInfo> {
        let data = self.post::<CreateIssue>(create_issue::Variables {
            team_id: team_id.to_string(),
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
        })?;
        let payload = data.issue_create;
        if !payload.success {
            bail!("Linear issueCreate returned success=false");
        }
        let issue = payload
            .issue
            .ok_or_else(|| anyhow!("Linear issueCreate returned no issue"))?;
        Ok(IssueInfo {
            identifier: issue.identifier,
            title: issue.title,
            description: issue.description,
            url: issue.url,
            branch_name: issue.branch_name,
        })
    }

    pub fn fetch_issue(&self, id: &str) -> Result<IssueInfo> {
        // Use raw JSON deserialization so we can treat a null `issue` field as
        // "not found" rather than a hard serde error (the Linear schema marks
        // the field NON_NULL, but the API can still return null for unknown IDs).
        let body = FetchIssue::build_query(fetch_issue::Variables { id: id.to_string() });
        let raw: serde_json::Value = self
            .http
            .post(&self.endpoint)
            .header("Authorization", &self.token)
            .json(&body)
            .send()
            .context("Linear HTTP request failed")?
            .error_for_status()
            .context("Linear returned non-2xx")?
            .json()
            .context("failed to decode Linear response JSON")?;

        // Surface GraphQL-layer errors if present.
        if let Some(errors) = raw.get("errors").and_then(|e| e.as_array()) {
            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                    .collect::<Vec<_>>()
                    .join("; ");
                bail!("Linear GraphQL error: {msg}");
            }
        }

        let issue = raw
            .get("data")
            .and_then(|d| d.get("issue"))
            .ok_or_else(|| anyhow!("Linear ticket {id} not found"))?;

        if issue.is_null() {
            bail!("Linear ticket {id} not found");
        }

        Ok(IssueInfo {
            identifier: issue["identifier"]
                .as_str()
                .ok_or_else(|| anyhow!("missing identifier"))?
                .to_string(),
            title: issue["title"]
                .as_str()
                .ok_or_else(|| anyhow!("missing title"))?
                .to_string(),
            description: issue["description"].as_str().map(String::from),
            url: issue["url"]
                .as_str()
                .ok_or_else(|| anyhow!("missing url"))?
                .to_string(),
            branch_name: issue["branchName"]
                .as_str()
                .ok_or_else(|| anyhow!("missing branchName"))?
                .to_string(),
        })
    }
}
