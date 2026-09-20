//! Incoming read boundaries are monotonic. Counts from a slower dialog snapshot
//! must not erase arrivals beyond the snapshot's top message.
use crate::model::Chat;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub max_id: i32,
    pub unread: u32,
    pub top_message: i32,
}

pub fn inbox_update(chat: &mut Chat, max_id: i32, unread: u32) {
    if max_id < chat.read_inbox_max_id.unwrap_or(0) {
        return;
    }
    chat.read_inbox_max_id = Some(max_id.max(0));
    chat.unread = unread.max(u32::from(chat.membership.unread_mark));
}

pub fn acknowledge(chat: &mut Chat, max_id: i32, snapshot: Option<&Snapshot>) {
    let previous = chat.read_inbox_max_id.unwrap_or(0);
    chat.read_inbox_max_id = Some(previous.max(max_id));
    if let Some(snapshot) = snapshot
        && snapshot.max_id >= previous.max(max_id)
        && chat
            .last_message_id
            .is_none_or(|id| id <= snapshot.top_message)
    {
        inbox_update(chat, snapshot.max_id, snapshot.unread);
    } else if chat.last_message_id.is_some_and(|id| id <= max_id) {
        chat.unread = u32::from(chat.membership.unread_mark);
    }
}
