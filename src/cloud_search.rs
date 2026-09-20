//! Cloud search arguments are separate from the verbatim local regex syntax.
use anyhow::{Result, ensure};
use chrono::NaiveDate;
use clap::{Parser, ValueEnum};

use crate::model::{ChatId, Message};

pub const PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Sender {
    Me,
    Username(String),
    Id(ChatId),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum Media {
    #[default]
    All,
    Photo,
    Video,
    File,
    Music,
    Voice,
    Round,
    Gif,
    Link,
    Poll,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Filters {
    pub sender: Option<Sender>,
    pub after: Option<NaiveDate>,
    pub before: Option<NaiveDate>,
    pub media: Media,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub id: u64,
    pub chat_id: ChatId,
    pub query: String,
    pub filters: Filters,
    pub before_id: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub messages: Vec<Message>,
    pub next: Option<i32>,
    pub total: usize,
    /// Resolve a username once, then retain this identity while paging.
    pub sender: Option<ChatId>,
}

#[derive(Parser)]
#[command(
    name = ":search --cloud",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Arguments {
    #[arg(long, value_parser = parse_sender, value_name = "me|@username|ID", allow_negative_numbers = true)]
    from: Option<Sender>,
    #[arg(long, value_parser = parse_date, value_name = "YYYY-MM-DD")]
    after: Option<NaiveDate>,
    #[arg(long, value_parser = parse_date, value_name = "YYYY-MM-DD")]
    before: Option<NaiveDate>,
    #[arg(long, value_enum, default_value = "all")]
    media: Media,
    #[arg(trailing_var_arg = true)]
    text: Vec<String>,
}

/// Parse only the arguments after `--cloud`; never interpret a local regex.
/// # Errors
/// Returns invalid quoting, option, sender or date errors for the command UI.
pub fn parse(value: &str) -> Result<(String, Filters)> {
    #[cfg(unix)]
    let words = yazi_shared::shell::unix::split(value, false)?.0;
    #[cfg(windows)]
    let words = yazi_shared::shell::windows::split(&format!("search {value}"))?
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>();
    let args = Arguments::try_parse_from(std::iter::once("search".to_owned()).chain(words))?;
    ensure!(
        !matches!((args.after, args.before), (Some(after), Some(before)) if after >= before),
        "--after must be earlier than --before (UTC)"
    );
    let query = args.text.join(" ");
    ensure!(query.len() <= 4096, "Search text is longer than 4096 bytes");
    Ok((
        query,
        Filters {
            sender: args.from,
            after: args.after,
            before: args.before,
            media: args.media,
        },
    ))
}

fn parse_sender(value: &str) -> Result<Sender, String> {
    if value == "me" {
        Ok(Sender::Me)
    } else if let Some(name) = value.strip_prefix('@').filter(|name| {
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }) {
        Ok(Sender::Username(name.to_owned()))
    } else if let Ok(id) = value.parse::<ChatId>()
        && id != 0
    {
        Ok(Sender::Id(id))
    } else {
        Err("Use me, @username, or a known Telegram peer ID".to_owned())
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "Use a date in YYYY-MM-DD format".to_owned())?;
    let timestamp = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp();
    if date.format("%Y-%m-%d").to_string() != value
        || !(1..=i64::from(i32::MAX)).contains(&timestamp)
    {
        return Err("Date must be YYYY-MM-DD between 1970-01-02 and 2038-01-19".to_owned());
    }
    Ok(date)
}

impl Filters {
    #[must_use]
    pub fn label(&self) -> String {
        let mut labels = Vec::new();
        if let Some(sender) = &self.sender {
            labels.push(match sender {
                Sender::Me => "from me".to_owned(),
                Sender::Username(name) => format!("from @{name}"),
                Sender::Id(id) => format!("from {id}"),
            });
        }
        if let Some(date) = self.after {
            labels.push(format!("after {date} UTC"));
        }
        if let Some(date) = self.before {
            labels.push(format!("before {date} UTC"));
        }
        if self.media != Media::All
            && let Some(value) = self.media.to_possible_value()
        {
            labels.push(value.get_name().to_owned());
        }
        labels.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_arguments_preserve_quoted_text_and_validate_filters() {
        let (text, filters) = parse("--from @ada --after 2026-09-01 --before 2026-09-20 --media file \"release notes\" 中文").unwrap();
        assert_eq!(text, "release notes 中文");
        assert_eq!(filters.sender, Some(Sender::Username("ada".to_owned())));
        assert_eq!(filters.media, Media::File);
        assert!(filters.label().contains("after 2026-09-01 UTC"));
        assert_eq!(parse("-- --from literal").unwrap().0, "--from literal");
        assert_eq!(
            parse("--from -1001234567890").unwrap().1.sender,
            Some(Sender::Id(-1_001_234_567_890))
        );
        assert_eq!(parse("word --media photo").unwrap().0, "word --media photo");
        assert!(parse("--from me --media poll").unwrap().0.is_empty());
        for bad in [
            "--from nobody",
            "--from 0",
            "--media invalid",
            "--after 2026-02-30",
            "--after 2026-9-01",
            "--after 2038-01-20",
            "--before 1970-01-01",
            "--after 2026-09-20 --before 2026-09-01",
            "--unknown",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
