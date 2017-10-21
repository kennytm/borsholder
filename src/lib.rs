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
extern crate chrono;
extern crate chrono_humanize;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate hyper;
extern crate kuchiki;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate maplit;
#[macro_use]
extern crate markup5ever;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tera;

pub mod errors;
mod args;
mod github;
mod homu;
mod render;
mod server;

use args::Args;
use errors::Result;
use server::serve;
use structopt::StructOpt;

/// Runs the borsholder CLI.
#[cfg_attr(feature = "cargo-clippy", allow(print_stdout))]
pub fn run() -> Result<()> {
    let args = Args::from_args();
    println!("Please open http://{}", args.address);
    serve(args)
}
