//! Local server of borsholder.

use args::Args;
use error_chain::ChainedError;
use errors::Result;
use futures::future::{ok, FutureResult};
use hyper::{Error, StatusCode};
use hyper::header::ContentType;
use hyper::server::{Http, Request, Response, Service};
use render::{parse_prs, register_tera_filters, summarize_prs, Pr, PrStats};
use reqwest::{Client, Proxy};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tera::Tera;
use ttl::Cache;

/// Serves the borsholder web page configured according to `args`.
///
/// This method will not return until the server is shutdown.
pub fn serve(mut args: Args) -> Result<()> {
    let mut tera = Tera::new(&args.templates)?;
    register_tera_filters(&mut tera);
    let mut builder = Client::builder();
    if let Some(proxy) = args.proxy.take() {
        builder.proxy(Proxy::all(proxy)?);
    }
    let client = builder.build()?;
    let address = args.address;

    let handler = Rc::new(Handler {
        tera,
        client,
        args,
        api_cache: RefCell::new(Cache::new()),
    });
    let server = Http::new().bind(&address, move || Ok(Rc::clone(&handler)))?;
    server.run()?;

    Ok(())
}

/// Request handler of the borsholder server.
struct Handler {
    /// The Tera template engine.
    tera: Tera,
    /// The reqwest client for making API requests.
    client: Client,
    /// The command line arguments.
    args: Args,
    /// The cached API response.
    api_cache: RefCell<Cache<HashMap<u32, Pr>>>,
}

/// Packaged JSON-like object to be sent to Tera for rendering.
#[derive(Serialize)]
struct RenderData<'a> {
    /// The list of PRs.
    prs: &'a HashMap<u32, Pr>,
    /// PR statistics.
    stats: PrStats,
    /// The command line arguments.
    args: &'a Args,
}

impl Service for Handler {
    type Request = Request;
    type Response = Response;
    type Error = Error;
    type Future = FutureResult<Response, Error>;

    fn call(&self, _: Request) -> Self::Future {
        let mut response = Response::new();
        match self.render() {
            Ok(body) => {
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::html());
                response.set_body(body);
            }
            Err(e) => {
                response.set_status(StatusCode::InternalServerError);
                response.set_body(e.display().to_string());
            }
        }
        ok(response)
    }
}

impl Handler {
    /// Renders the web page.
    ///
    /// This method will *synchronously* download PR information from GitHub and Homu.
    fn render(&self) -> Result<String> {
        let args = &self.args;
        let mut prs_cache = self.api_cache.borrow_mut();
        let prs = prs_cache.get_or_refresh(Duration::from_secs(5), || -> Result<_> {
            let github = ::github::query(&self.client, &args.token, &args.owner, &args.repository)?;
            let homu = ::homu::query(&self.client, &self.args.homu_url)?;
            Ok(parse_prs(github, homu))
        })?;
        let stats = summarize_prs(prs.values());
        let data = RenderData { prs, stats, args };
        let body = self.tera.render("index.html", &data)?;
        Ok(body)
    }
}
