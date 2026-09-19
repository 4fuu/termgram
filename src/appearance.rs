//! Account-specific color overrides, separate from discardable caches and Lua.
use crate::model::ChatId;
use anyhow::{Result, bail};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalColor {
    #[default]
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

impl TerminalColor {
    pub const ALL: [Self; 17] = [
        Self::Default,
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::Gray,
        Self::DarkGray,
        Self::LightRed,
        Self::LightGreen,
        Self::LightYellow,
        Self::LightBlue,
        Self::LightMagenta,
        Self::LightCyan,
        Self::White,
    ];
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Terminal default",
            Self::Black => "Black",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
            Self::Blue => "Blue",
            Self::Magenta => "Magenta",
            Self::Cyan => "Cyan",
            Self::Gray => "Gray",
            Self::DarkGray => "Dark gray",
            Self::LightRed => "Light red",
            Self::LightGreen => "Light green",
            Self::LightYellow => "Light yellow",
            Self::LightBlue => "Light blue",
            Self::LightMagenta => "Light magenta",
            Self::LightCyan => "Light cyan",
            Self::White => "White",
        }
    }
    #[must_use]
    pub const fn color(self) -> Color {
        match self {
            Self::Default => Color::Reset,
            Self::Black => Color::Black,
            Self::Red => Color::Red,
            Self::Green => Color::Green,
            Self::Yellow => Color::Yellow,
            Self::Blue => Color::Blue,
            Self::Magenta => Color::Magenta,
            Self::Cyan => Color::Cyan,
            Self::Gray => Color::Gray,
            Self::DarkGray => Color::DarkGray,
            Self::LightRed => Color::LightRed,
            Self::LightGreen => Color::LightGreen,
            Self::LightYellow => Color::LightYellow,
            Self::LightBlue => Color::LightBlue,
            Self::LightMagenta => Color::LightMagenta,
            Self::LightCyan => Color::LightCyan,
            Self::White => Color::White,
        }
    }
    #[must_use]
    pub fn folder_default(id: i32) -> Self {
        const COLORS: [TerminalColor; 6] = [
            TerminalColor::Cyan,
            TerminalColor::Yellow,
            TerminalColor::Magenta,
            TerminalColor::Green,
            TerminalColor::Blue,
            TerminalColor::Red,
        ];
        COLORS[usize::try_from(id.rem_euclid(6)).unwrap_or_default()]
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub chats: BTreeMap<ChatId, TerminalColor>,
    pub folders: BTreeMap<i32, TerminalColor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Chat(ChatId),
    Folder(i32),
}
impl Colors {
    #[must_use]
    pub fn get(&self, target: Target) -> Option<TerminalColor> {
        match target {
            Target::Chat(id) => self.chats.get(&id),
            Target::Folder(id) => self.folders.get(&id),
        }
        .copied()
    }
    pub fn set(&mut self, target: Target, value: Option<TerminalColor>) {
        match target {
            Target::Chat(id) => {
                if let Some(value) = value {
                    self.chats.insert(id, value);
                } else {
                    self.chats.remove(&id);
                }
            }
            Target::Folder(id) => {
                if let Some(value) = value {
                    self.folders.insert(id, value);
                } else {
                    self.folders.remove(&id);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub accounts: BTreeMap<i64, Colors>,
}
impl Preferences {
    /// # Errors
    /// Returns read or format errors without replacing the existing preferences.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        if text.len() > 1024 * 1024 {
            bail!("appearance preferences exceed 1 MiB");
        }
        Ok(serde_json::from_str(&text)?)
    }
    /// # Errors
    /// Returns write errors; uses the existing atomic settings writer.
    pub fn save(&self, path: &Path) -> Result<()> {
        crate::config::write_preferences(path, &serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_roundtrip_uses_durable_account_ids_and_keeps_bad_files() {
        let dir = std::env::temp_dir().join(format!("termgram-appearance-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("appearance.json");
        let mut preferences = Preferences::default();
        preferences
            .accounts
            .entry(101)
            .or_default()
            .set(Target::Chat(5), Some(TerminalColor::Magenta));
        preferences
            .accounts
            .entry(102)
            .or_default()
            .set(Target::Folder(5), Some(TerminalColor::Cyan));
        preferences.save(&path).unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), preferences);
        preferences
            .accounts
            .get_mut(&101)
            .unwrap()
            .set(Target::Chat(5), None);
        preferences.save(&path).unwrap();
        assert!(
            Preferences::load(&path).unwrap().accounts[&101]
                .chats
                .is_empty()
        );
        std::fs::write(&path, "invalid").unwrap();
        assert!(Preferences::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
