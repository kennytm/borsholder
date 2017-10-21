//! Utilities for rendering the page via Tera.

use ammonia::Builder;
use chrono_humanize::HumanTime;
use chrono::{DateTime, Local, Utc};
use github::graphql::{Label, MergeableState, PullRequest, StatusContext};
use homu::{Entry, Status};
use std::collections::HashMap;
use std::fmt::Display;
use std::time::UNIX_EPOCH;
use tera::{Tera, Value};

/// Information of a pull request.
#[derive(Serialize)]
pub struct Pr {
    /// The author of the PR (GitHub username).
    author: String,
    /// When the PR was opened.
    created_at: DateTime<Utc>,
    /// The last time the PR was updated.
    updated_at: DateTime<Utc>,
    /// Whether the PR can be merged cleanly.
    mergeable: MergeableState,
    /// PR title.
    title: String,
    /// Labels applied to the PR.
    labels: Vec<Label>,
    /// When the last commit of this PR was committed.
    committed_at: DateTime<Utc>,
    /// CI status of the last commit.
    ci_status: Vec<StatusContext>,
    /// Last comment added to the PR (excluding review comments).
    last_comment: Option<Comment>,
    /// Approval status.
    status: Status,
    /// Whether the approval status applies to a "try" run.
    is_trying: bool,
    /// Assigned reviewer.
    reviewer: String,
    /// Person who approved the PR.
    approver: String,
    /// Priority. Rollups are always assigned a priority of `-1`.
    priority: i32,
}

/// Statistics about all the pull requests in the queue.
#[derive(Serialize, Default)]
pub struct PrStats {
    /// Total number of pull requests.
    count: u32,
    /// Total number of approved, mergeable PRs.
    approved: u32,
    /// Total number of approved, mergeable PRs with rollup priority.
    rollups: u32,
}

// Cannot derive default since it is not implemented for DateTime.
impl Default for Pr {
    fn default() -> Self {
        Self {
            author: String::new(),
            created_at: UNIX_EPOCH.into(),
            updated_at: UNIX_EPOCH.into(),
            mergeable: MergeableState::Unknown,
            title: String::new(),
            labels: Vec::new(),
            committed_at: UNIX_EPOCH.into(),
            ci_status: Vec::new(),
            last_comment: None,
            status: Status::Reviewing,
            is_trying: false,
            reviewer: String::new(),
            approver: String::new(),
            priority: 0,
        }
    }
}

/// Information of a PR comment.
#[derive(Serialize)]
struct Comment {
    /// Database ID, used to produce direct link to the comment.
    id: u64,
    /// Author of the comment.
    author: String,
    /// HTML body of the comment. The HTML should be already sanitized.
    body: String,
    /// When the comment was published.
    published_at: DateTime<Utc>,
}

lazy_static! {
    /// The sanitizer used to clean up a raw PR comment.
    static ref HTML_SANITIZER: Builder<'static> = {
        let mut builder = Builder::new();
        builder.tags(hashset![
            "a", "blockquote", "br", "code", "dd", "del", "details", "div", "dl", "dt", "em",
            "h1", "h2", "h3", "h4", "h5", "h6", "img", "ins", "kbd", "li", "ol", "p", "pre",
            "q", "s", "samp", "strike", "strong", "sub", "summary", "sup",
            "table", "tbody", "td", "tfoot", "th", "thead", "tr", "ul", "var",
        ]).tag_attributes(hashmap![
            "a" => hashset!["href"],
            "img" => hashset!["src"],
            "ol" => hashset!["start"],
            "th" => hashset!["align", "colspan", "rowspan"],
            "td" => hashset!["align", "colspan", "rowspan"],
        ]).allowed_classes(hashmap!["div" => hashset!["email-quoted-reply"]]);
        builder
    };
}

/// Combines information from GitHub and Homu to get a list of pull request information.
pub fn parse_prs(github_entries: Vec<PullRequest>, homu_entries: Vec<Entry>) -> HashMap<u32, Pr> {
    let mut prs = HashMap::new();

    for mut gh in github_entries {
        let commit = gh.commits.nodes.swap_remove(0).commit;
        let comment = gh.comments.nodes.into_iter().next();
        prs.insert(
            gh.number,
            Pr {
                author: gh.author.login,
                created_at: gh.created_at,
                updated_at: gh.updated_at,
                mergeable: gh.mergeable,
                title: gh.title,
                labels: gh.labels.nodes,
                committed_at: commit.committed_date,
                ci_status: commit.status.contexts,
                last_comment: comment.map(|c| {
                    Comment {
                        id: c.database_id,
                        author: c.author.login,
                        body: HTML_SANITIZER.clean(&c.body_html).to_string(),
                        published_at: c.published_at,
                    }
                }),
                ..Pr::default()
            },
        );
    }

    for h in homu_entries {
        let pr = prs.entry(h.number).or_insert_with(Pr::default);
        pr.status = h.status;
        pr.is_trying = h.is_trying;
        pr.reviewer = h.reviewer;
        pr.approver = h.approver;
        pr.priority = h.priority;
    }

    prs
}

/// Reads in an iterator of PR references, and produces statistics about them.
pub fn summarize_prs<'a, I: IntoIterator<Item = &'a Pr>>(prs: I) -> PrStats {
    let mut stats = PrStats::default();
    for pr in prs {
        stats.count += 1;
        if pr.status == Status::Approved && pr.mergeable != MergeableState::Conflicting {
            stats.approved += 1;
            if pr.priority == -1 {
                stats.rollups += 1;
            }
        }
    }
    stats
}

/// Registers some Tera filters, testers and global functions needed for rendering.
pub fn register_tera_filters(tera: &mut Tera) {
    tera.register_filter("local_datetime", |input, _| {
        let result = parse_datetime(&input)?.with_timezone(&Local).to_rfc2822();
        Ok(Value::String(result))
    });
    tera.register_filter("relative_datetime", |input, _| {
        let result = HumanTime::from(parse_datetime(&input)?).to_string();
        Ok(Value::String(result))
    });
    tera.register_filter("text_color", |input, _| {
        let bg_color = input.as_str().ok_or("expecting a string as input")?;
        let red = map_err_to_string(u16::from_str_radix(&bg_color[0..2], 16))?;
        let green = map_err_to_string(u16::from_str_radix(&bg_color[2..4], 16))?;
        let blue = map_err_to_string(u16::from_str_radix(&bg_color[4..6], 16))?;
        let luminance = red * 3 + green * 4 + blue;
        let color = if luminance >= 1020 { "#000" } else { "#fff" };
        Ok(Value::String(color.to_owned()))
    });
    tera.register_tester("starting_with", |value, mut params| {
        let prefix_value = params.swap_remove(0);
        let prefix = prefix_value.as_str().expect("prefix should be a string");
        if let Some(s) = value.as_ref().and_then(Value::as_str) {
            Ok(s.starts_with(prefix))
        } else {
            Ok(false)
        }
    });
}

/// Parses a Tera value into a `DateTime`.
fn parse_datetime(input: &Value) -> ::tera::Result<DateTime<Utc>> {
    let datetime = input
        .as_str()
        .ok_or("expecting a string as input")?
        .parse::<DateTime<Utc>>();
    map_err_to_string(datetime)
}

/// If the result is an error, converts it to a string so that it can be recognized by Tera.
fn map_err_to_string<T, E: Display>(a: Result<T, E>) -> ::tera::Result<T> {
    Ok(a.map_err(|e| e.to_string())?)
}
