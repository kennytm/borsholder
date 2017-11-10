//! Home queue web scraper.

use errors::{Result, ResultExt};
use kuchiki::parse_html;
use kuchiki::traits::TendrilSink;
use markup5ever::ExpandedName;
use reqwest::{Client, Url};

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
    /// Assigned reviewer.
    pub reviewer: String,
    /// Person who approved the PR.
    pub approver: String,
    /// Priority. Rollups are always assigned a priority of `-1`.
    pub priority: i32,
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
pub fn query(client: &Client, url: &Url) -> Result<Vec<Entry>> {
    info!("Preparing to send Homu request");

    let mut resp = client.get(url.clone()).send()?.error_for_status()?;
    let doc = parse_html().from_utf8().read_from(&mut resp)?;

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
            bail!("Homu queue structure probably changed. Aborting.");
        }

        let number = tds[2].parse().chain_err(|| "invalid PR number")?;
        let (status, is_trying) = parse_status(&tds[3]);
        let priority = parse_priority(&tds[9]);
        let approver = tds.swap_remove(8);
        let reviewer = tds.swap_remove(7);
        let title = tds.swap_remove(5);

        res.push(Entry {
            number,
            title,
            status,
            is_trying,
            reviewer,
            approver,
            priority,
        });
    }

    info!("Obtained {} PRs from Homu", res.len());

    Ok(res)
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
