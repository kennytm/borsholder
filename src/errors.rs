// Temporary workaround until tera updated to error-chain 0.11.
#![allow(unused_doc_comment)]

error_chain! {
    links {
        Tera(::tera::Error, ::tera::ErrorKind);
    }

    foreign_links {
        Io(::std::io::Error);
        Reqwest(::reqwest::Error);
        Hyper(::hyper::Error);
    }
}
