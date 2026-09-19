//! Native Telegram dialog filters, independent of SDK and terminal types.

use crate::model::{Chat, ChatId, ChatKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ChatMembership {
    pub contact: bool,
    pub bot: bool,
    pub archived: bool,
    pub unread_mark: bool,
    pub mentions: bool,
    pub mute_until: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Folder {
    pub id: i32,
    pub title: String,
    pub pinned: Vec<ChatId>,
    pub include: Vec<ChatId>,
    pub exclude: Vec<ChatId>,
    pub contacts: bool,
    pub non_contacts: bool,
    pub bots: bool,
    pub groups: bool,
    pub broadcasts: bool,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
}

impl Folder {
    #[must_use]
    pub fn all() -> Self {
        Self {
            title: "All chats".to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn contains(&self, chat: &Chat, now: i64) -> bool {
        if self.id == 0 {
            return true;
        }
        if self.exclude.contains(&chat.id) {
            return false;
        }
        if self.include.contains(&chat.id) || self.pinned.contains(&chat.id) {
            return true;
        }
        let membership = &chat.membership;
        let kind = match chat.kind {
            ChatKind::Direct if membership.bot => self.bots,
            ChatKind::Direct if membership.contact => self.contacts,
            ChatKind::Direct => self.non_contacts,
            ChatKind::Group => self.groups,
            ChatKind::Channel => self.broadcasts,
        };
        kind && (!self.exclude_archived || !membership.archived)
            && (!self.exclude_read
                || chat.unread > 0
                || membership.mentions
                || membership.unread_mark)
            && (!self.exclude_muted
                || membership.mute_until <= now
                || (membership.mentions && !membership.archived))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_membership_wins_and_muted_mentions_follow_desktop_rules() {
        let mut chat = Chat {
            id: 1,
            title: String::new(),
            kind: ChatKind::Group,
            unread: 0,
            last_message: String::new(),
            last_activity: None,
            membership: ChatMembership {
                mute_until: 500,
                ..ChatMembership::default()
            },
        };
        let mut folder = Folder {
            id: 2,
            groups: true,
            exclude_muted: true,
            exclude_read: true,
            ..Folder::default()
        };
        assert!(!folder.contains(&chat, 100));
        chat.membership.mentions = true;
        assert!(folder.contains(&chat, 100));
        chat.membership.archived = true;
        assert!(!folder.contains(&chat, 100));
        folder.include.push(1);
        assert!(folder.contains(&chat, 100));
        folder.exclude.push(1);
        assert!(!folder.contains(&chat, 100));
        folder.exclude.clear();
        folder.include.clear();
        folder.exclude_read = false;
        assert!(folder.contains(&chat, 600));
        folder.exclude_archived = true;
        assert!(!folder.contains(&chat, 600));
        chat.kind = ChatKind::Direct;
        chat.membership.bot = true;
        chat.membership.contact = true;
        folder.contacts = true;
        folder.exclude_archived = false;
        assert!(!folder.contains(&chat, 600));
        folder.bots = true;
        assert!(folder.contains(&chat, 600));
    }
}
