//! Utilities for rendering the page via Tera.

use chrono::{DateTime, Local, Utc};
use github::graphql::{Label, MergeableState, PullRequest, StatusContext};
use homu::{Entry, Status};
use reqwest::Url;
use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;
use std::time::UNIX_EPOCH;
use tera::{self, Tera, Value};

/// Information of a pull request.
#[derive(Serialize)]
pub struct Pr {
    /// The author of the PR (GitHub username).
    author: String,
    /// When the PR was opened.
    created_at: DateTime<Utc>,
    /// Last update time of the PR.
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
    /// Recent actions performed on the PR.
    timeline: Vec<Value>,
    /// Approval status.
    status: Status,
    /// Whether the approval status applies to a "try" run.
    is_trying: bool,
    /// Priority. Rollups are always assigned a priority of `-1`.
    priority: i32,
    /// PR approver name.
    approver: String,
    /// Number of additions to the PR.
    additions: u32,
    /// Number of deletions to the PR.
    deletions: u32,
    /// Base branch name of the PR.
    base_ref_name: String,
    /// Branch name of the PR in the author's repository.
    head_ref_name: String,
    /// PR body text.
    body: String,
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
            timeline: Vec::new(),
            status: Status::Reviewing,
            is_trying: false,
            priority: 0,
            approver: String::new(),
            additions: 0,
            deletions: 0,
            base_ref_name: String::new(),
            head_ref_name: String::new(),
            body: String::new(),
        }
    }
}

/// Combines information from GitHub and Homu to get a list of pull request information.
pub fn parse_prs(github_entries: Vec<PullRequest>, homu_entries: Vec<Entry>) -> HashMap<u32, Pr> {
    let mut prs = HashMap::new();

    for mut gh in github_entries {
        let commit = gh.commits.nodes.swap_remove(0).commit;
        prs.insert(
            gh.number,
            Pr {
                author: gh.author.login,
                created_at: gh.created_at,
                updated_at: gh.updated_at,
                mergeable: gh.mergeable,
                title: gh.title,
                labels: gh.labels.nodes,
                ci_status: commit.status.map_or_else(Vec::new, |s| s.contexts),
                additions: gh.additions,
                deletions: gh.deletions,
                base_ref_name: gh.base_ref_name,
                head_ref_name: gh.head_ref_name,
                body: gh.body,
                ..Pr::default()
            },
        );
    }

    for h in homu_entries {
        let title = h.title;
        let pr = prs.entry(h.number).or_insert_with(move || Pr {
            title,
            ..Pr::default()
        });
        pr.status = h.status;
        pr.is_trying = h.is_trying;
        pr.priority = h.priority;
        pr.approver = h.approver;
    }

    prs
}

/// Reads in an iterator of PR references, and produces statistics about them.
pub fn summarize_prs<'b, I: IntoIterator<Item = &'b Pr>>(prs: I) -> PrStats {
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
        tera.register_filter("debug", |input, _| {
            Ok(Value::String(format!("{:#?}", input)))
        });
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
    tera.register_tester("starting_with", |value, mut params| {
        let prefix_value = params.swap_remove(0);
        let prefix = prefix_value.as_str().expect("prefix should be a string");
        if let Some(s) = value.as_ref().and_then(Value::as_str) {
            Ok(s.starts_with(prefix))
        } else {
            Ok(false)
        }
    });
    tera.register_filter("escape_rollup_instruction", |input, _| {
        Ok(Value::String(
            try_get_value!("escape_rollup_instruction", "value", String, input)
                .replace("&#x27;", "'&quot;'&quot;'"),
        ))
    });
    tera.register_function("sqrt", Box::new(|mut params| {
        let input = params.remove("input").and_then(|v| v.as_f64()).expect("number");
        Ok(Value::from(input.sqrt()))
    }));
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
    /// The Tera error kind.
    kind: tera::ErrorKind,
}

impl From<tera::Error> for TeraFailure {
    #[cfg_attr(feature = "cargo-clippy", allow(use_debug))]
    fn from(e: tera::Error) -> Self {
        warn!("captured tera error: {:#?}", e);
        Self { kind: e.0 }
    }
}
