//! Small revision journal for poll patches arriving during history RPCs.
//! Durable messages and the covered sync cursor still commit in one transaction.
use crate::{
    model::Message,
    polls::{Poll, ResultsPatch, Update},
};
use libsql::{Connection, params};
pub(super) type Journal = super::journal::Journal<Update>;

impl Journal {
    pub fn reconcile(&self, poll: &mut Poll, started: i64) {
        for update in self.since(started) {
            poll.apply(update);
        }
        // Dropped patches require a fresh lookup before choices can be trusted.
        if self.stale(started) {
            poll.stale = true;
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
        assert_eq!(journal.since(0).count(), 0);
    }
}
