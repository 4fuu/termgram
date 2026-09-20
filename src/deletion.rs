//! Explicit Telegram deletion scope, reviewed against a fresh server snapshot.
use crate::model::Message;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    OnlyMe,
    Everyone,
}

impl Scope {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OnlyMe => "Delete only for me",
            Self::Everyone => "Delete for everyone",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    pub message: Message,
    pub revision: [u8; 32],
    pub scopes: Vec<Scope>,
}
