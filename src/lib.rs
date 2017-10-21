#![recursion_limit = "512"]

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
pub fn run() -> Result<()> {
    let args = Args::from_args();
    println!("Please open http://{}", args.address);
    serve(args)
}
