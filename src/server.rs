//! Local server of borsholder.

use args::Args;
use error_chain::ChainedError;
use errors::Result;
use futures::future::{ok, FutureResult};
use hyper::{Error, StatusCode};
use hyper::header::{CacheControl, ContentType};
use hyper::header::CacheDirective::{MaxAge, Public};
use hyper::server::{Http, Request, Response, Service};
use render::{parse_prs, register_tera_filters, summarize_prs, Pr, PrStats};
use reqwest::{Client, Proxy};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::thread::sleep;
use std::time::Duration;
use std::fs::File;
use std::io::Read;
use std::ffi::OsStr;
use tera::Tera;
use ttl::Cache;
use mime::{Mime, TEXT_CSS, TEXT_JAVASCRIPT};
use regex::bytes::Regex;

/// Serves the borsholder web page configured according to `args`.
///
/// This method will not return until the server is shutdown.
pub fn serve(mut args: Args) -> Result<()> {
    let tera_pattern_os = args.templates.join("*.html").into_os_string();
    let tera_pattern = tera_pattern_os.to_string_lossy();
    let mut tera = Tera::new(&tera_pattern)?;
    register_tera_filters(&mut tera);
    let mut builder = Client::builder();
    if let Some(proxy) = args.proxy.take() {
        builder.proxy(Proxy::all(proxy)?);
    }
    let client = builder.timeout(Duration::from_secs(120)).build()?;
    let address = args.address;

    let handler = Rc::new(Handler {
        tera: RefCell::new(tera),
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
    tera: RefCell<Tera>,
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

    fn call(&self, request: Request) -> Self::Future {
        let uri = request.uri();
        debug!("Received request to {}", uri);

        let mut response = Response::new();
        if let Err(e) = self.serve(uri.path(), &mut response) {
            response.set_status(StatusCode::InternalServerError);
            response.headers_mut().set(ContentType::plaintext());
            response.set_body(e.display().to_string());
        }

        debug!("Responding with {}", response.status());
        ok(response)
    }
}

lazy_static! {
    static ref SAFE_PATH_RE: Regex = Regex::new(r"^/static/[^\\/]+$").expect("safe path regex");
    static ref KNOWN_CONTENT_TYPES: HashMap<&'static str, Mime> = hashmap![
        "css" => TEXT_CSS,
        "js" => TEXT_JAVASCRIPT,
    ];
}

impl Handler {
    /// Serves a response from the URL.
    fn serve(&self, path: &str, response: &mut Response) -> Result<()> {
        match path {
            "/" => {
                let body = self.render()?;
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::html());
                response.set_body(body);
            }
            "/reloadTemplates" => {
                self.reload_templates()?;
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::plaintext());
                response.set_body("reloaded");
            }
            _ => {
                response.set_status(StatusCode::NotFound);
                if SAFE_PATH_RE.is_match(path.as_bytes()) {
                    let path = self.args.templates.join(&path[1..]);
                    if path.exists() {
                        let mime = path.extension()
                            .and_then(OsStr::to_str)
                            .and_then(|ext| KNOWN_CONTENT_TYPES.get(ext));

                        let mut file = File::open(path)?;
                        let mut body = Vec::new();
                        file.read_to_end(&mut body)?;
                        drop(file);

                        response.set_status(StatusCode::Ok);
                        {
                            let headers = response.headers_mut();
                            headers.set(CacheControl(vec![Public, MaxAge(31_536_000)]));
                            if let Some(mime) = mime {
                                headers.set(ContentType(mime.clone()));
                            }
                        }
                        response.set_body(body);
                    }
                }
            }
        }
        Ok(())
    }

    /// Renders the web page.
    ///
    /// This method will *synchronously* download PR information from GitHub and Homu.
    fn render(&self) -> Result<String> {
        let args = &self.args;
        let mut prs_cache = self.api_cache.borrow_mut();
        let prs = prs_cache.get_or_refresh(Duration::from_secs(60), || -> Result<_> {
            // TODO: Reduce the number of retries when
            // https://platform.github.community/t/sporadic-502s-fetching-pr-data/3024 is fixed.
            let github = retry(6, Duration::from_secs(7), || {
                ::github::query(&self.client, &args.token, &args.owner, &args.repository)
            })?;
            let homu = ::homu::query(&self.client, &self.args.homu_url)?;
            Ok(parse_prs(github, homu))
        })?;
        let stats = summarize_prs(prs.values());
        let data = RenderData { prs, stats, args };
        let body = self.tera.borrow().render("index.html", &data)?;
        Ok(body)
    }

    /// Reloads the Tera template.
    fn reload_templates(&self) -> Result<()> {
        let mut tera = self.tera.borrow_mut();
        tera.full_reload()?;
        Ok(())
    }
}

/// Performs an `action` and retry on failure.
///
/// If the `action` returns `Ok`, it will be propagated immediately. Otherwise, it will sleep for
/// `idle_duration` and then performs the `action` again. The error will be returned after `count`
/// total tries.
fn retry<T, F>(count: u32, idle_duration: Duration, mut action: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut last_error = None;
    for i in 0..count {
        if i != 0 {
            if let Some(ref e) = last_error {
                debug!("retry {}/{}; last attempt failed with {}", i, count, e);
            }
            sleep(idle_duration);
        }
        match action() {
            Ok(value) => return Ok(value),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.expect("count > 0"))
}
