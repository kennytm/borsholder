//! Local server of borsholder.

use args::Args;
use failure::Error;
use futures::Stream;
use futures::future::{empty, result, Future};
use hyper::{self, StatusCode};
use hyper::header::{AcceptEncoding, CacheControl, Connection, ConnectionOption, ContentEncoding,
                    ContentType, Encoding, Headers};
use hyper::header::CacheDirective::{MaxAge, Public};
use hyper::server::{Http, Request, Response, Service};
use libflate::gzip::Encoder;
use mime::{Mime, IMAGE_PNG, TEXT_CSS, TEXT_JAVASCRIPT};
use regex::bytes::Regex;
use render::{parse_prs, register_tera_filters, summarize_prs, Pr, PrStats, TeraFailure};
use reqwest::unstable::async::Client;
use reqwest::Proxy;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::rc::Rc;
use std::time::Duration;
use tera::Tera;
use tokio_core::reactor::Core;

/// Serves the borsholder web page configured according to `args`.
///
/// This method will not return until the server is shutdown.
pub fn serve(mut args: Args) -> Result<(), Error> {
    let tera_pattern_os = args.templates.join("*.html").into_os_string();
    let tera_pattern = tera_pattern_os.to_string_lossy();
    let mut tera = Tera::new(&tera_pattern).map_err(TeraFailure::from)?;
    register_tera_filters(&mut tera);

    let mut core = Core::new()?;
    let handle = core.handle();

    let mut builder = Client::builder();
    if let Some(proxy) = args.proxy.take() {
        builder.proxy(Proxy::all(proxy)?);
    }
    let mut default_headers = Headers::new();
    default_headers.set(Connection(vec![ConnectionOption::Close]));
    let client = builder
        .timeout(Duration::from_secs(120))
        .default_headers(default_headers)
        .build(&handle)?;

    let address = args.address;
    let handler = Rc::new(Handler {
        tera: Rc::new(RefCell::new(tera)),
        client,
        args: Rc::new(args),
    });

    let serve = Http::new().serve_addr_handle(&address, &handle, move || Ok(Rc::clone(&handler)))?;

    let conn_handle = handle.clone();
    handle.spawn(
        serve
            .for_each(move |conn| {
                conn_handle.spawn(
                    conn.map(|_| ())
                        .map_err(|e| debug!("server error: {}", e)),
                );
                Ok(())
            })
            .map_err(|_| ()),
    );

    core.run(empty::<(), Error>())?;
    Ok(())
}

/// Request handler of the borsholder server.
struct Handler {
    /// The Tera template engine.
    tera: Rc<RefCell<Tera>>,
    /// The reqwest client for making API requests.
    client: Client,
    /// The command line arguments.
    args: Rc<Args>,
}

/// Packaged JSON-like object to be sent to Tera for rendering.
#[derive(Serialize)]
struct RenderData {
    /// The list of PRs.
    prs: HashMap<u32, Pr>,
    /// PR statistics.
    stats: PrStats,
    /// The command line arguments.
    args: Rc<Args>,
}

impl Service for Handler {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response, Error = hyper::Error>>;

    fn call(&self, request: Request) -> Self::Future {
        let uri = request.uri();
        debug!("Received request to {}", uri);

        let encodings = request.headers().get::<AcceptEncoding>();
        let can_gzip = encodings.map_or(false, |ae| ae.iter().any(|q| q.item == Encoding::Gzip));

        Box::new(
            self.serve(uri.path(), can_gzip)
                .or_else(|e| {
                    let mut response = Response::new();
                    response.set_status(StatusCode::InternalServerError);
                    response.headers_mut().set(ContentType::plaintext());
                    response.set_body(e.to_string());
                    Ok(response)
                })
                .inspect(|response| debug!("Responding with {}", response.status())),
        )
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
}

impl Handler {
    /// Serves a response from the URL.
    fn serve(&self, path: &str, can_gzip: bool) -> Box<Future<Item = Response, Error = Error>> {
        match path {
            "/" => {
                let future = self.render().and_then(move |body| {
                    let mut response = Response::new();
                    response.set_status(StatusCode::Ok);
                    response.headers_mut().set(ContentType::html());
                    set_response_body(&mut response, body.as_bytes(), can_gzip)?;
                    Ok(response)
                });
                Box::new(future)
            }
            _ => Box::new(result(self.serve_sync(path, can_gzip))),
        }
    }

    /// Serves a response which doesn't require asynchronous requests.
    fn serve_sync(&self, path: &str, can_gzip: bool) -> Result<Response, Error> {
        let mut response = Response::new();
        match path {
            "/reloadTemplates" => {
                self.reload_templates()?;
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::plaintext());
                response.set_body("reloaded");
            }
            "/sync" => {
                response.set_status(StatusCode::Ok);
                response.headers_mut().set(ContentType::plaintext());
                response.set_body("synchronization request sent, go back and refresh.");
            }
            _ => {
                response.set_status(StatusCode::NotFound);
                if SAFE_PATH_RE.is_match(path.as_bytes()) {
                    let path = self.args.templates.join(&path[1..]);
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
                        set_response_body(&mut response, file, can_gzip)?;
                    }
                }
            }
        }
        Ok(response)
    }

    /// Renders the web page.
    ///
    /// This method will *asynchronously* download PR information from GitHub and Homu.
    fn render(&self) -> Box<Future<Item = String, Error = Error>> {
        let args = Rc::clone(&self.args);
        let tera = Rc::clone(&self.tera);

        let homu_future = ::homu::query(&self.client, &args.homu_url);
        let github_future = ::github::query(
            self.client.clone(),
            args.token.clone(),
            args.owner.clone(),
            args.repository.clone(),
        );
        Box::new(
            homu_future
                .join(github_future)
                .and_then(move |(homu, github)| {
                    let prs = parse_prs(github, homu);
                    let stats = summarize_prs(prs.values());
                    let data = RenderData { prs, stats, args };
                    let body = tera.borrow()
                        .render("index.html", &data)
                        .map_err(TeraFailure::from)?;
                    Ok(body)
                }),
        )
    }

    /// Reloads the Tera template.
    fn reload_templates(&self) -> Result<(), Error> {
        let mut tera = self.tera.borrow_mut();
        tera.full_reload().map_err(TeraFailure::from)?;
        Ok(())
    }
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
