//! Sidebar dimensions and semantic colors. Terminal themes own the palette.
use crate::appearance::TerminalColor;
use serde::Deserialize;

pub const MIN_SPLIT_WIDTH: u16 = 80;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub width: u16,
    pub time_color: TerminalColor,
    pub unread_color: TerminalColor,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            width: 30,
            time_color: TerminalColor::Cyan,
            unread_color: TerminalColor::Yellow,
        }
    }
}

impl Configuration {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (24..=60).contains(&self.width),
            "sidebar width must be between 24 and 60 terminal columns"
        );
        Ok(())
    }
    #[must_use]
    pub fn width_for(&self, terminal_width: u16) -> u16 {
        self.width.min(terminal_width.saturating_sub(48))
    }
}
