use super::{App, Focus, KeyAction, Mode, TelegramCommand};
use crate::appearance::{Preferences, Target, TerminalColor};
use anyhow::Result;

#[derive(Clone)]
pub struct Picker {
    pub target: Target,
    pub label: String,
    pub selection: usize,
    previous_mode: Mode,
}

impl App {
    /// # Errors
    /// Leaves existing managed preferences untouched when they cannot be read.
    pub fn load_appearance(&mut self) -> Result<()> {
        if let Some(path) = &self.settings_path {
            self.appearance = Preferences::load(&path.with_file_name("appearance.json"))?;
        }
        Ok(())
    }
    #[must_use]
    pub fn color(&self, target: Target) -> ratatui::style::Color {
        self.account_user_id
            .and_then(|account| self.appearance.accounts.get(&account))
            .and_then(|colors| colors.get(target))
            .or_else(|| self.keymap.colors.get(target))
            .unwrap_or_else(|| match target {
                Target::Chat(_) => TerminalColor::Default,
                Target::Folder(id) => TerminalColor::folder_default(id),
            })
            .color()
    }
    pub(super) fn begin_color_picker(&mut self, folder: bool) -> Vec<TelegramCommand> {
        let target = if folder {
            self.folders
                .iter()
                .find(|folder| folder.id == self.folder_id)
                .map(|folder| {
                    (
                        Target::Folder(folder.id),
                        format!("Folder: {}", folder.title),
                    )
                })
        } else {
            (if self.focus == Focus::Chats {
                self.selected_chat_entry()
            } else {
                self.active_chat()
            })
            .map(|chat| (Target::Chat(chat.id), format!("Chat: {}", chat.title)))
        };
        if let Some((target, label)) = target {
            self.begin_color_picker_for(target, label);
        }
        Vec::new()
    }

    pub(super) fn begin_color_picker_for(&mut self, target: Target, label: String) {
        let selected = self
            .account_user_id
            .and_then(|account| self.appearance.accounts.get(&account))
            .and_then(|colors| colors.get(target));
        let selection = selected
            .and_then(|color| {
                TerminalColor::ALL
                    .iter()
                    .position(|candidate| *candidate == color)
            })
            .map_or(0, |index| index + 1);
        self.status_message = None;
        self.color_picker = Some(Picker {
            target,
            label,
            selection,
            previous_mode: self.mode,
        });
        self.mode = Mode::Colors;
    }
    pub(super) fn handle_colors(&mut self, action: KeyAction) -> Vec<TelegramCommand> {
        if action == KeyAction::Escape {
            self.mode = self
                .color_picker
                .as_ref()
                .map_or(Mode::Navigate, |picker| picker.previous_mode);
            self.color_picker = None;
            return Vec::new();
        }
        let Some(picker) = &mut self.color_picker else {
            return Vec::new();
        };
        match action {
            KeyAction::Up => picker.selection = picker.selection.saturating_sub(1),
            KeyAction::Down => {
                picker.selection = picker
                    .selection
                    .saturating_add(1)
                    .min(TerminalColor::ALL.len());
            }
            KeyAction::Enter => {
                let target = picker.target;
                let previous_mode = picker.previous_mode;
                let value = picker
                    .selection
                    .checked_sub(1)
                    .map(|index| TerminalColor::ALL[index]);
                match self.save_color(target, value) {
                    Ok(()) => {
                        self.color_picker = None;
                        self.mode = previous_mode;
                        self.status_message = None;
                    }
                    Err(error) => {
                        self.status_message = Some(format!("Color was not saved: {error:#}"));
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }
    fn save_color(&mut self, target: Target, value: Option<TerminalColor>) -> Result<()> {
        let account = self
            .account_user_id
            .ok_or_else(|| anyhow::anyhow!("account identity is not available yet"))?;
        let path = self
            .settings_path
            .as_ref()
            .map(|path| path.with_file_name("appearance.json"));
        let mut preferences = path.as_ref().map_or_else(
            || Ok(self.appearance.clone()),
            |path| Preferences::load(path),
        )?;
        preferences
            .accounts
            .entry(account)
            .or_default()
            .set(target, value);
        if let Some(path) = path {
            preferences.save(&path)?;
        }
        self.appearance = preferences;
        Ok(())
    }
}
