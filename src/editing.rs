//! Edit drafts retain their server revision separately from ordinary composition.
use crate::{input::TextInput, model::sanitize_terminal_text};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Source {
    pub message_id: i32,
    pub text: String,
    pub revision: [u8; 32],
    pub caption: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Draft {
    pub source: Source,
    pub input: TextInput,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Stored {
    pub source: Source,
    pub text: String,
    pub cursor: usize,
}

impl Draft {
    #[must_use]
    pub fn stored(&self) -> Stored {
        Stored {
            source: self.source.clone(),
            text: self.input.value().to_owned(),
            cursor: self.input.cursor(),
        }
    }
}

impl Stored {
    #[must_use]
    pub fn draft(&self) -> Draft {
        let mut source = self.source.clone();
        source.text = sanitize_terminal_text(&source.text);
        Draft {
            source,
            input: TextInput::with_cursor(sanitize_terminal_text(&self.text), self.cursor),
        }
    }
}
