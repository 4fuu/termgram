//! The local store remains available while Telegram connects or authenticates.

use super::{
    COMMAND_QUEUE_CAPACITY, Config, EVENT_QUEUE_CAPACITY, NetworkEvent, Result, TelegramCommand,
    VecDeque, mpsc, run,
};
use crate::{
    cache::{Store, SyncCursor},
    model::Chat,
};

pub(super) struct Bootstrap {
    pub cursor: SyncCursor,
    pub chats: Vec<Chat>,
    pub account_id: Option<i64>,
}
use tokio::{
    task::JoinSet,
    time::{Duration, MissedTickBehavior},
};

pub(super) async fn serve(
    config: Config,
    mut commands: mpsc::Receiver<TelegramCommand>,
    events: mpsc::Sender<NetworkEvent>,
    active_chat: tokio::sync::watch::Receiver<Option<super::ChatId>>,
) -> Result<()> {
    config.prepare_session_dir()?;
    let mut cache_path = config.session_path.as_os_str().to_os_string();
    cache_path.push(".cache.sqlite3");
    let mut store = Store::open(std::path::Path::new(&cache_path)).await?;
    let (user_name, chats) = store.snapshot().await?;
    if !chats.is_empty() {
        events
            .send(NetworkEvent::CachedSnapshot {
                user_name,
                chats: chats.clone(),
            })
            .await?;
    }
    events
        .send(NetworkEvent::Folders(store.folders().await?))
        .await?;
    let bootstrap = Bootstrap {
        cursor: store.cursor().await?,
        chats,
        account_id: store.account_id().await?,
    };
    let (network_tx, network_commands) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
    let (network_events, mut network_rx) = mpsc::channel(EVENT_QUEUE_CAPACITY);
    let mut tasks = JoinSet::new();
    tasks.spawn(run(
        config,
        network_commands,
        network_events,
        bootstrap,
        active_chat,
    ));
    let mut pending = VecDeque::new();
    let mut changes = Vec::new();
    let mut ready = false;
    let mut flush = tokio::time::interval(Duration::from_millis(100));
    flush.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break; };
                if let TelegramCommand::LoadOlder { chat_id, request_id, before_id } = &command {
                    store.apply(&changes).await?;
                    changes.clear();
                    if let Some(messages) = store.older_page(*chat_id, *before_id).await? {
                        events.send(NetworkEvent::OlderHistory { chat_id: *chat_id, request_id: *request_id, before_id: *before_id, messages }).await?;
                        continue;
                    }
                }
                if let TelegramCommand::LoadHistory { chat_id, request_id } = &command {
                    store.apply(&changes).await?;
                    changes.clear();
                    let messages = store.history(*chat_id, None, super::HISTORY_LIMIT).await?;
                    if !messages.is_empty() {
                        events.send(NetworkEvent::CachedHistory { chat_id: *chat_id, request_id: *request_id, messages }).await?;
                    }
                }
                if authentication_command(&command) {
                    pending.push_front(command);
                } else if pending.len() < COMMAND_QUEUE_CAPACITY {
                    pending.push_back(command);
                } else if let Some(event) = command.failure("Telegram command queue is busy".to_owned()) {
                    events.send(event).await?;
                }
            }
            permit = network_tx.reserve(), if pending.front().is_some_and(|command| ready || authentication_command(command)) => {
                if let Ok(permit) = permit {
                    permit.send(pending.pop_front().expect("pending command"));
                } else { break; }
            }
            event = network_rx.recv() => {
                let Some(event) = event else {
                    store.apply(&changes).await?;
                    return tasks.join_next().await.transpose()?.unwrap_or(Ok(()));
                };
                match &event {
                    NetworkEvent::Ready { .. } => ready = true,
                    NetworkEvent::Auth(_) => ready = false,
                    NetworkEvent::CacheAccountReset { .. } => pending.clear(),
                    _ => {}
                }
                let checkpoint = matches!(&event, NetworkEvent::SyncCheckpoint(_));
                changes.push(event.clone());
                if checkpoint || changes.len() >= 256 {
                    store.apply(&changes).await?;
                    changes.clear();
                }
                events.send(event).await?;
            }
            _ = flush.tick(), if !changes.is_empty() => {
                store.apply(&changes).await?;
                changes.clear();
            }
        }
    }
    store.apply(&changes).await?;
    tasks.shutdown().await;
    Ok(())
}

fn authentication_command(command: &TelegramCommand) -> bool {
    matches!(
        command,
        TelegramCommand::StartQrAuth
            | TelegramCommand::SubmitPhone(_)
            | TelegramCommand::SubmitCode(_)
            | TelegramCommand::SubmitPassword(_)
            | TelegramCommand::RestartAuth
            | TelegramCommand::Shutdown
    )
}
