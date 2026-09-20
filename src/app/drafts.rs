use super::{App, ChatId};
use crate::drafts::{Draft, Key, Snapshot, Stored};

impl App {
    pub(super) fn draft_key(&self, chat: ChatId) -> Key {
        Key {
            account: self.account_user_id.unwrap_or(0),
            chat,
            topic: 0,
        }
    }

    pub(super) fn draft_data(&self, chat: ChatId) -> Option<&Draft> {
        self.drafts.get(&self.draft_key(chat))
    }

    pub(super) fn draft_data_mut(&mut self, chat: ChatId) -> &mut Draft {
        let key = self.draft_key(chat);
        self.draft_at_mut(key)
    }

    pub(super) fn draft_at_mut(&mut self, key: Key) -> &mut Draft {
        self.drafts_dirty = true;
        if key.account > 0 {
            self.draft_modified_accounts.insert(key.account);
        }
        self.drafts.entry(key).or_default()
    }

    pub(super) fn restore_drafts(&mut self, account: i64, drafts: Vec<Stored>) {
        // A second login/bootstrap can race with a newer, unflushed edit in
        // this process. Already-loaded accounts retain their in-memory state.
        if !self.draft_accounts.insert(account) {
            return;
        }
        for draft in drafts {
            self.drafts
                .entry(Key {
                    account,
                    chat: draft.chat,
                    topic: draft.topic,
                })
                .or_insert_with(|| draft.draft());
        }
    }

    /// Coalesced, account-scoped persistence input. Empty loaded accounts are
    /// included so clearing the final draft is durable; unknown ones are absent.
    pub fn take_draft_snapshot(&mut self) -> Option<Snapshot> {
        if !std::mem::take(&mut self.drafts_dirty) {
            return None;
        }
        let mut snapshot: Snapshot = self
            .draft_accounts
            .intersection(&self.draft_modified_accounts)
            .map(|&account| (account, Vec::new()))
            .collect();
        for (key, draft) in &self.drafts {
            if !draft.is_empty()
                && let Some(drafts) = snapshot.get_mut(&key.account)
            {
                drafts.push(draft.stored(key.chat, key.topic));
            }
        }
        Some(snapshot)
    }
}
