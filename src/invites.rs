//! Invite links are capabilities. Keep hashes out of logs and persisted UI state.
use crate::model::Chat;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preview {
    pub title: String,
    pub about: String,
    pub participants: Option<u32>,
    pub request_needed: bool,
    pub warning: Option<String>,
    pub blocked: Option<String>,
    pub joined: Option<Chat>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Joined(Chat),
    Requested,
    Verification,
}

/// Recognize official web and tg invite forms using the shared URL parser.
#[must_use]
pub fn hash(value: &str) -> Option<String> {
    let value = value.trim();
    let normalized = if value.starts_with("t.me/") || value.starts_with("telegram.me/") {
        format!("https://{value}")
    } else {
        value.to_owned()
    };
    let url = url::Url::parse(&normalized).ok()?;
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return None;
    }
    let hash = if url.scheme() == "tg" && url.host_str() == Some("join") {
        let values = url
            .query_pairs()
            .filter(|(key, _)| key == "invite")
            .collect::<Vec<_>>();
        if values.len() != 1 {
            return None;
        }
        values[0].1.to_string()
    } else if matches!(url.scheme(), "http" | "https")
        && matches!(
            url.host_str(),
            Some("t.me" | "www.t.me" | "telegram.me" | "www.telegram.me")
        )
    {
        url.path()
            .strip_prefix("/+")
            .or_else(|| url.path().strip_prefix("/joinchat/"))?
            .to_owned()
    } else {
        return None;
    };
    (!hash.is_empty()
        && hash.len() <= 256
        && hash
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(hash)
}

#[cfg(test)]
mod tests {
    #[test]
    fn invite_links_accept_official_forms_and_reject_ambiguous_targets() {
        for value in [
            "https://t.me/+Ab_c-09",
            "t.me/joinchat/Ab_c-09",
            "tg://join?invite=Ab_c-09",
        ] {
            assert_eq!(super::hash(value).as_deref(), Some("Ab_c-09"));
        }
        for value in [
            "https://t.me.evil/+Ab",
            "https://t.me@evil/+Ab",
            "https://t.me/+Ab/more",
            "tg://join?invite=a&invite=b",
            "https://t.me/+",
            "https://t.me/joinchat-extra/Ab",
            "https://t.me/%2bAb",
        ] {
            assert_eq!(super::hash(value), None, "{value}");
        }
    }
}
