extern crate borsholder;

use std::process::exit;

fn main() {
    if let Err(e) = borsholder::run() {
        println!("{}", e);
        exit(1);
    }
}
