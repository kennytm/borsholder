//! GitHub Timeline API access.

use failure::Error;
use futures::Future;
use github::{send_github_query, CacheKey};
use reqwest::unstable::async::Client;
use tera::Value;

/// Types related to the PR timeline GraphQL query.
pub mod graphql {
    #![cfg_attr(feature = "cargo-clippy", allow(missing_docs_in_private_items))]

    use github::graphql::Connection;

    /// The reply of a Timeline GraphQL query.
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
        pub pull_request: PullRequest,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PullRequest {
        pub timeline: Connection<::tera::Value>,
    }
}

/// A Timeline GraphQL request.
#[derive(Serialize)]
struct Request<'variables> {
    /// The query string.
    query: &'static str,
    /// Variables of the query.
    variables: Variables<'variables>,
}

/// Variables in a Timeline GraphQL request.
///
/// This structure is hard-coded to support the Timeline GraphQL query, `QUERY`.
#[derive(Serialize)]
struct Variables<'variables> {
    /// Owner of the repository.
    owner: &'variables str,
    /// Name of the repository.
    repo: &'variables str,
    /// PR number.
    number: u32,
}

impl<'a, 'v> From<&'a Request<'v>> for CacheKey {
    fn from(req: &'a Request<'v>) -> Self {
        CacheKey::Timeline(req.variables.number)
    }
}

/// The Timeline GraphQL query.
const QUERY: &str = include!("timeline.gql");

/// Fetch the most recent timeline of a pull request.
pub fn query(
    client: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    number: u32,
) -> Box<Future<Item = Vec<Value>, Error = Error>> {
    Box::new(
        send_github_query(
            client,
            token,
            &Request {
                query: QUERY,
                variables: Variables {
                    owner,
                    repo,
                    number,
                },
            },
        ).map(|reply: graphql::Reply| reply.data.repository.pull_request.timeline.nodes),
    )
}
