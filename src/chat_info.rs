//! Read-only chat details and advisory send restrictions; Telegram is authoritative.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Info {
    pub title: String,
    pub username: Option<String>,
    pub about: String,
    pub members: Option<u32>,
    pub role: String,
    pub restrictions: Vec<Restriction>,
    pub slow_seconds: u32,
    pub next_send_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Content {
    Text,
    Photo,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Restriction {
    pub content: Option<Content>,
    pub reason: String,
    /// Zero is permanent. Server-provided restrictions may expire without an update.
    pub until: i64,
}

impl Info {
    #[must_use]
    pub fn restriction(&self, content: Content, now: i64) -> Option<String> {
        if let Some(rule) = self.restrictions.iter().find(|rule| {
            (rule.content.is_none() || rule.content == Some(content))
                && (rule.until == 0 || rule.until > now)
        }) {
            return Some(if rule.until > 0 {
                format!(
                    "{} · until {}",
                    rule.reason,
                    chrono::DateTime::from_timestamp(rule.until, 0).map_or_else(
                        || rule.until.to_string(),
                        |d| d.format("%Y-%m-%d %H:%M UTC").to_string()
                    )
                )
            } else {
                rule.reason.clone()
            });
        }
        (self.next_send_at > now).then(|| format!("Slow mode · wait {} s", self.next_send_at - now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_restrictions_expire_independently_of_slow_mode() {
        let mut info = Info {
            next_send_at: 120,
            restrictions: vec![Restriction {
                content: Some(Content::Text),
                reason: "Restricted".to_owned(),
                until: 110,
            }],
            ..Info::default()
        };
        assert!(
            info.restriction(Content::Text, 100)
                .unwrap()
                .starts_with("Restricted")
        );
        assert_eq!(
            info.restriction(Content::Photo, 100).as_deref(),
            Some("Slow mode · wait 20 s")
        );
        assert_eq!(
            info.restriction(Content::Text, 115).as_deref(),
            Some("Slow mode · wait 5 s")
        );
        assert_eq!(info.restriction(Content::Text, 120), None);
        info.restrictions[0].until = 0;
        assert_eq!(
            info.restriction(Content::Text, 130).as_deref(),
            Some("Restricted")
        );
    }
}
