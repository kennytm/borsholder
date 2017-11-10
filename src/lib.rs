//! borsholder
//! ==========
//!
//! **borsholder** is a dashboard for monitoring the [Rust compiler repository]'s pull requests
//! status. It is a combination of rust-lang/rust's [Homu queue] with useful information (labels,
//! last comment, CI status, etc) obtained from the GitHub API.
//!
//! See the [README] for usage.
//!
//! [Rust repository]: https://github.com/rust-lang/rust
//! [Homu queue]: https://buildbot2.rust-lang.org/homu/queue/rust
//! [README]: https://github.com/kennytm/borsholder

#![cfg_attr(feature = "cargo-clippy", warn(warnings, clippy_pedantic))]

extern crate ammonia;
extern crate antidote;
extern crate chrono;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate hyper;
extern crate kuchiki;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate maplit;
#[macro_use]
extern crate markup5ever;
extern crate mime;
extern crate regex;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
#[macro_use]
extern crate tera;

pub mod errors;
mod args;
mod github;
mod homu;
mod render;
mod server;

use args::Args;
use chrono::Local;
use env_logger::LogBuilder;
use errors::Result;
use server::serve;
use std::env::var;
use structopt::StructOpt;

/// Runs the borsholder CLI.
#[cfg_attr(feature = "cargo-clippy", allow(print_stdout))]
pub fn run() -> Result<()> {
    init_logger();
    let args = Args::from_args();
    println!("Please open http://{}", args.address);
    serve(args)
}

/// Initializes the logger via the `RUST_LOG` variable. See documentation of
/// [`env_logger`] for syntax.
///
/// [`env_logger`]: https://docs.rs/crate/env_logger/0.4.3
fn init_logger() {
    let mut log_builder = LogBuilder::new();
    log_builder.format(|record| {
        format!("[{}][{}]: {}", record.level(), Local::now(), record.args())
    });
    if let Ok(filters) = var("RUST_LOG") {
        log_builder.parse(&filters);
    }
    log_builder.init().expect("set logger");
}
