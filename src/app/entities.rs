use super::AppState;
use crate::{entities::Kind, model::Message};
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Default)]
pub(super) struct Visibility {
    revision: u64,
    spoilers: bool,
    quotes: bool,
}

fn revision(message: &Message) -> u64 {
    let mut hash = DefaultHasher::new();
    message.text.hash(&mut hash);
    message.entities.hash(&mut hash);
    hash.finish()
}

impl AppState {
    #[must_use]
    pub fn spoilers_revealed(&self, message: &Message) -> bool {
        self.text_visibility
            .get(&(message.chat_id, message.id))
            .is_some_and(|state| state.spoilers && state.revision == revision(message))
    }

    #[must_use]
    pub fn quotes_expanded(&self, message: &Message) -> bool {
        self.text_visibility
            .get(&(message.chat_id, message.id))
            .is_some_and(|state| state.quotes && state.revision == revision(message))
    }

    pub(super) fn toggle_text_details(&mut self, spoiler: bool) {
        let Some(message) = self.inspected_message() else {
            return;
        };
        let available = if spoiler {
            message.has_spoilers()
        } else {
            message.entities.iter().any(|entity| {
                matches!(entity.kind, Kind::Quote { collapsed: true })
                    && entity.valid_for(&message.text)
            })
        };
        if !available {
            self.status_message = Some(
                if spoiler {
                    "Selected message has no spoiler"
                } else {
                    "Selected message has no expandable quote"
                }
                .to_owned(),
            );
            return;
        }
        let key = (message.chat_id, message.id);
        let revision = revision(message);
        if self.text_visibility.len() >= 256 && !self.text_visibility.contains_key(&key) {
            self.text_visibility.pop_first();
        }
        let state = self.text_visibility.entry(key).or_default();
        if state.revision != revision {
            *state = Visibility {
                revision,
                ..Visibility::default()
            };
        }
        if spoiler {
            state.spoilers = !state.spoilers;
        } else {
            state.quotes = !state.quotes;
        }
        self.status_message = None;
    }
}
