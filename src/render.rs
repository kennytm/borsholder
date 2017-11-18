//! Utilities for rendering the page via Tera.

use ammonia::Builder;
use chrono::{DateTime, Local, Utc};
use github::graphql::{Label, MergeableState, PullRequest, StatusContext};
use homu::{Entry, Status};
use reqwest::Url;
use std::collections::HashMap;
use std::fmt::Display;
use std::time::UNIX_EPOCH;
use std::str::FromStr;
use tera::{self, Tera, Value};

/// Information of a pull request.
#[derive(Serialize)]
pub struct Pr<'a> {
    /// The author of the PR (GitHub username).
    author: &'a str,
    /// When the PR was opened.
    created_at: DateTime<Utc>,
    /// Whether the PR can be merged cleanly.
    mergeable: MergeableState,
    /// PR title.
    title: &'a str,
    /// Labels applied to the PR.
    labels: &'a [Label],
    /// When the last commit of this PR was committed.
    committed_at: DateTime<Utc>,
    /// CI status of the last commit.
    ci_status: &'a [StatusContext],
    /// Recent actions performed on the PR.
    timeline: &'a [Value],
    /// Approval status.
    status: Status,
    /// Whether the approval status applies to a "try" run.
    is_trying: bool,
    /// Assigned reviewer.
    reviewer: &'a str,
    /// Person who approved the PR.
    approver: &'a str,
    /// Priority. Rollups are always assigned a priority of `-1`.
    priority: i32,
    /// Number of additions to the PR.
    additions: u32,
    /// Number of deletions to the PR.
    deletions: u32,
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
impl<'a> Default for Pr<'a> {
    fn default() -> Self {
        Self {
            author: "",
            created_at: UNIX_EPOCH.into(),
            mergeable: MergeableState::Unknown,
            title: "",
            labels: &[],
            committed_at: UNIX_EPOCH.into(),
            ci_status: &[],
            timeline: &[],
            status: Status::Reviewing,
            is_trying: false,
            reviewer: "",
            approver: "",
            priority: 0,
            additions: 0,
            deletions: 0,
        }
    }
}

lazy_static! {
    /// The sanitizer used to clean up a raw PR comment.
    static ref HTML_SANITIZER: Builder<'static> = {
        let mut builder = Builder::new();
        builder.tags(hashset![
            "a", "blockquote", "br", "code", "dd", "del", "details", "div", "dl", "dt", "em",
            "h1", "h2", "h3", "h4", "h5", "h6", "hr", "img", "ins", "kbd", "li", "ol", "p", "pre",
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
pub fn parse_prs<'a>(
    github_entries: &'a [PullRequest],
    homu_entries: &'a [Entry],
) -> HashMap<u32, Pr<'a>> {
    let mut prs = HashMap::new();

    for gh in github_entries {
        let commit = &gh.commits.nodes[0].commit;
        prs.insert(
            gh.number,
            Pr {
                author: &gh.author.login,
                created_at: gh.created_at,
                mergeable: gh.mergeable,
                title: &gh.title,
                labels: &gh.labels.nodes,
                committed_at: commit.committed_date,
                ci_status: commit.status.as_ref().map_or(&[], |s| &s.contexts),
                timeline: &gh.timeline.nodes,
                additions: gh.additions,
                deletions: gh.deletions,
                ..Pr::default()
            },
        );
    }

    for h in homu_entries {
        let pr = prs.entry(h.number).or_insert_with(|| {
            Pr {
                title: &h.title,
                ..Pr::default()
            }
        });
        pr.status = h.status;
        pr.is_trying = h.is_trying;
        pr.reviewer = &h.reviewer;
        pr.approver = &h.approver;
        pr.priority = h.priority;
    }

    prs
}

/// Reads in an iterator of PR references, and produces statistics about them.
pub fn summarize_prs<'b, 'a: 'b, I: IntoIterator<Item = &'b Pr<'a>>>(prs: I) -> PrStats {
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
    if cfg!(debug_assertions) {
        #[cfg_attr(feature = "cargo-clippy", allow(use_debug))]
        tera.register_filter(
            "debug",
            |input, _| Ok(Value::String(format!("{:#?}", input))),
        );
    }
    tera.register_filter("local_datetime", |input, _| {
        let result = parse::<DateTime<Utc>>("local_datetime", &input)?
            .with_timezone(&Local)
            .to_rfc2822();
        Ok(Value::String(result))
    });
    tera.register_filter("text_color", |input, _| {
        let bg_color = try_get_value!("text_color", "value", String, input);
        let red = map_err_to_string(u16::from_str_radix(&bg_color[0..2], 16))?;
        let green = map_err_to_string(u16::from_str_radix(&bg_color[2..4], 16))?;
        let blue = map_err_to_string(u16::from_str_radix(&bg_color[4..6], 16))?;
        let luminance = red * 3 + green * 4 + blue;
        let color = if luminance >= 1020 { "#000" } else { "#fff" };
        Ok(Value::String(color.to_owned()))
    });
    tera.register_filter("url_last_path_component", |input, _| {
        let last_component = parse::<Url>("url_last_path_component", &input)?
            .path_segments()
            .ok_or("URL has no path")?
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or("")
            .to_owned();
        Ok(Value::String(last_component))
    });
    tera.register_filter("sanitize", |input, _| {
        let unclean = try_get_value!("sanitize", "value", String, input);
        Ok(Value::String(HTML_SANITIZER.clean(&unclean).to_string()))
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

/// Parses a Tera value into a value.
fn parse<T: FromStr>(filter: &str, input: &Value) -> ::tera::Result<T>
where
    T::Err: Display,
{
    let value = try_get_value!(filter, "value", String, input).parse();
    map_err_to_string(value)
}

/// If the result is an error, converts it to a string so that it can be recognized by Tera.
fn map_err_to_string<T, E: Display>(a: Result<T, E>) -> ::tera::Result<T> {
    Ok(a.map_err(|e| e.to_string())?)
}

/// Wraps a `tera::Error` to use the `Fail` trait.
#[derive(Debug, Fail)]
#[fail(display = "{}", kind)]
pub struct TeraFailure {
    kind: tera::ErrorKind,
}

impl From<tera::Error> for TeraFailure {
    fn from(e: tera::Error) -> Self {
        TeraFailure { kind: e.0 }
    }
}
