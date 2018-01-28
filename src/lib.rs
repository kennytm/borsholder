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

extern crate antidote;
extern crate chrono;
extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate futures;
extern crate hyper;
extern crate kuchiki;
#[macro_use]
extern crate lazy_static;
extern crate libflate;
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

mod args;
mod github;
mod homu;
mod render;
mod server;

use args::Args;
use env_logger::{Builder, Env};
use failure::Error;
use server::serve;
use structopt::StructOpt;
use std::io::Write;

/// Runs the borsholder CLI.
#[cfg_attr(feature = "cargo-clippy", allow(print_stdout))]
pub fn run() -> Result<(), Error> {
    init_logger();
    let args = Args::from_args();
    println!("Please open http://{}", args.address);
    serve(args)
}

/// Initializes the logger via the `RUST_LOG` variable. See documentation of
/// [`env_logger`] for syntax.
///
/// [`env_logger`]: https://docs.rs/crate/env_logger/0.5.3
fn init_logger() {
    Builder::from_env(Env::default())
        .format(|buf, record| {
            let timestamp = buf.timestamp();
            writeln!(
                buf,
                "[{}][{}]: {}",
                record.level(),
                timestamp,
                record.args()
            )
        })
        .init();
}
