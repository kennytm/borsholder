//! Argument parsing

use reqwest::Url;
use serde::Serializer;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Stores the command line argument.
#[derive(Debug, StructOpt, Serialize)]
pub struct Args {
    /// The token to access the GitHub APIs.
    #[structopt(short = "t", long = "token", help = "GitHub token")]
    pub token: String,

    /// Owner of the GitHub repository.
    #[structopt(long = "owner", help = "Repository owner", default_value = "rust-lang")]
    pub owner: String,

    /// Name of the GitHub repository.
    #[structopt(long = "repository", help = "Repository name", default_value = "rust")]
    pub repository: String,

    /// URL to access the Homu queue.
    #[structopt(
        long = "homu-queue-url",
        help = "URL to the Homu queue",
        default_value = "https://buildbot2.rust-lang.org/homu/queue/rust"
    )]
    #[serde(serialize_with = "serialize_url")]
    pub homu_url: Url,

    /// Client ID of the Homu GitHub OAuth App.
    #[structopt(
        long = "homu-client-id",
        help = "Client ID of the Homu GitHub OAuth App",
        default_value = "f828d548f928f1e11199"
    )]
    pub homu_client_id: String,

    /// Socket address of the local web server.
    #[structopt(
        short = "l",
        long = "listen",
        help = "Address of local server",
        default_value = "127.0.0.1:55727"
    )]
    pub address: SocketAddr,

    /// Directory to find Tera templates and static resources
    #[structopt(
        short = "i",
        long = "templates",
        help = "Directory of the templates",
        default_value = "res",
        parse(from_os_str)
    )]
    #[serde(skip_serializing)]
    pub templates: PathBuf,

    /// HTTP(S) proxy server. If not `None`, all API requests will pass through this URL.
    #[structopt(short = "p", long = "proxy", help = "HTTP(S) proxy server")]
    #[serde(skip_serializing)]
    pub proxy: Option<Url>,
}

/// Serializes a URL using serde.
fn serialize_url<S: Serializer>(url: &Url, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(url.as_str())
}
