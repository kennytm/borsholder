//! GitHub API access.

use failure::Error;
use reqwest::Client;
use reqwest::header::{Authorization, Bearer, Headers, Raw};
use std::str::from_utf8;

/// Types related to the main GraphQL query.
///
/// Please see [GitHub's GraphQL schema] for details.
///
/// [GitHub's GraphQL schema]: https://developer.github.com/v4/reference/query/
pub mod graphql {
    #![cfg_attr(feature = "cargo-clippy", allow(missing_docs_in_private_items))]

    use chrono::{DateTime, Utc};
    use tera::Value;

    /// A generic GraphQL connection, which is the same as a vector in our use case.
    #[derive(Deserialize)]
    pub struct Connection<T> {
        /// List of nodes in this connection.
        pub nodes: Vec<T>,
    }

    /// The reply of a GraphQL query.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Reply {
        pub data: Data,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Data {
        pub repository: Repository,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Repository {
        pub pull_requests: Connection<PullRequest>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PullRequest {
        pub author: Actor,
        pub created_at: DateTime<Utc>,
        pub mergeable: MergeableState,
        pub number: u32,
        pub title: String,
        pub additions: u32,
        pub deletions: u32,
        pub labels: Connection<Label>,
        pub commits: Connection<PullRequestCommit>,
        pub timeline: Connection<Value>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Actor {
        pub login: String,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Label {
        pub name: String,
        pub color: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PullRequestCommit {
        pub commit: Commit,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Commit {
        pub committed_date: DateTime<Utc>,
        pub status: Option<Status>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Status {
        pub contexts: Vec<StatusContext>,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct StatusContext {
        pub context: String,
        pub description: String,
        pub target_url: String,
        pub state: StatusState,
    }

    #[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Copy)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum MergeableState {
        Unknown,
        Mergeable,
        Conflicting,
    }

    #[derive(Deserialize, Serialize, Clone, Copy)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum StatusState {
        Expected,
        Error,
        Failure,
        Pending,
        Success,
    }
}

/// A GraphQL request.
#[derive(Serialize)]
struct Request<'variables> {
    /// The query string.
    query: &'static str,
    /// Variables of the query.
    variables: Variables<'variables>,
}

/// Variables in a GraphQL request.
///
/// This structure is hard-coded to support the main GraphQL query, `QUERY`.
#[derive(Serialize)]
struct Variables<'variables> {
    /// Owner of the repository.
    owner: &'variables str,
    /// Name of the repository.
    repo: &'variables str,
}

/// URL to send the GraphQL requests.
const GITHUB_ENDPOINT: &str = "https://api.github.com/graphql";

/// The main GraphQL query.
const QUERY: &str = include!("github.gql");

/// Obtains the list of open pull requests and associated information from GitHub.
pub fn query(
    client: &Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> Result<Vec<graphql::PullRequest>, Error> {
    info!("Preparing to send GitHub request");
    let mut response = client
        .post(GITHUB_ENDPOINT)
        .header(Authorization(Bearer {
            token: token.to_owned(),
        }))
        .json(&Request {
            query: QUERY,
            variables: Variables { owner, repo },
        })
        .send()?
        .error_for_status()?;

    {
        let headers = response.headers();
        let rate_limit_remaining = fetch_rate_limit(headers, "X-RateLimit-Remaining");
        let rate_limit_limit = fetch_rate_limit(headers, "X-RateLimit-Limit");
        info!(
            "GitHub rate limit: {}/{}",
            rate_limit_remaining,
            rate_limit_limit
        );
    }

    let reply = response.json::<graphql::Reply>()?;

    let prs = reply.data.repository.pull_requests.nodes;
    info!("Obtained {} PRs from GitHub", prs.len());
    Ok(prs)
}

/// Fetch rate-limit related number from the GitHub response.
fn fetch_rate_limit(headers: &Headers, name: &str) -> u32 {
    headers
        .get_raw(name)
        .and_then(Raw::one)
        .and_then(|one| from_utf8(one).ok())
        .and_then(|string| string.parse().ok())
        .unwrap_or(0)
}
