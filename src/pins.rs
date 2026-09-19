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
