use reqwest::Url;
use serde::Serializer;
use std::net::SocketAddr;

#[derive(Debug, StructOpt, Serialize)]
pub struct Args {
    #[structopt(
        short = "t",
        long = "token",
        help = "GitHub token",
    )]
    pub token: String,

    #[structopt(
        long = "owner",
        help = "Repository owner",
        default_value = "rust-lang",
    )]
    pub owner: String,

    #[structopt(
        long = "repository",
        help = "Repository name",
        default_value = "rust",
    )]
    pub repository: String,

    #[structopt(
        long = "homu-queue-url",
        help = "URL to the Homu queue",
        default_value = "https://buildbot2.rust-lang.org/homu/queue/rust",
    )]
    #[serde(serialize_with = "serialize_url")]
    pub homu_url: Url,

    #[structopt(
        long = "homu-client-id",
        help = "Client ID of the Homu GitHub OAuth App",
        default_value = "f828d548f928f1e11199",
    )]
    pub homu_client_id: String,

    #[structopt(
        short = "l",
        long = "listen",
        help = "Address of local server",
        default_value = "127.0.0.1:55727",
    )]
    pub address: SocketAddr,

    #[structopt(
        short = "i",
        long = "templates",
        help = "Glob pattern to find Tera templates",
        default_value = "res/*.html",
    )]
    pub templates: String,

    #[structopt(
        short = "p",
        long = "proxy",
        help = "HTTP(S) proxy server",
    )]
    #[serde(skip_serializing)]
    pub proxy: Option<Url>,
}

fn serialize_url<S: Serializer>(url: &Url, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(url.as_str())
}
