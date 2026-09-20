//! Small revision journal for poll patches arriving during history RPCs.
//! Durable messages and the covered sync cursor still commit in one transaction.
use crate::{
    model::Message,
    polls::{Poll, ResultsPatch, Update},
};
use libsql::{Connection, params};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Journal {
    updates: VecDeque<(i64, Update)>,
    overflow: i64,
}

impl Journal {
    pub fn record(&mut self, revision: i64, update: Update) {
        self.updates.push_back((revision, update));
        if self.updates.len() > 512 {
            self.overflow = self.updates.pop_front().expect("over limit").0;
        }
    }

    pub fn reconcile(&self, poll: &mut Poll, started: i64) {
        for (_, update) in self
            .updates
            .iter()
            .filter(|(revision, _)| *revision > started)
        {
            poll.apply(update);
        }
        // A long-stalled RPC must not advertise a possibly older choice after
        // the bounded journal overflows. A fresh lookup clears this marker.
        if started < self.overflow {
            poll.stale = true;
        }
    }

    pub fn prune(&mut self, oldest: i64) {
        self.updates.retain(|(revision, _)| *revision > oldest);
        if oldest >= self.overflow {
            self.overflow = 0;
        }
    }
}

pub(super) fn snapshot(poll: &Poll) -> Update {
    Update {
        id: poll.definition.id,
        stale: poll.stale,
        definition: Some(poll.definition.clone()),
        results: ResultsPatch {
            min: !poll.results.choice_known,
            counts: (!poll.results.counts.is_empty()).then(|| poll.results.counts.clone()),
            total: poll.results.total,
            solution: poll.results.solution.clone(),
        },
    }
}

/// Minimal snapshots omit the current user's choices. Preserve those known
/// fields from any cached copy of this globally identified poll before replaying
/// updates that arrived after the RPC began.
pub(super) async fn merge_cached(connection: &Connection, poll: &mut Poll) -> anyhow::Result<()> {
    let row = connection.query("SELECT data FROM messages WHERE json_extract(data,'$.poll.definition.id')=?1 ORDER BY revision DESC LIMIT 1", [poll.definition.id]).await?.next().await?;
    if let Some(row) = row {
        let message: Message = serde_json::from_str(&row.get::<String>(0)?)?;
        if let Some(mut previous) = message.poll {
            previous.apply(&snapshot(poll));
            *poll = previous;
        }
    }
    Ok(())
}

pub(super) async fn apply(
    connection: &Connection,
    update: &Update,
    revision: i64,
) -> anyhow::Result<()> {
    let mut rows = connection
        .query(
            "SELECT data FROM messages WHERE json_extract(data,'$.poll.definition.id')=?1",
            [update.id],
        )
        .await?;
    let mut messages = Vec::new();
    while let Some(row) = rows.next().await? {
        let mut message: Message = serde_json::from_str(&row.get::<String>(0)?)?;
        if let Some(poll) = &mut message.poll {
            poll.apply(update);
        }
        messages.push(message);
    }
    drop(rows);
    for message in messages {
        connection.execute("UPDATE messages SET data=?1, revision=?2 WHERE chat_id=?3 AND id=?4 AND data IS NOT NULL", params![serde_json::to_string(&message)?, revision, message.chat_id, message.id]).await?;
        if let Some((mut chat, _)) = super::load_chat(connection, message.chat_id).await?
            && chat.last_message_id == Some(message.id)
        {
            chat.last_message = message.preview_text();
            super::write_chat(connection, &chat, revision).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_journal_overflow_requires_a_fresh_snapshot() {
        let mut journal = Journal::default();
        let mut poll = crate::polls::example();
        for revision in 1..=514 {
            poll.results.total = Some(u32::try_from(revision).unwrap());
            journal.record(revision, snapshot(&poll));
        }
        let mut late = crate::polls::example();
        journal.reconcile(&mut late, 1);
        assert!(late.stale);
        assert_eq!(late.results.total, Some(514));
        assert!(late.read_only_reason(0).is_some());
        let mut fresh = crate::polls::example();
        journal.reconcile(&mut fresh, 513);
        assert!(!fresh.stale);
        assert_eq!(fresh.results.total, Some(514));
        journal.prune(514);
        assert!(journal.updates.is_empty());
    }
}
