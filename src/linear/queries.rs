use graphql_client::GraphQLQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "linear-schema.json",
    query_path = "queries/fetch_issue.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FetchIssue;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "linear-schema.json",
    query_path = "queries/list_teams.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ListTeams;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "linear-schema.json",
    query_path = "queries/create_issue.graphql",
    response_derives = "Debug, Clone"
)]
pub struct CreateIssue;
