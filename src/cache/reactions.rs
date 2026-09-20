use crate::{
    model::Message,
    reactions::{Summary, Update},
};
use libsql::{Connection, params};

pub(super) type Journal = super::journal::Journal<Update>;

pub(super) fn reconcile(journal: &Journal, message: &mut Message, started: i64) {
    for update in journal
        .since(started)
        .filter(|update| (update.chat, update.message) == (message.chat_id, message.id))
    {
        message
            .reactions
            .get_or_insert_with(Summary::default)
            .apply(&update.summary);
    }
    if journal.stale(started) {
        message.reactions.get_or_insert_with(Summary::default).stale = true;
    }
}

pub(super) fn reconcile_summary(
    journal: &Journal,
    chat: i64,
    id: i32,
    summary: &mut Summary,
    started: i64,
) {
    for update in journal
        .since(started)
        .filter(|update| (update.chat, update.message) == (chat, id))
    {
        summary.apply(&update.summary);
    }
    summary.stale |= journal.stale(started);
}

pub(super) async fn merge_cached(
    connection: &Connection,
    message: &mut Message,
) -> anyhow::Result<()> {
    if let Some(summary) = &mut message.reactions {
        merge_summary(connection, message.chat_id, message.id, summary).await?;
    }
    Ok(())
}

pub(super) async fn merge_summary(
    connection: &Connection,
    chat: i64,
    id: i32,
    fresh: &mut Summary,
) -> anyhow::Result<()> {
    if fresh.choices_known {
        return Ok(());
    }
    let row = connection
        .query(
            "SELECT data FROM messages WHERE chat_id=?1 AND id=?2 AND data IS NOT NULL",
            params![chat, id],
        )
        .await?
        .next()
        .await?;
    if let Some(row) = row {
        let cached: Message = serde_json::from_str(&row.get::<String>(0)?)?;
        if let Some(mut summary) = cached.reactions {
            summary.apply(fresh);
            *fresh = summary;
        }
    }
    Ok(())
}

pub(super) async fn apply(
    connection: &Connection,
    update: &Update,
    revision: i64,
) -> anyhow::Result<()> {
    let row = connection
        .query(
            "SELECT data FROM messages WHERE chat_id=?1 AND id=?2 AND data IS NOT NULL",
            params![update.chat, update.message],
        )
        .await?
        .next()
        .await?;
    if let Some(row) = row {
        let mut message: Message = serde_json::from_str(&row.get::<String>(0)?)?;
        message
            .reactions
            .get_or_insert_with(Summary::default)
            .apply(&update.summary);
        connection.execute("UPDATE messages SET data=?1,revision=?2 WHERE chat_id=?3 AND id=?4 AND data IS NOT NULL", params![serde_json::to_string(&message)?, revision, update.chat, update.message]).await?;
    }
    Ok(())
}
