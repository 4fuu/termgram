//! A reviewed, single-message native forward. Retries retain the same random ID.
use crate::model::{Chat, Message};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    pub message: Message,
    pub destination: Chat,
    pub revision: [u8; 32],
    pub random_id: i64,
}
