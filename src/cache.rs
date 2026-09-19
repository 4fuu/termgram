//! Account-local, discardable message storage. Authentication stays in the
//! Telegram session; user preferences do not belong in this database.

use std::{collections::BTreeMap, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use libsql::{Connection, Database, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    event::NetworkEvent,
    model::{Chat, ChatId, Delivery, Message},
};

const SCHEMA_VERSION: i64 = 2;
const MAX_MESSAGES: i64 = 100_000;
const MAX_CHAT_MESSAGES: i64 = 5_000;

/// SDK-independent cursor. Only complete update batches may advance it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncCursor {
    pub pts: i32,
    pub qts: i32,
    pub date: i32,
    pub seq: i32,
    pub channels: Vec<(i64, i32)>,
}

pub struct Store {
    _database: Database,
    connection: Connection,
    revision: i64,
    dialog_revision: i64,
    history_revisions: BTreeMap<(ChatId, u64), i64>,
}

impl Store {
    /// # Errors
    /// Returns filesystem, migration or SQLite errors; never discards a cache
    /// merely because it could not be read.
    pub async fn open(path: &Path) -> Result<Self> {
        prepare_file(path)?;
        let database = libsql::Builder::new_local(path).build().await?;
        let connection = database.connect()?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await?;
        let version = transaction
            .query("PRAGMA user_version", ())
            .await?
            .next()
            .await?
            .context("missing schema version")?
            .get::<i64>(0)?;
        if version > SCHEMA_VERSION {
            bail!("message cache was created by a newer Termgram version");
        }
        if version == 0 {
            transaction.execute_batch(
                "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE chats (id INTEGER PRIMARY KEY, data TEXT NOT NULL, revision INTEGER NOT NULL);
                 CREATE TABLE messages (chat_id INTEGER NOT NULL, id INTEGER NOT NULL,
                    data TEXT, timestamp INTEGER NOT NULL, revision INTEGER NOT NULL,
                    PRIMARY KEY (chat_id, id));
                 CREATE INDEX messages_recent ON messages(timestamp DESC);
                 CREATE TABLE global_deletions (id INTEGER PRIMARY KEY, revision INTEGER NOT NULL);
                 CREATE TABLE attachments (chat_id INTEGER NOT NULL, message_id INTEGER NOT NULL,
                    path TEXT NOT NULL, PRIMARY KEY(chat_id, message_id));
                 PRAGMA user_version = 1;"
            ).await?;
        }
        if version < 2 {
            transaction.execute_batch("CREATE TABLE history_pages (chat_id INTEGER NOT NULL, before_id INTEGER NOT NULL, oldest_id INTEGER NOT NULL, PRIMARY KEY(chat_id,before_id)); PRAGMA user_version=2;").await?;
        }
        transaction.commit().await?;
        let revision = metadata(&connection, "revision")
            .await?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        Ok(Self {
            _database: database,
            connection,
            revision,
            dialog_revision: revision,
            history_revisions: BTreeMap::new(),
        })
    }

    /// # Errors
    /// Returns database or decoding errors.
    pub async fn snapshot(&self) -> Result<(Option<String>, Vec<Chat>)> {
        let name = metadata(&self.connection, "user_name").await?;
        let mut rows = self.connection.query("SELECT data FROM chats", ()).await?;
        let mut chats: Vec<Chat> = Vec::new();
        while let Some(row) = rows.next().await? {
            chats.push(serde_json::from_str(&row.get::<String>(0)?)?);
        }
        chats.sort_by_key(|chat| std::cmp::Reverse(chat.last_activity));
        Ok((name, chats))
    }

    /// # Errors
    /// Returns database or decoding errors.
    pub async fn cursor(&self) -> Result<SyncCursor> {
        metadata(&self.connection, "cursor")
            .await?
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map(Option::unwrap_or_default)
            .map_err(Into::into)
    }

    /// # Errors
    /// Returns a database error.
    pub async fn account_id(&self) -> Result<Option<i64>> {
        Ok(metadata(&self.connection, "account_id")
            .await?
            .and_then(|value| value.parse().ok()))
    }

    /// Read a bounded page without any network dependency.
    /// # Errors
    /// Returns database or decoding errors.
    pub async fn history(
        &self,
        chat_id: ChatId,
        before: Option<i32>,
        limit: usize,
    ) -> Result<Vec<Message>> {
        let limit = i64::try_from(limit.min(500)).unwrap_or(500);
        let mut rows = self.connection.query(
            "SELECT data FROM messages WHERE chat_id=?1 AND id<?2 AND data IS NOT NULL ORDER BY id DESC LIMIT ?3",
            params![chat_id, before.unwrap_or(i32::MAX), limit],
        ).await?;
        let mut messages = Vec::new();
        while let Some(row) = rows.next().await? {
            messages.push(serde_json::from_str(&row.get::<String>(0)?)?);
        }
        messages.reverse();
        Ok(messages)
    }

    /// A cache page is reusable only when this boundary was fetched completely.
    /// # Errors
    /// Returns database or decoding errors.
    pub async fn older_page(
        &self,
        chat_id: ChatId,
        before_id: i32,
    ) -> Result<Option<Vec<Message>>> {
        let row = self
            .connection
            .query(
                "SELECT oldest_id FROM history_pages WHERE chat_id=?1 AND before_id=?2",
                params![chat_id, before_id],
            )
            .await?
            .next()
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let oldest_id: i32 = row.get(0)?;
        if oldest_id == 0 {
            return Ok(Some(Vec::new()));
        }
        let messages = self.history(chat_id, Some(before_id), 80).await?;
        if messages
            .first()
            .is_none_or(|message| message.id != oldest_id)
        {
            return Ok(None);
        }
        Ok(Some(messages))
    }

    /// Commit events and any covered cursor together. Replay after an interrupted
    /// commit is idempotent, and snapshot writes cannot overwrite later updates.
    /// # Errors
    /// Returns database or encoding errors; the transaction is rolled back.
    #[allow(clippy::too_many_lines)]
    pub async fn apply(&mut self, events: &[NetworkEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await?;
        for event in events {
            self.revision += 1;
            let revision = self.revision;
            match event {
                NetworkEvent::CacheAccountReset { user_id } => {
                    transaction.execute_batch("DELETE FROM chats; DELETE FROM messages; DELETE FROM metadata; DELETE FROM attachments; DELETE FROM global_deletions; DELETE FROM history_pages;").await?;
                    set_metadata(&transaction, "account_id", &user_id.to_string()).await?;
                }
                NetworkEvent::AccountIdentity { user_id } => {
                    set_metadata(&transaction, "account_id", &user_id.to_string()).await?;
                }
                NetworkEvent::Ready { user_name } => {
                    set_metadata(&transaction, "user_name", user_name).await?;
                }
                NetworkEvent::SyncCheckpoint(cursor) => {
                    set_metadata(&transaction, "cursor", &serde_json::to_string(cursor)?).await?;
                }
                NetworkEvent::DialogsLoading => self.dialog_revision = revision,
                NetworkEvent::HistoryLoading {
                    chat_id,
                    request_id,
                } => {
                    self.history_revisions
                        .insert((*chat_id, *request_id), revision);
                }
                NetworkEvent::Dialogs(chats) => {
                    for chat in chats {
                        let mut chat = chat.clone();
                        if let Some((current, updated)) = load_chat(&transaction, chat.id).await?
                            && updated > self.dialog_revision
                        {
                            chat.last_message = current.last_message;
                            chat.last_activity = current.last_activity;
                            chat.unread = current.unread;
                        }
                        write_chat(&transaction, &chat, revision).await?;
                    }
                    transaction
                        .execute(
                            "DELETE FROM chats WHERE revision<=?1",
                            [self.dialog_revision],
                        )
                        .await?;
                }
                NetworkEvent::History {
                    chat_id,
                    request_id,
                    messages,
                }
                | NetworkEvent::OlderHistory {
                    chat_id,
                    request_id,
                    messages,
                    ..
                } => {
                    let Some(started) = self.history_revisions.remove(&(*chat_id, *request_id))
                    else {
                        continue;
                    };
                    if let NetworkEvent::OlderHistory { before_id, .. } = event {
                        transaction
                            .execute(
                                "INSERT OR REPLACE INTO history_pages VALUES(?1,?2,?3)",
                                params![
                                    *chat_id,
                                    *before_id,
                                    messages.first().map_or(0, |message| message.id)
                                ],
                            )
                            .await?;
                    }
                    for message in messages {
                        write_message(&transaction, message, revision, started).await?;
                    }
                    transaction
                        .execute(
                            "DELETE FROM messages WHERE chat_id=?1 AND id NOT IN
                         (SELECT id FROM messages WHERE chat_id=?1 ORDER BY id DESC LIMIT ?2)",
                            params![*chat_id, MAX_CHAT_MESSAGES],
                        )
                        .await?;
                }
                NetworkEvent::HistoryFailed {
                    chat_id,
                    request_id,
                    ..
                }
                | NetworkEvent::OlderHistoryFailed {
                    chat_id,
                    request_id,
                    ..
                } => {
                    self.history_revisions.remove(&(*chat_id, *request_id));
                }
                NetworkEvent::NewMessage(message)
                | NetworkEvent::CacheMessage(message)
                | NetworkEvent::MessageUpdated(message)
                | NetworkEvent::MessageSent { message, .. }
                | NetworkEvent::MessageLoaded { message, .. } => {
                    write_message(&transaction, message, revision, revision).await?;
                    if let Some((mut chat, _)) = load_chat(&transaction, message.chat_id).await?
                        && chat
                            .last_activity
                            .is_none_or(|date| message.timestamp >= date)
                    {
                        chat.last_message.clone_from(&message.text);
                        chat.last_activity = Some(message.timestamp);
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }
                NetworkEvent::MessagesDeleted {
                    channel_id,
                    message_ids,
                } => {
                    for id in message_ids {
                        if let Some(chat_id) = channel_id {
                            transaction.execute(
                                "INSERT INTO messages VALUES(?1,?2,NULL,unixepoch(),?3) ON CONFLICT(chat_id,id)
                                 DO UPDATE SET data=NULL,revision=excluded.revision",
                                params![*chat_id, *id, revision],
                            ).await?;
                        } else {
                            transaction
                                .execute(
                                    "INSERT OR REPLACE INTO global_deletions VALUES(?1,?2)",
                                    params![*id, revision],
                                )
                                .await?;
                            transaction.execute("UPDATE messages SET data=NULL,revision=?2 WHERE id=?1 AND chat_id>-1000000000000", params![*id, revision]).await?;
                        }
                    }
                }
                NetworkEvent::MessagesRead { chat_id, max_id } => {
                    let mut rows = transaction.query("SELECT data FROM messages WHERE chat_id=?1 AND id<=?2 AND data IS NOT NULL", params![*chat_id, *max_id]).await?;
                    let mut messages: Vec<Message> = Vec::new();
                    while let Some(row) = rows.next().await? {
                        messages.push(serde_json::from_str(&row.get::<String>(0)?)?);
                    }
                    drop(rows);
                    for mut message in messages {
                        if message.outgoing {
                            message.delivery = Delivery::Read;
                            write_message(&transaction, &message, revision, revision).await?;
                        }
                    }
                }
                NetworkEvent::ReadMarked { chat_id } => {
                    if let Some((mut chat, _)) = load_chat(&transaction, *chat_id).await? {
                        chat.unread = 0;
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }
                NetworkEvent::AttachmentDownloaded {
                    chat_id,
                    message_id,
                    path,
                } => {
                    transaction
                        .execute(
                            "INSERT OR REPLACE INTO attachments VALUES(?1,?2,?3)",
                            params![*chat_id, *message_id, path.to_string_lossy().as_ref()],
                        )
                        .await?;
                }
                NetworkEvent::CacheInvalidated { chat_id } => {
                    self.history_revisions
                        .retain(|(id, _), _| chat_id.is_some_and(|chat_id| *id != chat_id));
                    if let Some(chat_id) = chat_id {
                        transaction
                            .execute("DELETE FROM history_pages WHERE chat_id=?1", [*chat_id])
                            .await?;
                        transaction
                            .execute("DELETE FROM messages WHERE chat_id=?1", [*chat_id])
                            .await?;
                    } else {
                        transaction.execute("DELETE FROM history_pages", ()).await?;
                        transaction.execute("DELETE FROM messages", ()).await?;
                    }
                }
                _ => {}
            }
        }
        transaction.execute("DELETE FROM messages WHERE (chat_id,id) IN (SELECT chat_id,id FROM messages ORDER BY timestamp DESC LIMIT -1 OFFSET ?1)", [MAX_MESSAGES]).await?;
        set_metadata(&transaction, "revision", &self.revision.to_string()).await?;
        transaction.commit().await?;
        Ok(())
    }
}

async fn write_message(
    connection: &Connection,
    message: &Message,
    revision: i64,
    started: i64,
) -> Result<()> {
    if message.id <= 0 {
        return Ok(());
    }
    connection.execute(
        "INSERT INTO messages(chat_id,id,data,timestamp,revision)
         SELECT ?1,?2,?3,?4,?5 WHERE NOT EXISTS
         (SELECT 1 FROM global_deletions WHERE id=?2 AND ?1>-1000000000000)
         ON CONFLICT(chat_id,id) DO UPDATE SET data=excluded.data,timestamp=excluded.timestamp,revision=excluded.revision
         WHERE messages.data IS NOT NULL AND messages.revision<=?6",
        params![message.chat_id, message.id, serde_json::to_string(message)?, message.timestamp.timestamp(), revision, started],
    ).await?;
    Ok(())
}

async fn load_chat(connection: &Connection, id: ChatId) -> Result<Option<(Chat, i64)>> {
    let row = connection
        .query("SELECT data,revision FROM chats WHERE id=?1", [id])
        .await?
        .next()
        .await?;
    row.map(|row| Ok((serde_json::from_str(&row.get::<String>(0)?)?, row.get(1)?)))
        .transpose()
}

async fn write_chat(connection: &Connection, chat: &Chat, revision: i64) -> Result<()> {
    connection
        .execute(
            "INSERT OR REPLACE INTO chats VALUES(?1,?2,?3)",
            params![chat.id, serde_json::to_string(chat)?, revision],
        )
        .await?;
    Ok(())
}

async fn metadata(connection: &Connection, key: &str) -> Result<Option<String>> {
    connection
        .query("SELECT value FROM metadata WHERE key=?1", [key])
        .await?
        .next()
        .await?
        .map(|row| row.get(0))
        .transpose()
        .map_err(Into::into)
}

async fn set_metadata(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection
        .execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![key, value],
        )
        .await?;
    Ok(())
}

fn prepare_file(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            bail!("message cache must be a regular file")
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path)?;
        }
        Err(error) => return Err(error.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatKind;
    use chrono::{TimeZone, Utc};

    fn message(id: i32, text: &str) -> Message {
        Message {
            id,
            chat_id: 42,
            sender: "Ada".to_owned(),
            reply_to: None,
            text: text.to_owned(),
            timestamp: Utc.timestamp_opt(100 + i64::from(id), 0).unwrap(),
            outgoing: false,
            delivery: Delivery::Read,
            attachment: None,
            links: Vec::new(),
            buttons: Vec::new(),
        }
    }

    #[tokio::test]
    async fn restart_keeps_live_changes_and_their_cursor_and_isolates_accounts() {
        let directory = std::env::temp_dir().join(format!("termgram-cache-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("one.sqlite3");
        let mut store = Store::open(&path).await.unwrap();
        let chat = Chat {
            id: 42,
            title: "Group".to_owned(),
            kind: ChatKind::Group,
            unread: 0,
            last_message: String::new(),
            last_activity: None,
        };
        let cursor = SyncCursor {
            pts: 12,
            date: 123,
            seq: 4,
            ..SyncCursor::default()
        };
        store
            .apply(&[
                NetworkEvent::AccountIdentity { user_id: 7 },
                NetworkEvent::Ready {
                    user_name: "Ada".to_owned(),
                },
                NetworkEvent::DialogsLoading,
                NetworkEvent::Dialogs(vec![chat]),
                NetworkEvent::HistoryLoading {
                    chat_id: 42,
                    request_id: 1,
                },
                NetworkEvent::MessageUpdated(message(1, "edited")),
                NetworkEvent::MessagesDeleted {
                    channel_id: None,
                    message_ids: vec![2],
                },
                NetworkEvent::NewMessage(message(3, "live")),
                NetworkEvent::History {
                    chat_id: 42,
                    request_id: 1,
                    messages: vec![message(1, "old"), message(2, "deleted")],
                },
                NetworkEvent::SyncCheckpoint(cursor.clone()),
            ])
            .await
            .unwrap();
        drop(store);
        let mut store = Store::open(&path).await.unwrap();
        let messages = store.history(42, None, 80).await.unwrap();
        assert_eq!(
            messages
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>(),
            vec!["edited", "live"]
        );
        assert_eq!(store.cursor().await.unwrap(), cursor);
        assert_eq!(store.account_id().await.unwrap(), Some(7));
        assert_eq!(store.snapshot().await.unwrap().0.as_deref(), Some("Ada"));
        let other = Store::open(&directory.join("two.sqlite3")).await.unwrap();
        assert!(other.history(42, None, 80).await.unwrap().is_empty());
        assert!(other.snapshot().await.unwrap().1.is_empty());
        drop(other);

        // A failed batch must roll back both its content and its checkpoint.
        store.connection.execute_batch("CREATE TRIGGER reject_message BEFORE INSERT ON messages WHEN NEW.id=9 BEGIN SELECT RAISE(ABORT,'simulated write failure'); END;").await.unwrap();
        assert!(
            store
                .apply(&[
                    NetworkEvent::SyncCheckpoint(SyncCursor {
                        pts: 99,
                        ..cursor.clone()
                    }),
                    NetworkEvent::NewMessage(message(9, "uncommitted"))
                ])
                .await
                .is_err()
        );
        assert_eq!(store.cursor().await.unwrap(), cursor);
        store
            .apply(&[NetworkEvent::CacheAccountReset { user_id: 8 }])
            .await
            .unwrap();
        assert!(store.history(42, None, 80).await.unwrap().is_empty());
        assert!(store.snapshot().await.unwrap().1.is_empty());
        assert_eq!(store.cursor().await.unwrap(), SyncCursor::default());
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn cached_paging_requires_a_complete_boundary_and_keeps_tombstones() {
        let directory =
            std::env::temp_dir().join(format!("termgram-paging-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = Store::open(&directory.join("cache.sqlite3")).await.unwrap();
        store
            .apply(&[
                NetworkEvent::NewMessage(message(3, "live")),
                NetworkEvent::MessagesDeleted {
                    channel_id: None,
                    message_ids: vec![2],
                },
            ])
            .await
            .unwrap();
        assert!(
            store.older_page(42, 4).await.unwrap().is_none(),
            "isolated cached messages are not a complete history page"
        );
        store
            .apply(&[
                NetworkEvent::HistoryLoading {
                    chat_id: 42,
                    request_id: 2,
                },
                NetworkEvent::OlderHistory {
                    chat_id: 42,
                    request_id: 2,
                    before_id: 4,
                    messages: vec![
                        message(1, "edited"),
                        message(2, "deleted"),
                        message(3, "live"),
                    ],
                },
            ])
            .await
            .unwrap();
        assert_eq!(store.older_page(42, 4).await.unwrap().unwrap().len(), 2);

        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
