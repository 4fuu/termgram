//! Merge a cloud snapshot with edits and tombstones received during its RPC.
use super::{NetworkEvent, Result, Store, params};

impl Store {
    pub(crate) async fn reconcile_cloud_search(&self, event: &mut NetworkEvent) -> Result<()> {
        let (request_id, chat_id, messages) = match event {
            NetworkEvent::CloudSearchResults {
                request_id,
                chat_id,
                page,
            } => (*request_id, *chat_id, &mut page.messages),
            NetworkEvent::CloudSearchContext {
                request_id,
                chat_id,
                messages,
                ..
            } => (*request_id, *chat_id, messages),
            _ => return Ok(()),
        };
        let Some((_, _, started)) = self
            .search_revision
            .filter(|(id, chat, _)| *id == request_id && *chat == chat_id)
        else {
            *event = NetworkEvent::SearchFailed {
                request_id,
                error: "History changed during search; run the search again".to_owned(),
            };
            return Ok(());
        };
        let mut merged = Vec::with_capacity(messages.len());
        for message in messages.drain(..) {
            if message.chat_id != chat_id || message.id <= 0 {
                continue;
            }
            if self
                .connection
                .query(
                    "SELECT 1 FROM global_deletions WHERE id=?1 AND ?2>-1000000000000",
                    params![message.id, chat_id],
                )
                .await?
                .next()
                .await?
                .is_some()
            {
                continue;
            }
            let row = self
                .connection
                .query(
                    "SELECT data,revision FROM messages WHERE chat_id=?1 AND id=?2",
                    params![chat_id, message.id],
                )
                .await?
                .next()
                .await?;
            if let Some(row) = row {
                let Some(data) = row.get::<Option<String>>(0)? else {
                    continue;
                };
                if row.get::<i64>(1)? > started {
                    merged.push(serde_json::from_str(&data)?);
                    continue;
                }
            }
            merged.push(message);
        }
        *messages = merged;
        Ok(())
    }
}
