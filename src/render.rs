use ammonia::Builder;
use chrono_humanize::HumanTime;
use chrono::{DateTime, Local, Utc};
use github::graphql::{Label, MergeableState, PullRequest, StatusContext};
use homu::{Entry, Status};
use std::collections::HashMap;
use std::fmt::Display;
use std::time::UNIX_EPOCH;
use tera::{Tera, Value};

#[derive(Serialize)]
pub struct Pr {
    author: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    mergeable: MergeableState,
    title: String,
    labels: Vec<Label>,
    committed_at: DateTime<Utc>,
    ci_status: Vec<StatusContext>,
    last_comment: Option<Comment>,
    status: Status,
    is_trying: bool,
    reviewer: String,
    approver: String,
    priority: i32,
}

#[derive(Serialize, Default)]
pub struct PrStats {
    count: u32,
    approved: u32,
    rollups: u32,
}

// Cannot derive default since it is not implemented for DateTime.
impl Default for Pr {
    fn default() -> Pr {
        Pr {
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

#[derive(Serialize)]
struct Comment {
    id: u64,
    author: String,
    body: String,
    published_at: DateTime<Utc>,
}

lazy_static! {
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
        let prefix = prefix_value.as_str().unwrap();
        if let Some(s) = value.as_ref().and_then(Value::as_str) {
            Ok(s.starts_with(prefix))
        } else {
            Ok(false)
        }
    });
}

fn parse_datetime(input: &Value) -> ::tera::Result<DateTime<Utc>> {
    let datetime = input
        .as_str()
        .ok_or("expecting a string as input")?
        .parse::<DateTime<Utc>>();
    map_err_to_string(datetime)
}

fn map_err_to_string<T, E: Display>(a: Result<T, E>) -> ::tera::Result<T> {
    Ok(a.map_err(|e| e.to_string())?)
}
