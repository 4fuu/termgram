//! Notification preferences shared by commands, Telegram and status rendering.

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
