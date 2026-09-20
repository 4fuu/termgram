//! Notification preferences shared by commands, Telegram and status rendering.
pub mod delivery;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mute {
    Off,
    Hour,
    EightHours,
    TwoDays,
    Forever,
}

impl Mute {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "1h" => Some(Self::Hour),
            "8h" => Some(Self::EightHours),
            "2d" => Some(Self::TwoDays),
            "" | "forever" => Some(Self::Forever),
            _ => None,
        }
    }

    pub(crate) fn until(self, now: i64) -> i32 {
        let seconds = match self {
            Self::Off => return 0,
            Self::Forever => return i32::MAX,
            Self::Hour => 3600,
            Self::EightHours => 8 * 3600,
            Self::TwoDays => 2 * 24 * 3600,
        };
        i32::try_from(now.saturating_add(seconds)).unwrap_or(i32::MAX)
    }
}

#[must_use]
pub fn mute_label(until: i64, now: i64) -> String {
    if until <= now {
        "Notifications on".to_owned()
    } else if until >= i64::from(i32::MAX) {
        "Muted forever".to_owned()
    } else {
        chrono::DateTime::from_timestamp(until, 0).map_or_else(
            || "Muted".to_owned(),
            |date| {
                format!(
                    "Muted until {}",
                    date.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                )
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Auto,
    Native,
    Osc9,
    Bell,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum When {
    #[default]
    Unseen,
    Unfocused,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub enabled: bool,
    pub backend: Backend,
    pub previews: bool,
    pub sound: bool,
    pub when: When,
    pub group_delay_ms: u64,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: Backend::Auto,
            previews: true,
            sound: true,
            when: When::Unseen,
            group_delay_ms: 800,
        }
    }
}

impl Configuration {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (100..=5000).contains(&self.group_delay_ms),
            "notifications.group_delay_ms must be 100–5000"
        );
        Ok(())
    }
}

/// Transient delivery metadata. Cached history never produces notification intents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub sender: Option<crate::model::ChatId>,
    pub silent: bool,
    pub protected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SettingsKey {
    pub chat: crate::model::ChatId,
    pub sender: Option<crate::model::ChatId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Preferences {
    pub mute_until: i64,
    pub previews: bool,
    pub sound: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolved {
    pub chat: Preferences,
    pub sender_mute_until: Option<i64>,
}

pub struct Alert {
    pub account: i64,
    pub chat: crate::model::ChatId,
    pub ids: Vec<i32>,
    pub title: String,
    pub body: String,
    pub sound: bool,
    pub valid: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub expires: std::time::Instant,
}
