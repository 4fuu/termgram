//! Telegram pin identities and ordering, independent of protocol and UI types.

use crate::model::ChatId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DialogPins {
    pub main: Vec<ChatId>,
    pub archive: Vec<ChatId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogScope {
    Main,
    Archive,
    Filter(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogAction {
    Set(bool),
    MoveUp,
    MoveDown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageAction {
    Pin { notify: bool, only_self: bool },
    Unpin,
    UnpinAll,
}

pub const PAGE_SIZE: usize = 30;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MessagePage {
    pub messages: Vec<crate::model::Message>,
    pub total: Option<usize>,
    pub before: i32,
    pub next: Option<i32>,
}

/// Move within the complete server order, including peers hidden by the UI.
pub(crate) fn move_pin<T>(order: &mut [T], position: usize, action: DialogAction) {
    let target = match action {
        DialogAction::MoveUp => position.checked_sub(1),
        DialogAction::MoveDown => position.checked_add(1).filter(|index| *index < order.len()),
        DialogAction::Set(_) => None,
    };
    if let Some(target) = target {
        order.swap(position, target);
    }
}
