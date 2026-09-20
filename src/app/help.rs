//! Search the effective keymap and command registry without a second input path.
use super::{App, KeyAction, Mode, TelegramCommand};
use crate::{actions::Action, input::TextInput};
use regex::{Regex, RegexBuilder};

#[derive(Clone, Default)]
pub struct State {
    pub query: TextInput,
    pub editing: bool,
    pub scroll: usize,
    pub matcher: Option<Regex>,
}

impl State {
    pub fn edited(&mut self) {
        self.scroll = 0;
        self.matcher = (!self.query.is_empty()).then(|| {
            RegexBuilder::new(&regex::escape(self.query.value()))
                .case_insensitive(true)
                .build()
                .expect("bounded literal help query")
        });
    }
}

impl App {
    pub(super) fn open_help(&mut self) {
        self.mode_before_help = self.mode;
        self.help = State::default();
        self.mode = Mode::Help;
    }

    pub(super) fn help_binding(
        &mut self,
        action: &Action,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        if self.mode != Mode::Help {
            return None;
        }
        match action {
            Action::Filter | Action::Search => self.help.editing = true,
            Action::Up | Action::PageUp => {
                let step = count.saturating_mul(if *action == Action::PageUp { 10 } else { 1 });
                self.help.scroll = self.help.scroll.saturating_sub(step);
            }
            Action::Down | Action::PageDown => {
                let step = count.saturating_mul(if *action == Action::PageDown { 10 } else { 1 });
                self.help.scroll = self.help.scroll.saturating_add(step);
            }
            Action::Home if !self.help.editing => self.help.scroll = 0,
            Action::End if !self.help.editing => self.help.scroll = usize::MAX,
            _ => return None,
        }
        Some(Vec::new())
    }

    pub(super) fn handle_help(&mut self, action: KeyAction) -> Vec<TelegramCommand> {
        match action {
            KeyAction::Escape => {
                if self.help.editing || !self.help.query.is_empty() {
                    self.help = State::default();
                } else {
                    self.mode = self.mode_before_help;
                }
            }
            KeyAction::Enter => self.help.editing = false,
            KeyAction::Redraw => self.force_redraw = true,
            KeyAction::Character(character) if self.help.editing => {
                self.help.query.insert(character);
            }
            KeyAction::Backspace if self.help.editing => {
                self.help.query.backspace();
            }
            KeyAction::Delete if self.help.editing => {
                self.help.query.delete();
            }
            KeyAction::Left if self.help.editing => {
                self.help.query.move_left();
            }
            KeyAction::Right if self.help.editing => {
                self.help.query.move_right();
            }
            KeyAction::Home if self.help.editing => self.help.query.move_home(),
            KeyAction::End if self.help.editing => self.help.query.move_end(),
            KeyAction::Clear if self.help.editing => self.help.query.clear(),
            KeyAction::DeleteWord if self.help.editing => {
                self.help.query.delete_word_before();
            }
            _ => return Vec::new(),
        }
        if self.help.editing
            && !matches!(
                action,
                KeyAction::Left
                    | KeyAction::Right
                    | KeyAction::Home
                    | KeyAction::End
                    | KeyAction::Redraw
            )
        {
            self.help.edited();
        }
        Vec::new()
    }

    #[must_use]
    pub fn help_lines(&self) -> Vec<String> {
        let mut lines = vec!["Effective shortcuts (including Lua overrides)".to_owned()];
        lines.extend(self.keymap.help());
        lines.extend([
            String::new(),
            "Commands · type : to browse and complete".to_owned(),
        ]);
        lines.extend(
            crate::commands::COMMANDS
                .iter()
                .map(|spec| format!(":{} — {}", spec.usage(), spec.description())),
        );
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn query_is_literal_case_insensitive_and_bounded() {
        let mut state = State::default();
        state.query.set_value("[C-F]");
        state.edited();
        let matcher = state.matcher.as_ref().unwrap();
        assert!(matcher.is_match("help [c-f] filter"));
        assert!(!matcher.is_match("C"));
        state.query.set_value("é".repeat(16_384));
        state.edited();
        assert!(state.matcher.is_some());
    }
}
