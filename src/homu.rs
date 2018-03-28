//! Home queue web scraper.

use failure::{err_msg, Error, ResultExt};
use futures::{Future, Stream};
use kuchiki::parse_html;
use kuchiki::traits::TendrilSink;
use reqwest::Url;
use reqwest::unstable::async::Client;
use tendril::Tendril;

/// An entry in the Homu queue.
pub struct Entry {
    /// Pull request number.
    pub number: u32,
    /// Pull request title.
    pub title: String,
    /// Approval status.
    pub status: Status,
    /// Whether the approval status applies to a "try" run.
    pub is_trying: bool,
    /// Priority. Rollups are always assigned a priority of `-1`.
    pub priority: i32,
    /// Name of approver
    pub approver: String,
}

/// The approval status of a pull request in the Homu queue.
#[derive(Serialize, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    /// CI reported success, waiting for reviewer's further action.
    Success,
    /// The pull request is sent to the CI, and is waiting for test result.
    Pending,
    /// The pull request has been approved, waiting to be tested.
    Approved,
    /// The pull request is being actively reviewed.
    Reviewing,
    /// Error while testing the pull request, likely due to merge conflict.
    Error,
    /// CI reported failure.
    Failure,
}

impl Default for Status {
    fn default() -> Self {
        Status::Reviewing
    }
}

/// Obtains the list of pull requests and associated information from Homu queue.
pub fn query(client: &Client, url: &Url) -> Box<Future<Item = Vec<Entry>, Error = Error>> {
    info!("Preparing to send Homu request");

    Box::new(
        client
            .get(url.clone())
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| {
                response
                    .into_body()
                    .fold(parse_html().from_utf8(), |mut parser, chunk| {
                        parser.process(Tendril::from_slice(&*chunk));
                        Ok::<_, ::reqwest::Error>(parser)
                    })
            })
            .map(|parser| parser.finish())
            .map_err(Error::from)
            .and_then(|doc| {
                let mut res = Vec::new();
                for tr in doc.select("#queue > tbody > tr")
                    .expect("well-formed CSS query")
                {
                    let mut tds = tr.as_node()
                        .children()
                        .filter_map(|td| {
                            if let Some(elem) = td.as_element() {
                                if elem.name.expanded() == expanded_name!(html "td") {
                                    return Some(td.text_contents());
                                }
                            }
                            None
                        })
                        .collect::<Vec<_>>();

                    if tds.len() != 10 {
                        return Err(err_msg("Homu queue structure probably changed. Aborting."));
                    }

                    let number = tds[2].parse::<u32>().context("invalid PR number")?;
                    let (status, is_trying) = parse_status(&tds[3]);
                    let priority = parse_priority(&tds[9]);
                    let approver = tds.swap_remove(8);
                    let title = tds.swap_remove(5);

                    res.push(Entry {
                        number,
                        title,
                        status,
                        is_trying,
                        priority,
                        approver,
                    });
                }

                info!("Obtained {} PRs from Homu", res.len());
                Ok(res)
            }),
    )
}

/// Parses the rendered approval status string into the status/is-try pair.
fn parse_status(status_str: &str) -> (Status, bool) {
    let mut status = Status::Reviewing;
    let mut is_trying = false;
    for word in status_str.splitn(2, ' ') {
        match word {
            "success" => status = Status::Success,
            "pending" => status = Status::Pending,
            "approved" => status = Status::Approved,
            "error" => status = Status::Error,
            "failure" => status = Status::Failure,
            "(try)" => is_trying = true,
            _ => {}
        }
    }
    (status, is_trying)
}

/// Parses the rendered priority string.
fn parse_priority(priority_str: &str) -> i32 {
    if priority_str == "rollup" {
        -1
    } else {
        priority_str.parse().unwrap_or(0)
    }
}
