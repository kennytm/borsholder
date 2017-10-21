use errors::{Result, ResultExt};
use kuchiki::parse_html;
use kuchiki::traits::TendrilSink;
use markup5ever::ExpandedName;
use reqwest::{Client, Url};

pub struct Entry {
    pub number: u32,
    pub status: Status,
    pub is_trying: bool,
    pub reviewer: String,
    pub approver: String,
    pub priority: i32,
}

#[derive(Serialize, PartialEq, Eq)]
pub enum Status {
    Success,
    Pending,
    Approved,
    Reviewing,
    Error,
    Failure,
}

impl Default for Status {
    fn default() -> Self {
        Status::Reviewing
    }
}

pub fn query(client: &Client, url: &Url) -> Result<Vec<Entry>> {
    let mut resp = client.get(url.clone()).send()?.error_for_status()?;
    let doc = parse_html().from_utf8().read_from(&mut resp)?;

    let mut res = Vec::new();
    for tr in doc.select("#queue > tbody > tr").unwrap() {
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

        res.push(Entry {
            number,
            status,
            is_trying,
            reviewer,
            approver,
            priority,
        });
    }

    Ok(res)
}

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

fn parse_priority(priority_str: &str) -> i32 {
    if priority_str == "rollup" {
        -1
    } else {
        priority_str.parse().unwrap_or(0)
    }
}
