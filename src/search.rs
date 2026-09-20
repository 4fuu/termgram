//! Bounded, cancellable regex searches over the account's local message cache.
use crate::{
    event::NetworkEvent,
    model::{ChatId, Message},
};
use anyhow::{Result, bail};
use libsql::{OpenFlags, params};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const PAGE_SIZE: usize = 100;
const SCAN_BATCH: i64 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor(pub i64, pub ChatId, pub i32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub id: u64,
    pub pattern: String,
    /// None means all cached conversations; Some(empty) is an empty scope.
    pub chats: Option<Vec<ChatId>>,
    pub before: Option<Cursor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub messages: Vec<Message>,
    pub next: Option<Cursor>,
    pub cached_messages: usize,
    pub oldest: Option<i64>,
    pub newest: Option<i64>,
}

pub(crate) struct Worker {
    path: PathBuf,
    jobs: tokio::task::JoinSet<(u64, Arc<AtomicBool>, Result<Page>)>,
    cancel: Arc<AtomicBool>,
    pending: Option<Request>,
}

impl Worker {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            jobs: tokio::task::JoinSet::new(),
            cancel: Arc::default(),
            pending: None,
        }
    }
    pub fn queue(&mut self, request: Request) {
        self.cancel();
        self.pending = Some(request);
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.pending = None;
    }
    pub fn start_pending(&mut self) {
        if !self.jobs.is_empty() {
            return;
        }
        let Some(request) = self.pending.take() else {
            return;
        };
        self.cancel = Arc::default();
        let cancel = self.cancel.clone();
        let path = self.path.clone();
        let runtime = tokio::runtime::Handle::current();
        self.jobs.spawn_blocking(move || {
            let result = runtime.block_on(scan(&path, &request, &cancel));
            (request.id, cancel, result)
        });
    }
    pub fn running(&self) -> bool {
        !self.jobs.is_empty()
    }
    pub async fn next(&mut self) -> Option<NetworkEvent> {
        match self.jobs.join_next().await? {
            Ok((_, cancel, _)) if cancel.load(Ordering::Relaxed) => None,
            Ok((request_id, _, Ok(page))) => Some(NetworkEvent::SearchResults { request_id, page }),
            Ok((request_id, _, Err(error))) => Some(NetworkEvent::SearchFailed {
                request_id,
                error: format!("{error:#}"),
            }),
            Err(error) => Some(NetworkEvent::Error(format!(
                "Local search task failed: {error}"
            ))),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn scan(path: &Path, request: &Request, cancel: &AtomicBool) -> Result<Page> {
    if request.pattern.len() > 4096 {
        bail!("Regular expression is longer than 4096 bytes");
    }
    let regex = regex::RegexBuilder::new(&request.pattern)
        .size_limit(4 * 1024 * 1024)
        .dfa_size_limit(2 * 1024 * 1024)
        .build()?;
    let database = libsql::Builder::new_local(path)
        .flags(OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await?;
    let connection = database.connect()?;
    connection.busy_timeout(Duration::from_millis(250))?;
    let scope = request
        .chats
        .as_ref()
        .map(|ids| ids.iter().copied().collect::<HashSet<_>>());
    let mut page = Page {
        messages: Vec::new(),
        next: None,
        cached_messages: 0,
        oldest: None,
        newest: None,
    };
    let mut cursor = Cursor(i64::MAX, i64::MAX, i32::MAX);
    let mut last_match = None;
    let mut full = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("Search cancelled");
        }
        let mut rows = connection.query(
            "SELECT m.timestamp,m.chat_id,m.id,m.data FROM messages m JOIN chats c ON c.id=m.chat_id WHERE m.data IS NOT NULL AND (m.timestamp,m.chat_id,m.id)<(?1,?2,?3) ORDER BY m.timestamp DESC,m.chat_id DESC,m.id DESC LIMIT ?4",
            params![cursor.0, cursor.1, cursor.2, SCAN_BATCH]).await?;
        let mut scanned = 0;
        while let Some(row) = rows.next().await? {
            if cancel.load(Ordering::Relaxed) {
                bail!("Search cancelled");
            }
            cursor = Cursor(row.get(0)?, row.get(1)?, row.get(2)?);
            scanned += 1;
            if scope.as_ref().is_some_and(|ids| !ids.contains(&cursor.1)) {
                continue;
            }
            page.cached_messages += 1;
            page.oldest = Some(cursor.0);
            page.newest.get_or_insert(cursor.0);
            if full
                || request.before.is_some_and(|before| {
                    (cursor.0, cursor.1, cursor.2) >= (before.0, before.1, before.2)
                })
            {
                continue;
            }
            let message: Message = serde_json::from_str(&row.get::<String>(3)?)?;
            if regex.is_match(&message.text) {
                if page.messages.len() == PAGE_SIZE {
                    page.next = last_match;
                    full = true;
                } else {
                    page.messages.push(message);
                    last_match = Some(cursor);
                }
            }
        }
        if scanned < SCAN_BATCH {
            break;
        }
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cache::Store,
        model::{Chat, ChatKind, Delivery},
    };
    use chrono::TimeZone;

    #[tokio::test]
    async fn regex_search_pages_scopes_tombstones_and_stale_context() {
        let dir = std::env::temp_dir().join(format!("termgram-search-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.sqlite3");
        let mut store = Store::open(&path).await.unwrap();
        let chat = |id| Chat {
            id,
            title: format!("Chat {id}"),
            kind: ChatKind::Group,
            unread: 0,
            last_message_id: None,
            last_message: String::new(),
            last_activity: None,
            read_inbox_max_id: Some(0),
            membership: crate::folders::ChatMembership::default(),
        };
        let mut events = vec![NetworkEvent::Dialogs(vec![chat(42), chat(43)])];
        for id in 1..=206 {
            events.push(NetworkEvent::NewMessage(Message {
                entities: Vec::new(),
                notification: None,
                mention: None,
                edited_at: None,
                pinned: false,
                id,
                chat_id: if id == 206 { 43 } else { 42 },
                sender: "Ada".to_owned(),
                reply_to: None,
                text: format!("東京 error {id}"),
                timestamp: chrono::Utc.timestamp_opt(i64::from(id), 0).unwrap(),
                outgoing: false,
                delivery: Delivery::Read,
                attachment: None,
                links: Vec::new(),
                buttons: Vec::new(),
            }));
        }
        events.push(NetworkEvent::MessagesDeleted {
            channel_id: None,
            message_ids: vec![11],
        });
        store.apply(&events).await.unwrap();
        let mut request = Request {
            id: 1,
            pattern: r"東京.*error \d+$".to_owned(),
            chats: Some(vec![42]),
            before: None,
        };
        let cancelled = AtomicBool::new(false);
        let first = scan(&path, &request, &cancelled).await.unwrap();
        assert_eq!(first.cached_messages, 204);
        assert_eq!(first.messages.len(), 100);
        assert_eq!(first.messages[0].id, 205);
        request.before = first.next;
        let second = scan(&path, &request, &cancelled).await.unwrap();
        assert_eq!(second.messages.len(), 100);
        assert!(
            second
                .messages
                .iter()
                .all(|message| !first.messages.iter().any(|other| other.id == message.id))
        );
        request.before = second.next;
        let last = scan(&path, &request, &cancelled).await.unwrap();
        assert_eq!(last.messages.len(), 4);
        assert!(last.next.is_none());
        assert!(last.messages.iter().all(|message| message.id != 11));
        request.before = None;
        request.chats = Some(vec![43]);
        assert_eq!(
            scan(&path, &request, &cancelled)
                .await
                .unwrap()
                .messages
                .len(),
            1
        );
        let context = store.context(42, 50).await.unwrap();
        assert!(context.iter().any(|message| message.id == 49));
        assert!(context.iter().any(|message| message.id == 51));
        store
            .apply(&[NetworkEvent::MessagesDeleted {
                channel_id: None,
                message_ids: vec![50],
            }])
            .await
            .unwrap();
        assert!(store.context(42, 50).await.is_err());
        request.pattern = "(".to_owned();
        assert!(scan(&path, &request, &cancelled).await.is_err());
        request.pattern = "error".to_owned();
        cancelled.store(true, Ordering::Relaxed);
        assert!(scan(&path, &request, &cancelled).await.is_err());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
