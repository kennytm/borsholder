use errors::Result;
use reqwest::Client;
use reqwest::header::{Authorization, Bearer};

pub mod graphql {
    use chrono::{DateTime, Utc};

    #[derive(Deserialize)]
    pub struct Connection<T> {
        pub nodes: Vec<T>,
    }

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
        pub status: Status,
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

#[derive(Serialize)]
struct Request<'variables> {
    query: &'static str,
    variables: Variables<'variables>,
}

#[derive(Serialize)]
struct Variables<'variables> {
    owner: &'variables str,
    repo: &'variables str,
}

const GITHUB_ENDPOINT: &str = "https://api.github.com/graphql";

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

pub fn query(
    client: &Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> Result<Vec<graphql::PullRequest>> {
    let reply = client
        .post(GITHUB_ENDPOINT)
        .header(Authorization(Bearer {
            token: token.to_owned(),
        }))
        .json(&Request {
            query: QUERY,
            variables: Variables { owner, repo },
        })
        .send()?
        .error_for_status()?
        .json::<graphql::Reply>()?;
    Ok(reply.data.repository.pull_requests.nodes)
}
