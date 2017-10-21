use args::Args;
use error_chain::ChainedError;
use errors::Result;
use futures::future::{ok, FutureResult};
use hyper::{Error, StatusCode};
use hyper::header::ContentType;
use hyper::server::{Http, Request, Response, Service};
use render::{parse_prs, register_tera_filters, summarize_prs, Pr, PrStats};
use reqwest::{Client, Proxy};
use std::collections::HashMap;
use std::rc::Rc;
use tera::Tera;

pub fn serve(mut args: Args) -> Result<()> {
    let mut tera = Tera::new(&args.templates)?;
    register_tera_filters(&mut tera);
    let mut builder = Client::builder();
    if let Some(proxy) = args.proxy.take() {
        builder.proxy(Proxy::all(proxy)?);
    }
    let client = builder.build()?;
    let address = args.address;

    let handler = Rc::new(Handler { tera, client, args });
    let server = Http::new().bind(&address, move || Ok(Rc::clone(&handler)))?;
    server.run()?;

    Ok(())
}

struct Handler {
    tera: Tera,
    client: Client,
    args: Args,
}

impl Service for Handler {
    type Request = Request;
    type Response = Response;
    type Error = Error;
    type Future = FutureResult<Response, Error>;

    fn call(&self, _: Request) -> Self::Future {
        let mut response = Response::new();
        match render(&self.tera, &self.client, &self.args) {
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

#[derive(Serialize)]
struct RenderData<'a> {
    prs: HashMap<u32, Pr>,
    stats: PrStats,
    args: &'a Args,
}

fn render(tera: &Tera, client: &Client, args: &Args) -> Result<String> {
    let github_entries = ::github::query(client, &args.token, &args.owner, &args.repository)?;
    let homu_entries = ::homu::query(client, &args.homu_url)?;
    let prs = parse_prs(github_entries, homu_entries);
    let stats = summarize_prs(prs.values());
    let body = tera.render("index.html", &RenderData { prs, stats, args })?;
    Ok(body)
}
