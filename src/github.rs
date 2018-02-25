//! GitHub API access.

use failure::Error;
use reqwest::unstable::async::Client;
use reqwest::header::{Authorization, Bearer, Headers, Raw};
use std::str::from_utf8;
use futures::future::Future;
use futures::stream::{unfold, Stream};
use serde::ser::Serialize;
use serde::de::DeserializeOwned;

/// Types related to the main GraphQL query.
///
/// Please see [GitHub's GraphQL schema] for details.
///
/// [GitHub's GraphQL schema]: https://developer.github.com/v4/reference/query/
pub mod graphql {
    #![cfg_attr(feature = "cargo-clippy", allow(missing_docs_in_private_items))]

    use chrono::{DateTime, Utc};

    /// A generic GraphQL connection, which is the same as a vector in our use case.
    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct Connection<T> {
        /// List of nodes in this connection.
        pub nodes: Vec<T>,
        /// Paging information about this connection.
        #[serde(default)]
        pub page_info: PageInfo,
    }

    /// Paging information about a GraphQL connection.
    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct PageInfo {
        /// The cursor beyond the end of all data presented in this connection.
        pub end_cursor: String,
        /// Whether a new page exists.
        pub has_next_page: bool,
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
        pub additions: u32,
        pub deletions: u32,
        pub labels: Connection<Label>,
        pub commits: Connection<PullRequestCommit>,
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
    /// Only read the content after
    after: Option<&'variables str>,
}

/// URL to send the GraphQL requests.
const GITHUB_ENDPOINT: &str = "https://api.github.com/graphql";

/// The main GraphQL query.
const QUERY: &str = include!("github.gql");

/// Pagination status for multi-page results (pull request list).
enum PaginationState {
    /// This is the first page. Used to initialize the requests.
    First,
    /// There are more pages after this request.
    HasNext(String),
    /// This is the last page.
    Done,
}

impl PaginationState {
    /// Whether the last page has been reached.
    fn is_done(&self) -> bool {
        match *self {
            PaginationState::Done => true,
            _ => false,
        }
    }

    /// Extracts the cursor ID which can be used to load the next page.
    fn as_after(&self) -> Option<&str> {
        match *self {
            PaginationState::HasNext(ref s) => Some(&**s),
            _ => None,
        }
    }
}

/// Obtains the list of open pull requests and associated information from GitHub.
pub fn query(
    client: Client,
    token: String,
    owner: String,
    repo: String,
) -> Box<Future<Item = Vec<graphql::PullRequest>, Error = Error>> {
    Box::new(
        unfold(PaginationState::First, move |next_page| {
            if next_page.is_done() {
                None
            } else {
                Some(query_single_page(
                    &client,
                    &token,
                    &owner,
                    &repo,
                    next_page.as_after(),
                ))
            }
        }).concat2(),
    )
}

/// Sends a generic GitHub GraphQL query.
pub(super) fn send_github_query<R: DeserializeOwned + 'static, T: Serialize + ?Sized>(
    client: &Client,
    token: &str,
    request: &T,
) -> Box<Future<Item = R, Error = Error>> {
    info!("Preparing to send GitHub request");
    Box::new(
        client
            .post(GITHUB_ENDPOINT)
            .header(Authorization(Bearer {
                token: token.to_owned(),
            }))
            .json(request)
            .send()
            .and_then(|response| response.error_for_status())
            .inspect(|response| {
                let headers = response.headers();
                let rate_limit_remaining = fetch_rate_limit(headers, "X-RateLimit-Remaining");
                let rate_limit_limit = fetch_rate_limit(headers, "X-RateLimit-Limit");
                info!(
                    "GitHub rate limit: {}/{}",
                    rate_limit_remaining, rate_limit_limit
                );
            })
            .and_then(|mut response| response.json::<R>())
            .map_err(Error::from),
    )
}

/// Obtains a single page of open pull requests and associated information from GitHub.
fn query_single_page(
    client: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    after: Option<&str>,
) -> Box<Future<Item = (Vec<graphql::PullRequest>, PaginationState), Error = Error>> {
    Box::new(
        send_github_query(
            client,
            token,
            &Request {
                query: QUERY,
                variables: Variables { owner, repo, after },
            },
        ).map(|reply: graphql::Reply| {
            let prs = reply.data.repository.pull_requests.nodes;
            let next_page = match reply.data.repository.pull_requests.page_info {
                graphql::PageInfo {
                    has_next_page: true,
                    end_cursor,
                } => PaginationState::HasNext(end_cursor),
                graphql::PageInfo {
                    has_next_page: false,
                    ..
                } => PaginationState::Done,
            };
            info!(
                "Obtained {} PRs from GitHub, has next page = {}",
                prs.len(),
                !next_page.is_done()
            );
            (prs, next_page)
        }),
    )
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
