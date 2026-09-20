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

const SCHEMA_VERSION: i64 = 5;
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
    pin_revisions: BTreeMap<(ChatId, u64), i64>,
    downloads: BTreeMap<(ChatId, i32), (u64, Option<i64>)>,
    owner: std::sync::Arc<std::fs::File>,
}

impl Store {
    /// # Errors
    /// Returns filesystem, migration or SQLite errors; never discards a cache
    /// merely because it could not be read.
    pub async fn open(path: &Path) -> Result<Self> {
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push("-lock");
        crate::config::prepare_private_file(Path::new(&lock_path))?;
        let owner = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(Path::new(&lock_path))?;
        owner
            .try_lock()
            .context("this account cache is already open in another Termgram process")?;
        crate::config::prepare_private_file(path)?;
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
        if version < 3 {
            transaction.execute_batch("CREATE INDEX messages_search ON messages(timestamp DESC,chat_id DESC,id DESC); PRAGMA user_version=3;").await?;
        }
        if version < 4 {
            transaction.execute_batch("ALTER TABLE attachments ADD COLUMN source_id INTEGER; DELETE FROM attachments; PRAGMA user_version=4;").await?;
        }
        if version < 5 {
            transaction.execute_batch("ALTER TABLE chats ADD COLUMN last_message_id INTEGER; CREATE INDEX chats_last_message ON chats(last_message_id); PRAGMA user_version=5;").await?;
        }
        transaction.commit().await?;
        let revision = metadata(&connection, "revision")
            .await?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        Ok(Self {
            _database: database,
            owner: std::sync::Arc::new(owner),
            connection,
            revision,
            dialog_revision: revision,
            history_revisions: BTreeMap::new(),
            pin_revisions: BTreeMap::new(),
            downloads: BTreeMap::new(),
        })
    }

    pub(crate) fn owner(&self) -> std::sync::Arc<std::fs::File> {
        self.owner.clone()
    }

    /// Exact cached reply targets, including deletion tombstones.
    /// # Errors
    /// Returns database or decoding errors.
    pub async fn reply_previews(
        &self,
        chat_id: ChatId,
        ids: &[i32],
    ) -> Result<(Vec<Message>, Vec<i32>)> {
        anyhow::ensure!(ids.len() <= 32, "reply preview batch is too large");
        let mut rows = self.connection.query(
            "SELECT wanted.value, messages.data, messages.id IS NOT NULL OR
              (?1>-1000000000000 AND EXISTS(SELECT 1 FROM global_deletions WHERE id=wanted.value))
             FROM json_each(?2) AS wanted LEFT JOIN messages ON messages.chat_id=?1 AND messages.id=wanted.value",
            params![chat_id, serde_json::to_string(ids)?],
        ).await?;
        let mut messages = Vec::new();
        let mut unavailable = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(data) = row.get::<Option<String>>(1)? {
                messages.push(serde_json::from_str(&data)?);
            } else if row.get::<i64>(2)? != 0 {
                unavailable.push(row.get(0)?);
            }
        }
        Ok((messages, unavailable))
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
    pub async fn folders(&self) -> Result<Vec<crate::folders::Folder>> {
        Ok(metadata(&self.connection, "folders")
            .await?
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_else(|| vec![crate::folders::Folder::all()]))
    }

    /// # Errors
    /// Returns database or decoding errors.
    pub async fn dialog_pins(&self) -> Result<crate::pins::DialogPins> {
        Ok(metadata(&self.connection, "dialog_pins")
            .await?
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default())
    }

    /// # Errors
    /// Returns database or message decoding errors.
    pub async fn pinned_messages(
        &self,
        chat_id: ChatId,
        before: i32,
    ) -> Result<crate::pins::MessagePage> {
        let mut rows = self
            .connection
            .query(
                "SELECT data FROM messages WHERE chat_id=?1 AND data IS NOT NULL
             AND json_extract(data,'$.pinned')=1 AND (?2=0 OR id<?2) ORDER BY id DESC LIMIT ?3",
                params![chat_id, before, i64::try_from(crate::pins::PAGE_SIZE)?],
            )
            .await?;
        let mut messages: Vec<Message> = Vec::new();
        while let Some(row) = rows.next().await? {
            messages.push(serde_json::from_str(&row.get::<String>(0)?)?);
        }
        let next = messages
            .last()
            .filter(|_| messages.len() == crate::pins::PAGE_SIZE)
            .map(|message| message.id);
        Ok(crate::pins::MessagePage {
            messages,
            before,
            next,
            total: None,
        })
    }

    /// # Errors
    /// Returns database errors. Missing files are treated as cache misses.
    pub async fn attachment(
        &self,
        chat_id: ChatId,
        message_id: i32,
        media_id: Option<i64>,
    ) -> Result<Option<std::path::PathBuf>> {
        let row = self.connection.query("SELECT path FROM attachments WHERE chat_id=?1 AND message_id=?2 AND source_id IS ?3", params![chat_id, message_id, media_id]).await?.next().await?;
        let path = row
            .map(|row| row.get::<String>(0).map(std::path::PathBuf::from))
            .transpose()?;
        Ok(path.filter(|path| path.is_file()))
    }

    /// Cached neighbors for a search hit, without triggering network reads.
    /// # Errors
    /// Returns an error when the hit was deleted or evicted since the search.
    pub async fn context(&self, chat_id: ChatId, message_id: i32) -> Result<Vec<Message>> {
        let mut messages = self
            .history(chat_id, Some(message_id.saturating_add(1)), 40)
            .await?;
        if !messages.iter().any(|message| message.id == message_id) {
            bail!("Search result is no longer cached; run the search again");
        }
        let mut rows = self.connection.query("SELECT data FROM messages WHERE chat_id=?1 AND id>?2 AND data IS NOT NULL ORDER BY id LIMIT 40", params![chat_id, message_id]).await?;
        while let Some(row) = rows.next().await? {
            messages.push(serde_json::from_str(&row.get::<String>(0)?)?);
        }
        Ok(messages)
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

    /// A provisional ascending page; the server still verifies its boundary.
    /// # Errors
    /// Returns database or decoding errors.
    pub async fn history_after(
        &self,
        chat_id: ChatId,
        after: i32,
        limit: usize,
    ) -> Result<Vec<Message>> {
        let mut rows = self.connection.query(
            "SELECT data FROM messages WHERE chat_id=?1 AND id>?2 AND data IS NOT NULL ORDER BY id LIMIT ?3",
            params![chat_id, after, i64::try_from(limit.min(500)).unwrap_or(500)],
        ).await?;
        let mut messages = Vec::new();
        while let Some(row) = rows.next().await? {
            messages.push(serde_json::from_str(&row.get::<String>(0)?)?);
        }
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
                NetworkEvent::ReplyPreviews {
                    chat_id,
                    request_id,
                    messages,
                    complete: true,
                    ..
                } => {
                    let Some(started) = self.history_revisions.remove(&(*chat_id, *request_id))
                    else {
                        continue;
                    };
                    for message in messages {
                        write_message(&transaction, message, revision, started).await?;
                    }
                    prune_chat_messages(&transaction, *chat_id).await?;
                }
                NetworkEvent::PinnedMessagesLoading {
                    chat_id,
                    request_id,
                } => {
                    self.pin_revisions.insert((*chat_id, *request_id), revision);
                }
                NetworkEvent::PinnedMessages {
                    chat_id,
                    request_id,
                    page,
                } if page.total.is_some() => {
                    let Some(started) = self.pin_revisions.remove(&(*chat_id, *request_id)) else {
                        continue;
                    };
                    for message in &page.messages {
                        write_message(&transaction, message, revision, started).await?;
                    }
                    transaction.execute(
                        "UPDATE messages SET data=json_set(data,'$.pinned',json('false')), revision=?1
                         WHERE chat_id=?2 AND data IS NOT NULL AND json_extract(data,'$.pinned')=1
                         AND revision<=?3 AND (?4=0 OR id<?4) AND id>=?5",
                        params![revision, *chat_id, started, page.before, page.next.unwrap_or(0)],
                    ).await?;
                    prune_chat_messages(&transaction, *chat_id).await?;
                }
                NetworkEvent::PinnedMessagesFailed {
                    chat_id,
                    request_id,
                    ..
                } => {
                    self.pin_revisions.remove(&(*chat_id, *request_id));
                }
                NetworkEvent::MessagePinsChanged {
                    chat_id,
                    message_ids,
                    pinned,
                } => {
                    for id in message_ids {
                        transaction.execute(
                            "UPDATE messages SET data=json_set(data,'$.pinned',json(?1)), revision=?2
                             WHERE chat_id=?3 AND id=?4 AND data IS NOT NULL",
                            params![if *pinned { "true" } else { "false" }, revision, *chat_id, *id],
                        ).await?;
                    }
                }
                NetworkEvent::MessagePinsCleared { chat_id } => {
                    transaction.execute("UPDATE messages SET data=json_set(data,'$.pinned',json('false')), revision=?1 WHERE chat_id=?2 AND data IS NOT NULL", params![revision, *chat_id]).await?;
                }
                NetworkEvent::PinnedContext {
                    chat_id,
                    request_id,
                    messages,
                    ..
                } => {
                    let Some(started) = self.pin_revisions.remove(&(*chat_id, *request_id)) else {
                        continue;
                    };
                    for message in messages {
                        write_message(&transaction, message, revision, started).await?;
                    }
                    prune_chat_messages(&transaction, *chat_id).await?;
                }
                NetworkEvent::DialogPins(pins) => {
                    set_metadata(&transaction, "dialog_pins", &serde_json::to_string(pins)?)
                        .await?;
                }
                NetworkEvent::ArchiveChanged { chat_id, archived } => {
                    if let Some((mut chat, _)) = load_chat(&transaction, *chat_id).await? {
                        chat.membership.archived = *archived;
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }
                NetworkEvent::CacheAccountReset { user_id } => {
                    self.downloads.clear();
                    transaction.execute_batch("DELETE FROM chats; DELETE FROM messages; DELETE FROM metadata; DELETE FROM attachments; DELETE FROM global_deletions; DELETE FROM history_pages;").await?;
                    set_metadata(&transaction, "account_id", &user_id.to_string()).await?;
                }
                NetworkEvent::Folders(folders) => {
                    set_metadata(&transaction, "folders", &serde_json::to_string(folders)?).await?;
                }
                NetworkEvent::UnreadChanged {
                    chat_id,
                    max_id,
                    unread,
                } => {
                    if let Some((mut chat, _)) = load_chat(&transaction, *chat_id).await? {
                        crate::read_state::inbox_update(&mut chat, *max_id, *unread);
                        write_chat(&transaction, &chat, revision).await?;
                    }
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
                }
                | NetworkEvent::ReplyPreviewsLoading {
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
                            chat.last_message_id = current.last_message_id;
                            chat.unread = current.unread;
                            chat.membership.unread_mark = current.membership.unread_mark;
                            chat.read_inbox_max_id =
                                chat.read_inbox_max_id.max(current.read_inbox_max_id);
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
                    prune_chat_messages(&transaction, *chat_id).await?;
                }
                NetworkEvent::HistoryFailed {
                    chat_id,
                    request_id,
                    ..
                }
                | NetworkEvent::ReplyPreviewsFailed {
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
                    if matches!(event, NetworkEvent::MessageUpdated(_)) {
                        let media_id = message
                            .attachment
                            .as_ref()
                            .and_then(|attachment| attachment.source_id);
                        if media_id.is_none()
                            || self
                                .downloads
                                .get(&(message.chat_id, message.id))
                                .is_some_and(|(_, expected)| *expected != media_id)
                        {
                            self.downloads.remove(&(message.chat_id, message.id));
                        }
                        if media_id.is_none() {
                            transaction
                                .execute(
                                    "DELETE FROM attachments WHERE chat_id=?1 AND message_id=?2",
                                    params![message.chat_id, message.id],
                                )
                                .await?;
                        }
                    }
                    let existed = transaction
                        .query(
                            "SELECT 1 FROM messages WHERE chat_id=?1 AND id=?2",
                            params![message.chat_id, message.id],
                        )
                        .await?
                        .next()
                        .await?
                        .is_some();
                    write_message(&transaction, message, revision, revision).await?;
                    if let Some((mut chat, _)) = load_chat(&transaction, message.chat_id).await? {
                        let latest = chat.last_message_id.map_or_else(
                            || {
                                chat.last_activity
                                    .is_none_or(|date| message.timestamp >= date)
                            },
                            |id| message.id >= id,
                        );
                        if matches!(event, NetworkEvent::NewMessage(_))
                            && !message.outgoing
                            && !existed
                            && latest
                            && chat.read_inbox_max_id.is_none_or(|seen| message.id > seen)
                        {
                            chat.unread = chat.unread.saturating_add(1);
                        }
                        if latest {
                            chat.last_message.clone_from(&message.text);
                            chat.last_message_id = Some(message.id);
                            chat.last_activity = Some(message.timestamp);
                        }
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }

                NetworkEvent::MessagesDeleted {
                    channel_id,
                    message_ids,
                } => {
                    for message_id in message_ids {
                        let mut rows = transaction.query("SELECT data FROM chats WHERE last_message_id=?1 AND ((?2 IS NULL AND id>-1000000000000) OR id=?2)", params![*message_id, *channel_id]).await?;
                        let mut affected = Vec::new();
                        while let Some(row) = rows.next().await? {
                            let mut chat: Chat = serde_json::from_str(&row.get::<String>(0)?)?;
                            chat.last_message.clear();
                            chat.last_message_id = None;
                            affected.push(chat);
                        }
                        drop(rows);
                        for chat in affected {
                            write_chat(&transaction, &chat, revision).await?;
                        }
                    }
                    self.downloads.retain(|(chat_id, id), _| {
                        !(message_ids.contains(id)
                            && channel_id.map_or(*chat_id > -1_000_000_000_000, |channel| {
                                channel == *chat_id
                            }))
                    });
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
                NetworkEvent::ChatUnreadChanged { chat_id, unread } => {
                    if let Some((mut chat, _)) = load_chat(&transaction, *chat_id).await? {
                        crate::read_state::unread_mark(&mut chat, *unread);
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }
                NetworkEvent::ChatUnreadFinished {
                    chat_id,
                    unread,
                    snapshot,
                    error: None,
                    ..
                } => {
                    if let Some((mut chat, _)) = load_chat(&transaction, *chat_id).await? {
                        crate::read_state::unread_mark(&mut chat, *unread);
                        if let Some(snapshot) = snapshot {
                            crate::read_state::acknowledge(
                                &mut chat,
                                snapshot.max_id,
                                Some(snapshot),
                            );
                        }
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }
                NetworkEvent::ReadMarked {
                    chat_id,
                    max_id,
                    snapshot,
                } => {
                    if let Some((mut chat, _)) = load_chat(&transaction, *chat_id).await? {
                        crate::read_state::acknowledge(&mut chat, *max_id, snapshot.as_ref());
                        write_chat(&transaction, &chat, revision).await?;
                    }
                }
                NetworkEvent::AttachmentDownloadStarted {
                    chat_id,
                    message_id,
                    request_id,
                    media_id,
                } => {
                    self.downloads
                        .insert((*chat_id, *message_id), (*request_id, *media_id));
                }
                NetworkEvent::AttachmentDownloadFailed {
                    chat_id,
                    message_id,
                    request_id,
                    ..
                } => {
                    if self
                        .downloads
                        .get(&(*chat_id, *message_id))
                        .is_some_and(|(id, _)| *id == *request_id)
                    {
                        self.downloads.remove(&(*chat_id, *message_id));
                    }
                }
                NetworkEvent::AttachmentDownloaded {
                    request_id,
                    chat_id,
                    message_id,
                    path,
                } => {
                    let Some((id, media_id)) =
                        self.downloads.get(&(*chat_id, *message_id)).copied()
                    else {
                        continue;
                    };
                    if id != *request_id {
                        continue;
                    }
                    self.downloads.remove(&(*chat_id, *message_id));
                    transaction
                        .execute(
                            "INSERT OR REPLACE INTO attachments VALUES(?1,?2,?3,?4)",
                            params![
                                *chat_id,
                                *message_id,
                                path.to_string_lossy().as_ref(),
                                media_id
                            ],
                        )
                        .await?;
                }
                NetworkEvent::CacheInvalidated { chat_id } => {
                    self.downloads
                        .retain(|(id, _), _| chat_id.is_some_and(|chat_id| *id != chat_id));
                    self.history_revisions
                        .retain(|(id, _), _| chat_id.is_some_and(|chat_id| *id != chat_id));
                    self.pin_revisions
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
        transaction.execute_batch("DELETE FROM attachments WHERE NOT EXISTS(SELECT 1 FROM messages WHERE messages.chat_id=attachments.chat_id AND messages.id=attachments.message_id AND messages.data IS NOT NULL);
            DELETE FROM history_pages WHERE oldest_id>0 AND NOT EXISTS(SELECT 1 FROM messages WHERE messages.chat_id=history_pages.chat_id AND messages.id=history_pages.oldest_id);").await?;
        transaction.execute("DELETE FROM history_pages WHERE rowid IN (SELECT rowid FROM history_pages ORDER BY rowid DESC LIMIT -1 OFFSET ?1)", [MAX_MESSAGES]).await?;
        let oldest_in_flight = self
            .history_revisions
            .values()
            .chain(self.pin_revisions.values())
            .copied()
            .min()
            .unwrap_or(self.revision);
        transaction.execute("DELETE FROM global_deletions WHERE revision<?1 AND id IN (SELECT id FROM global_deletions ORDER BY revision DESC LIMIT -1 OFFSET ?2)", params![oldest_in_flight, MAX_MESSAGES]).await?;
        set_metadata(&transaction, "revision", &self.revision.to_string()).await?;
        transaction.commit().await?;
        Ok(())
    }
}

async fn prune_chat_messages(connection: &Connection, chat_id: ChatId) -> Result<()> {
    connection
        .execute(
            "DELETE FROM messages WHERE chat_id=?1 AND id NOT IN
         (SELECT id FROM messages WHERE chat_id=?1 ORDER BY id DESC LIMIT ?2)",
            params![chat_id, MAX_CHAT_MESSAGES],
        )
        .await?;
    Ok(())
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
    connection
        .execute(
            "DELETE FROM attachments WHERE chat_id=?1 AND message_id=?2 AND source_id IS NOT ?3",
            params![
                message.chat_id,
                message.id,
                message
                    .attachment
                    .as_ref()
                    .and_then(|attachment| attachment.source_id)
            ],
        )
        .await?;
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
            "INSERT OR REPLACE INTO chats VALUES(?1,?2,?3,?4)",
            params![
                chat.id,
                serde_json::to_string(chat)?,
                revision,
                chat.last_message_id
            ],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatKind;
    use chrono::{TimeZone, Utc};

    fn message(id: i32, text: &str) -> Message {
        Message {
            edited_at: None,
            pinned: false,
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
    async fn reply_snapshot_keeps_live_edits_and_deleted_targets_across_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reply-cache.sqlite3");
        let mut store = Store::open(&path).await.unwrap();
        store
            .apply(&[
                NetworkEvent::ReplyPreviewsLoading {
                    chat_id: 42,
                    request_id: 1,
                },
                NetworkEvent::MessageUpdated(message(10, "edited")),
                NetworkEvent::MessagesDeleted {
                    channel_id: None,
                    message_ids: vec![11],
                },
                NetworkEvent::ReplyPreviews {
                    chat_id: 42,
                    request_id: 1,
                    messages: vec![
                        message(10, "stale"),
                        message(11, "deleted"),
                        message(12, "original"),
                    ],
                    unavailable: Vec::new(),
                    complete: true,
                },
            ])
            .await
            .unwrap();
        drop(store);
        let store = Store::open(&path).await.unwrap();
        let (messages, deleted) = store.reply_previews(42, &[10, 11, 12, 13]).await.unwrap();
        assert_eq!(
            messages
                .iter()
                .map(|message| (message.id, message.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(10, "edited"), (12, "original")]
        );
        assert_eq!(deleted, vec![11]);
        assert!(
            store
                .reply_previews(43, &[10, 12])
                .await
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn restart_keeps_live_changes_and_their_cursor_and_isolates_accounts() {
        let directory = std::env::temp_dir().join(format!("termgram-cache-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("one.sqlite3");
        let mut store = Store::open(&path).await.unwrap();
        let chat = Chat {
            read_inbox_max_id: Some(0),
            membership: crate::folders::ChatMembership::default(),
            id: 42,
            title: "Group".to_owned(),
            kind: ChatKind::Group,
            unread: 0,
            last_message_id: None,
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
                NetworkEvent::ArchiveChanged {
                    chat_id: 42,
                    archived: true,
                },
                NetworkEvent::DialogPins(crate::pins::DialogPins {
                    main: vec![7],
                    archive: vec![42],
                }),
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
        assert!(store.snapshot().await.unwrap().1[0].membership.archived);
        assert_eq!(store.dialog_pins().await.unwrap().archive, vec![42]);
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
        assert_eq!(
            store.dialog_pins().await.unwrap(),
            crate::pins::DialogPins::default()
        );
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
    #[tokio::test]
    async fn attachment_mapping_survives_restart_but_not_media_edits() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("cache.sqlite3");
        let path = directory.path().join("file.txt");
        std::fs::write(&path, "file").unwrap();
        let mut store = Store::open(&database).await.unwrap();
        let mut media = message(1, "file");
        media.attachment = Some(crate::model::Attachment {
            source_id: Some(55),
            kind: crate::model::AttachmentKind::File,
            file_name: Some("file.txt".to_owned()),
            mime_type: None,
            size: Some(4),
            fallback_emoji: None,
        });
        store
            .apply(&[
                NetworkEvent::NewMessage(media.clone()),
                NetworkEvent::AttachmentDownloadStarted {
                    chat_id: 42,
                    message_id: 1,
                    request_id: 1,
                    media_id: Some(55),
                },
                NetworkEvent::AttachmentDownloaded {
                    chat_id: 42,
                    message_id: 1,
                    request_id: 1,
                    path: path.clone(),
                },
            ])
            .await
            .unwrap();
        drop(store);
        let mut store = Store::open(&database).await.unwrap();
        assert_eq!(
            store.attachment(42, 1, Some(55)).await.unwrap(),
            Some(path.clone())
        );
        assert!(store.attachment(42, 1, Some(56)).await.unwrap().is_none());
        std::fs::remove_file(&path).unwrap();
        assert!(store.attachment(42, 1, Some(55)).await.unwrap().is_none());
        std::fs::write(&path, "file").unwrap();
        media.attachment.as_mut().unwrap().source_id = Some(56);
        store
            .apply(&[
                NetworkEvent::AttachmentDownloadStarted {
                    chat_id: 42,
                    message_id: 1,
                    request_id: 2,
                    media_id: Some(55),
                },
                NetworkEvent::MessageUpdated(media),
                NetworkEvent::AttachmentDownloadStarted {
                    chat_id: 42,
                    message_id: 1,
                    request_id: 3,
                    media_id: Some(56),
                },
                NetworkEvent::AttachmentDownloaded {
                    chat_id: 42,
                    message_id: 1,
                    request_id: 2,
                    path: path.clone(),
                },
            ])
            .await
            .unwrap();
        assert!(store.attachment(42, 1, Some(55)).await.unwrap().is_none());
        assert!(store.attachment(42, 1, Some(56)).await.unwrap().is_none());
        store
            .apply(&[NetworkEvent::AttachmentDownloaded {
                chat_id: 42,
                message_id: 1,
                request_id: 3,
                path: path.clone(),
            }])
            .await
            .unwrap();
        assert_eq!(store.attachment(42, 1, Some(56)).await.unwrap(), Some(path));
    }

    #[tokio::test]
    async fn account_cache_has_one_owner_until_transfers_release_the_lock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.sqlite3");
        let store = Store::open(&path).await.unwrap();
        let transfer_owner = store.owner();
        assert!(Store::open(&path).await.is_err());
        drop(store);
        assert!(Store::open(&path).await.is_err());
        drop(transfer_owner);
        assert!(Store::open(&path).await.is_ok());
    }

    #[tokio::test]
    async fn pinned_pages_persist_without_undoing_live_unpins_or_deletions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pins.sqlite3");
        let mut store = Store::open(&path).await.unwrap();
        let mut pin = message(4, "pinned");
        pin.pinned = true;
        let mut removed = message(5, "deleted");
        removed.pinned = true;
        store
            .apply(&[
                NetworkEvent::CacheMessage(pin.clone()),
                NetworkEvent::CacheMessage(removed.clone()),
                NetworkEvent::PinnedMessagesLoading {
                    chat_id: 42,
                    request_id: 1,
                },
                NetworkEvent::MessagePinsChanged {
                    chat_id: 42,
                    message_ids: vec![4],
                    pinned: false,
                },
                NetworkEvent::MessagesDeleted {
                    channel_id: None,
                    message_ids: vec![5],
                },
                NetworkEvent::PinnedMessages {
                    chat_id: 42,
                    request_id: 1,
                    page: crate::pins::MessagePage {
                        messages: vec![removed, pin.clone()],
                        total: Some(2),
                        ..Default::default()
                    },
                },
            ])
            .await
            .unwrap();
        assert!(
            store
                .pinned_messages(42, 0)
                .await
                .unwrap()
                .messages
                .is_empty()
        );
        store
            .apply(&[
                NetworkEvent::PinnedMessagesLoading {
                    chat_id: 42,
                    request_id: 2,
                },
                NetworkEvent::PinnedMessages {
                    chat_id: 42,
                    request_id: 2,
                    page: crate::pins::MessagePage {
                        messages: vec![pin],
                        total: Some(1),
                        ..Default::default()
                    },
                },
            ])
            .await
            .unwrap();
        drop(store);
        let mut store = Store::open(&path).await.unwrap();
        assert_eq!(
            store.pinned_messages(42, 0).await.unwrap().messages[0].id,
            4
        );
        store
            .apply(&[NetworkEvent::MessagePinsCleared { chat_id: 42 }])
            .await
            .unwrap();
        assert!(
            store
                .pinned_messages(42, 0)
                .await
                .unwrap()
                .messages
                .is_empty()
        );
        assert_eq!(store.history(42, None, 20).await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn invalidated_pin_reads_cannot_repopulate_the_cache() {
        for scope in [Some(42), None] {
            let directory = tempfile::tempdir().unwrap();
            let mut store = Store::open(&directory.path().join("pins.sqlite3"))
                .await
                .unwrap();
            let mut pin = message(4, "obsolete pin");
            pin.pinned = true;
            let mut other = pin.clone();
            other.chat_id = 99;
            store
                .apply(&[
                    NetworkEvent::CacheMessage(pin.clone()),
                    NetworkEvent::CacheMessage(other),
                    NetworkEvent::PinnedMessagesLoading {
                        chat_id: 42,
                        request_id: 1,
                    },
                    NetworkEvent::PinnedMessagesLoading {
                        chat_id: 42,
                        request_id: 2,
                    },
                    NetworkEvent::CacheInvalidated { chat_id: scope },
                    NetworkEvent::PinnedMessages {
                        chat_id: 42,
                        request_id: 1,
                        page: crate::pins::MessagePage {
                            messages: vec![pin.clone()],
                            total: Some(1),
                            ..Default::default()
                        },
                    },
                    NetworkEvent::PinnedContext {
                        chat_id: 42,
                        message_id: 4,
                        request_id: 2,
                        messages: vec![pin.clone()],
                    },
                ])
                .await
                .unwrap();
            assert!(store.history(42, None, 20).await.unwrap().is_empty());
            assert_eq!(
                store.history(99, None, 20).await.unwrap().is_empty(),
                scope.is_none()
            );
            pin.id = 6;
            store
                .apply(&[
                    NetworkEvent::PinnedMessagesLoading {
                        chat_id: 42,
                        request_id: 3,
                    },
                    NetworkEvent::PinnedMessages {
                        chat_id: 42,
                        request_id: 3,
                        page: crate::pins::MessagePage {
                            messages: vec![pin],
                            total: Some(1),
                            ..Default::default()
                        },
                    },
                ])
                .await
                .unwrap();
            let pins = store.pinned_messages(42, 0).await.unwrap();
            assert_eq!(pins.messages.len(), 1);
            assert_eq!(pins.messages[0].id, 6);
        }
    }

    #[tokio::test]
    async fn cached_dialogs_count_new_messages_once_and_clear_deleted_previews() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.sqlite3");
        let mut store = Store::open(&path).await.unwrap();
        let chat = Chat {
            id: 42,
            title: "Group".to_owned(),
            kind: ChatKind::Group,
            unread: 0,
            read_inbox_max_id: Some(0),
            membership: crate::folders::ChatMembership::default(),
            last_message: "first".to_owned(),
            last_message_id: Some(1),
            last_activity: Some(message(1, "first").timestamp),
        };
        store
            .apply(&[
                NetworkEvent::DialogsLoading,
                NetworkEvent::Dialogs(vec![chat]),
                NetworkEvent::NewMessage(message(2, "second")),
                NetworkEvent::NewMessage(message(2, "second")),
                NetworkEvent::MessageUpdated(message(1, "edited first")),
            ])
            .await
            .unwrap();
        let chats = store.snapshot().await.unwrap().1;
        assert_eq!(chats[0].unread, 1);
        assert_eq!(chats[0].last_message_id, Some(2));
        assert_eq!(chats[0].last_message, "second");
        assert_eq!(
            store
                .history_after(42, 1, 80)
                .await
                .unwrap()
                .iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        store
            .apply(&[NetworkEvent::ReadMarked {
                chat_id: 42,
                max_id: 1,
                snapshot: Some(crate::read_state::Snapshot {
                    max_id: 1,
                    unread: 0,
                    top_message: 1,
                }),
            }])
            .await
            .unwrap();
        let chats = store.snapshot().await.unwrap().1;
        assert_eq!(chats[0].unread, 1);
        assert_eq!(chats[0].read_inbox_max_id, Some(1));
        store
            .apply(&[
                NetworkEvent::MessagesDeleted {
                    channel_id: None,
                    message_ids: vec![2],
                },
                NetworkEvent::UnreadChanged {
                    chat_id: 42,
                    max_id: 2,
                    unread: 0,
                },
                NetworkEvent::ChatUnreadChanged {
                    chat_id: 42,
                    unread: true,
                },
            ])
            .await
            .unwrap();
        drop(store);
        let store = Store::open(&path).await.unwrap();
        let chats = store.snapshot().await.unwrap().1;
        assert!(chats[0].last_message.is_empty());
        assert_eq!(chats[0].unread, 0);
        assert_eq!(chats[0].last_message_id, None);
        assert_eq!(chats[0].read_inbox_max_id, Some(2));
        assert!(chats[0].membership.unread_mark);
    }
}
