//! Local server of borsholder.

use antidote::{Condvar, Mutex, MutexGuard};
use args::Args;
use chrono::{DateTime, Utc};
use failure::Error;
use futures::future::{ok, FutureResult};
use hyper::{self, StatusCode};
use hyper::header::{AcceptEncoding, CacheControl, Connection, ConnectionOption, ContentEncoding,
                    ContentType, Encoding, Headers};
use hyper::header::CacheDirective::{MaxAge, Public};
use hyper::server::{Http, Request, Response, Service};
use libflate::gzip::Encoder;
use mime::{Mime, IMAGE_PNG, TEXT_CSS, TEXT_JAVASCRIPT};
use regex::bytes::Regex;
use render::{parse_prs, register_tera_filters, summarize_prs, Pr, PrStats, TeraFailure};
use reqwest::{Client, Proxy};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};
use tera::Tera;

/// Serves the borsholder web page configured according to `args`.
///
/// This method will not return until the server is shutdown.
pub fn serve(mut args: Args) -> Result<(), Error> {
    let tera_pattern_os = args.templates.join("*.html").into_os_string();
    let tera_pattern = tera_pattern_os.to_string_lossy();
    let mut tera = Tera::new(&tera_pattern).map_err(TeraFailure::from)?;
    register_tera_filters(&mut tera);

    let mut builder = Client::builder();
    if let Some(proxy) = args.proxy.take() {
        builder.proxy(Proxy::all(proxy)?);
    }
    let mut default_headers = Headers::new();
    default_headers.set(Connection(vec![ConnectionOption::Close]));
    let client = builder
        .timeout(Duration::from_secs(120))
        .default_headers(default_headers)
        .build()?;

    let address = args.address;
    let is_server_dead = Mutex::new(false);
    let is_server_dead_condition = Condvar::new();

    let context = Arc::new(Context {
        client,
        args,
        is_server_dead,
        is_server_dead_condition,
    });
    let server_context = Arc::clone(&context);
    let github_context = Arc::clone(&context);

    let github_thread = thread::Builder::new()
        .name("GitHub".to_owned())
        .spawn(move || load_from_github(&github_context))?;

    let handler = Rc::new(Handler {
        tera: RefCell::new(tera),
        context: server_context,
    });
    let server = Http::new().bind(&address, move || Ok(Rc::clone(&handler)))?;
    server.run()?;

    {
        *context.is_server_dead.lock() = true;
        context.is_server_dead_condition.notify_all();
    }

    github_thread.join().expect("GitHub thread is complete");

    Ok(())
}

/// Shared context between the web server and the worker threads.
struct Context {
    /// The reqwest client for making API requests.
    client: Client,
    /// The command line arguments.
    args: Args,
    /// Whether the server is still running. When this value is false, all
    /// worker threads should stop as soon as possible.
    is_server_dead: Mutex<bool>,
    /// The condition variable operating on the `is_server_dead` mutex.
    is_server_dead_condition: Condvar,
}

/// Request handler of the borsholder server.
struct Handler {
    /// The Tera template engine.
    tera: RefCell<Tera>,
    /// Shared context with the worker threads.
    context: Arc<Context>,
}

/// Packaged JSON-like object to be sent to Tera for rendering.
#[derive(Serialize)]
struct RenderData<'a> {
    /// The list of PRs.
    prs: &'a HashMap<u32, Pr<'a>>,
    /// PR statistics.
    stats: PrStats,
    /// The command line arguments.
    args: &'a Args,
    /// Last update time for GitHub.
    github_last_update: DateTime<Utc>,
}

impl Service for Handler {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Response, hyper::Error>;

    fn call(&self, request: Request) -> Self::Future {
        let uri = request.uri();
        debug!("Received request to {}", uri);

        let mut response = Response::new();

        let encodings = request.headers().get::<AcceptEncoding>();
        let can_gzip = encodings.map_or(false, |ae| ae.iter().any(|q| q.item == Encoding::Gzip));

        if let Err(e) = self.serve(uri.path(), &mut response, can_gzip) {
            response.set_status(StatusCode::InternalServerError);
            response.headers_mut().set(ContentType::plaintext());
            response.set_body(e.to_string());
        }

        debug!("Responding with {}", response.status());
        ok(response)
    }
}

/// Represent a cached value that is shared across threads, containing the last
/// update time.
struct Cache<T>(Mutex<(T, DateTime<Utc>)>);

impl<T: Default> Default for Cache<T> {
    fn default() -> Self {
        Cache(Mutex::new((T::default(), UNIX_EPOCH.into())))
    }
}

impl<T> Cache<T> {
    /// Sets a value to the cache.
    fn set(&self, value: T) {
        let now = Utc::now();
        *self.0.lock() = (value, now);
    }

    /// Gets the value and last update time from the cache.
    fn lock(&self) -> MutexGuard<(T, DateTime<Utc>)> {
        self.0.lock()
    }
}

lazy_static! {
    /// The regex which represents path can be used for static resource.
    static ref SAFE_PATH_RE: Regex = Regex::new(r"^/static/[^\\/]+$").expect("safe path regex");
    /// A hash map of file extension to their media types.
    static ref KNOWN_CONTENT_TYPES: HashMap<&'static str, Mime> = hashmap![
        "css" => TEXT_CSS,
        "js" => TEXT_JAVASCRIPT,
        "png" => IMAGE_PNG,
    ];

    /// The cached GitHub API request result.
    static ref GITHUB_ENTRIES: Cache<Vec<::github::graphql::PullRequest>> = Cache::default();
}

impl Handler {
    /// Serves a response from the URL.
    fn serve(&self, path: &str, response: &mut Response, can_gzip: bool) -> Result<(), Error> {
        match path {
            "/" => {
                let body = self.render()?;
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::html());
                set_response_body(response, body.as_bytes(), can_gzip)?;
            }
            "/reloadTemplates" => {
                self.reload_templates()?;
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::plaintext());
                response.set_body("reloaded");
            }
            "/sync" => {
                self.context.is_server_dead_condition.notify_all();
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::plaintext());
                response.set_body("synchronization request sent, go back and refresh.");
            }
            _ => {
                response.set_status(StatusCode::NotFound);
                if SAFE_PATH_RE.is_match(path.as_bytes()) {
                    let path = self.context.args.templates.join(&path[1..]);
                    if path.exists() {
                        let mime = path.extension()
                            .and_then(OsStr::to_str)
                            .and_then(|ext| KNOWN_CONTENT_TYPES.get(ext));

                        response.set_status(StatusCode::Ok);
                        {
                            let headers = response.headers_mut();
                            headers.set(CacheControl(vec![Public, MaxAge(31_536_000)]));
                            if let Some(mime) = mime {
                                headers.set(ContentType(mime.clone()));
                            }
                        }

                        let file = File::open(path)?;
                        set_response_body(response, file, can_gzip)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Renders the web page.
    ///
    /// This method will *synchronously* download PR information from GitHub and Homu.
    fn render(&self) -> Result<String, Error> {
        let args = &self.context.args;
        let homu = ::homu::query(&self.context.client, &args.homu_url)?;

        let github_guard = GITHUB_ENTRIES.lock();

        let prs = parse_prs(&github_guard.0, &homu);
        let stats = summarize_prs(prs.values());
        let data = RenderData {
            prs: &prs,
            stats,
            args,
            github_last_update: github_guard.1,
        };

        let body = self.tera
            .borrow()
            .render("index.html", &data)
            .map_err(TeraFailure::from)?;
        Ok(body)
    }

    /// Reloads the Tera template.
    fn reload_templates(&self) -> Result<(), Error> {
        let mut tera = self.tera.borrow_mut();
        tera.full_reload().map_err(TeraFailure::from)?;
        Ok(())
    }
}

/// Common routine for a background worker thread.
///
/// The `query` function will be executed periodically. On success, it will
/// write the result into the `output` cache.
fn worker_thread<T, F: Fn() -> Result<T, Error>>(context: &Context, output: &Cache<T>, query: F) {
    loop {
        let sleep_duration = match query() {
            Ok(entries) => {
                output.set(entries);
                context.args.refresh_interval
            }
            Err(e) => {
                debug!(
                    "Query for {} failed: {}",
                    thread::current().name().unwrap_or("<unnamed>"),
                    e
                );
                context.args.retry_interval
            }
        };

        let guard = context.is_server_dead.lock();
        if *guard {
            break;
        }
        context
            .is_server_dead_condition
            .wait_timeout(guard, sleep_duration);
    }
}

/// Worker thread for loading data from GitHub.
fn load_from_github(context: &Context) {
    worker_thread(context, &GITHUB_ENTRIES, || {
        ::github::query(
            &context.client,
            &context.args.token,
            &context.args.owner,
            &context.args.repository,
        )
    });
}

/// Sets the response's body with optional compression.
///
/// If `can_gzip` is true, the body will be gzip-compressed, and the corresponding
/// `Content-Encoding: gzip` header will be added to the response.
fn set_response_body<R: Read>(
    response: &mut Response,
    mut body: R,
    can_gzip: bool,
) -> io::Result<()> {
    let mut compressed = Vec::new();
    if can_gzip {
        let mut encoder = Encoder::new(compressed)?;
        io::copy(&mut body, &mut encoder)?;
        compressed = encoder.finish().into_result()?;
        response
            .headers_mut()
            .set(ContentEncoding(vec![Encoding::Gzip]));
    } else {
        body.read_to_end(&mut compressed)?;
    }
    response.set_body(compressed);
    Ok(())
}
