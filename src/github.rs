//! GitHub API access.

use errors::Result;
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
        pub updated_at: DateTime<Utc>,
        pub mergeable: MergeableState,
        pub number: u32,
        pub title: String,
        pub labels: Connection<Label>,
        pub commits: Connection<PullRequestCommit>,
        pub comments: Connection<IssueComment>,
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

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct IssueComment {
        pub database_id: u64,
        pub author: Actor,
        #[serde(rename = "bodyHTML")] pub body_html: String,
        pub published_at: DateTime<Utc>,
    }

    #[derive(Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum MergeableState {
        Unknown,
        Mergeable,
        Conflicting,
    }

    #[derive(Deserialize, Serialize)]
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
const QUERY: &str = r#"

query($owner: String!, $repo: String!) {
    repository(owner: $owner, name: $repo) {
        pullRequests(first: 100, states: [OPEN], orderBy: {field: UPDATED_AT, direction: DESC}) {
            nodes {
                author {
                    login
                }
                createdAt
                updatedAt
                mergeable
                number
                title
                labels(first: 10) {
                    nodes {
                        name
                        color
                    }
                }
                commits(last: 1) {
                    nodes {
                        commit {
                            committedDate
                            status {
                                contexts {
                                    context
                                    description
                                    targetUrl
                                    state
                                }
                            }
                        }
                    }
                }
                comments(last: 1) {
                    nodes {
                        databaseId
                        author {
                            login
                        }
                        bodyHTML
                        publishedAt
                    }
                }
            }
        }
    }
}

"#;

/// Obtains the list of open pull requests and associated information from GitHub.
pub fn query(
    client: &Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> Result<Vec<graphql::PullRequest>> {
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
