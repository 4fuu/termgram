//! Notification policy and coalescing; platform delivery belongs to the runtime.
use super::{App, Mode, Screen, TelegramCommand};
use crate::{
    event::{ConnectionStatus, NetworkEvent},
    model::{ChatId, Message},
    notifications::{Alert, Resolved, SettingsKey, When},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_CHATS: usize = 32;
const MAX_MESSAGES: usize = 32;
const MAX_AGE: i64 = 30;

#[derive(Clone)]
struct Group {
    messages: Vec<Message>,
    due: Instant,
    expires: Instant,
}

#[derive(Clone)]
struct Settings {
    request: u64,
    loading: bool,
    value: Option<Resolved>,
    expires: Instant,
}

#[derive(Clone)]
struct Submitted {
    ids: Vec<i32>,
    valid: Arc<AtomicBool>,
    expires: Instant,
}

#[derive(Clone)]
pub(super) struct State {
    online: bool,
    after: i64,
    high_water: BTreeMap<ChatId, i32>,
    groups: BTreeMap<ChatId, Group>,
    settings: BTreeMap<SettingsKey, Settings>,
    submitted: BTreeMap<(ChatId, i32), Submitted>,
    next_request: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            online: false,
            after: chrono::Utc::now().timestamp(),
            high_water: BTreeMap::new(),
            groups: BTreeMap::new(),
            settings: BTreeMap::new(),
            submitted: BTreeMap::new(),
            next_request: 0,
        }
    }
}

fn key(message: &Message) -> SettingsKey {
    SettingsKey {
        chat: message.chat_id,
        sender: message
            .mention
            .as_ref()
            .filter(|mention| mention.unread)
            .and_then(|_| message.notification.as_ref()?.sender),
    }
}

fn fresh(message: &Message, after: i64, now: i64) -> bool {
    let date = message.timestamp.timestamp();
    message.id > 0
        && !message.outgoing
        && message.notification.is_some()
        && date > after
        && date <= now.saturating_add(5)
        && now.saturating_sub(date) <= MAX_AGE
        && message
            .mention
            .as_ref()
            .is_none_or(|mention| mention.unread)
}

fn remove_matching(state: &mut State, mut affected: impl FnMut(ChatId, i32) -> bool) {
    for (chat, group) in &mut state.groups {
        group
            .messages
            .retain(|message| !affected(*chat, message.id));
    }
    state.groups.retain(|_, group| !group.messages.is_empty());
    state.submitted.retain(|(chat, _), sent| {
        let keep = !sent.ids.iter().any(|id| affected(*chat, *id));
        if !keep {
            sent.valid.store(false, Ordering::Release);
        }
        keep
    });
}

impl App {
    pub fn cancel_alerts(&mut self) {
        self.alerts.groups.clear();
        for sent in self.alerts.submitted.values() {
            sent.valid.store(false, Ordering::Release);
        }
        self.alerts.submitted.clear();
    }

    fn invalidate_alert_settings(&mut self) {
        self.alerts.settings.clear();
        for sent in self.alerts.submitted.values() {
            sent.valid.store(false, Ordering::Release);
        }
        self.alerts.submitted.clear();
    }

    pub(super) fn observe_alerts(&mut self, event: &NetworkEvent) {
        self.observe_alerts_at(event, chrono::Utc::now().timestamp(), Instant::now());
    }

    fn observe_alerts_at(&mut self, event: &NetworkEvent, now: i64, instant: Instant) {
        match event {
            NetworkEvent::Ready { .. } | NetworkEvent::Status(ConnectionStatus::Online) => {
                if !self.alerts.online {
                    self.alerts.after = now;
                    self.alerts.online = true;
                }
            }
            NetworkEvent::Status(_) | NetworkEvent::Auth(_) | NetworkEvent::Fatal(_) => {
                self.alerts.online = false;
                self.cancel_alerts();
                self.alerts.settings.clear();
            }
            NetworkEvent::CachedSnapshot { chats, .. } => {
                for chat in chats.iter().take(4096) {
                    if let Some(id) = chat.last_message_id {
                        self.alerts.high_water.insert(chat.id, id);
                    }
                }
            }
            NetworkEvent::NewMessage(message) => self.queue_alert(message, now, instant),
            NetworkEvent::MessageUpdated(message) => {
                if let Some(group) = self.alerts.groups.get_mut(&message.chat_id)
                    && let Some(current) = group
                        .messages
                        .iter_mut()
                        .find(|current| current.id == message.id)
                {
                    *current = message.clone();
                }
                // Already queued native text cannot be rewritten after an edit.
                for ((chat, _), sent) in &self.alerts.submitted {
                    if *chat == message.chat_id && sent.ids.contains(&message.id) {
                        sent.valid.store(false, Ordering::Release);
                    }
                }
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            }
            | NetworkEvent::MessageContentsRead {
                channel_id,
                message_ids,
            } => {
                remove_matching(&mut self.alerts, |chat, id| {
                    channel_id.map_or(chat > -1_000_000_000_000, |channel| chat == channel)
                        && message_ids.contains(&id)
                });
            }
            NetworkEvent::ReadMarked {
                chat_id, max_id, ..
            }
            | NetworkEvent::UnreadChanged {
                chat_id, max_id, ..
            } => {
                remove_matching(&mut self.alerts, |chat, id| {
                    chat == *chat_id && id <= *max_id
                });
            }
            NetworkEvent::CacheInvalidated { chat_id } => {
                remove_matching(&mut self.alerts, |chat, _| {
                    chat_id.is_none_or(|id| id == chat)
                });
            }
            NetworkEvent::NotificationSettingsChanged
            | NetworkEvent::ChatMuteChanged { .. }
            | NetworkEvent::ChatMuteFinished { result: Ok(_), .. } => {
                self.invalidate_alert_settings();
            }
            NetworkEvent::AlertSettingsReady {
                key,
                request_id,
                result,
            } => {
                if let Some(settings) = self.alerts.settings.get_mut(key)
                    && settings.request == *request_id
                {
                    settings.loading = false;
                    settings.value = result.as_ref().ok().copied();
                    settings.expires =
                        instant + Duration::from_secs(if result.is_ok() { 60 } else { 5 });
                }
            }
            _ => {}
        }
    }

    fn queue_alert(&mut self, message: &Message, now: i64, instant: Instant) {
        let previous = self.alerts.high_water.entry(message.chat_id).or_default();
        let new = message.id > *previous;
        *previous = (*previous).max(message.id);
        while self.alerts.high_water.len() > 4096 {
            self.alerts.high_water.pop_first();
        }
        if !new
            || !self.alerts.online
            || !self.keymap.notifications.enabled
            || !fresh(message, self.alerts.after, now)
            || self.account_user_id == Some(message.chat_id)
        {
            return;
        }
        if !self.alerts.groups.contains_key(&message.chat_id)
            && self.alerts.groups.len() >= MAX_CHATS
        {
            return;
        }
        let group = self
            .alerts
            .groups
            .entry(message.chat_id)
            .or_insert_with(|| Group {
                messages: Vec::new(),
                due: instant + Duration::from_millis(self.keymap.notifications.group_delay_ms),
                expires: instant + Duration::from_secs(30),
            });
        if group.messages.len() == MAX_MESSAGES {
            group.messages.remove(0);
        }
        group.messages.push(message.clone());
    }

    /// Evaluate after drawing so a freshly arrived visible message is silent.
    fn prune_alerts(&mut self, now: i64, instant: Instant) {
        if !self.keymap.notifications.enabled
            || !self.alerts.online
            || self.should_quit
            || self.screen != Screen::Main
            || self.account_user_id.is_none()
        {
            self.cancel_alerts();
            return;
        }
        let visible =
            if self.terminal_focused && matches!(self.mode, Mode::Navigate | Mode::Compose) {
                self.reads.visible
            } else {
                None
            };
        let suppress_all =
            self.terminal_focused && self.keymap.notifications.when == When::Unfocused;
        let seen = |chat: ChatId, id: i32| {
            suppress_all
                || visible.is_some_and(|(visible_chat, max)| visible_chat == chat && id <= max)
                || self
                    .chats
                    .iter()
                    .find(|item| item.id == chat)
                    .and_then(|item| item.read_inbox_max_id)
                    .is_some_and(|max| id <= max)
        };
        remove_matching(&mut self.alerts, seen);
        for group in self.alerts.groups.values_mut() {
            group
                .messages
                .retain(|message| fresh(message, self.alerts.after, now));
        }
        self.alerts
            .groups
            .retain(|_, group| !group.messages.is_empty() && group.expires > instant);
        self.alerts.submitted.retain(|_, sent| {
            if sent.expires <= instant {
                sent.valid.store(false, Ordering::Release);
                false
            } else {
                sent.valid.load(Ordering::Acquire)
            }
        });
        self.alerts
            .settings
            .retain(|_, settings| settings.expires > instant);
    }

    pub fn request_alert_settings(&mut self) -> Vec<TelegramCommand> {
        self.request_alert_settings_at(chrono::Utc::now().timestamp(), Instant::now())
    }

    fn request_alert_settings_at(&mut self, now: i64, instant: Instant) -> Vec<TelegramCommand> {
        self.prune_alerts(now, instant);
        let wanted: BTreeSet<_> = self
            .alerts
            .groups
            .values()
            .flat_map(|group| group.messages.iter().map(key))
            .collect();
        let capacity = 2usize.saturating_sub(
            self.alerts
                .settings
                .values()
                .filter(|settings| settings.loading)
                .count(),
        );
        if capacity == 0 {
            return Vec::new();
        }
        let mut commands = Vec::new();
        for key in wanted {
            if self.alerts.settings.contains_key(&key) {
                continue;
            }
            if self.alerts.settings.len() >= 128 {
                if let Some(old) = self
                    .alerts
                    .settings
                    .iter()
                    .filter(|(_, entry)| !entry.loading)
                    .min_by_key(|(_, entry)| entry.expires)
                    .map(|(key, _)| *key)
                {
                    self.alerts.settings.remove(&old);
                } else {
                    break;
                }
            }
            self.alerts.next_request = self.alerts.next_request.wrapping_add(1);
            let request_id = self.alerts.next_request;
            self.alerts.settings.insert(
                key,
                Settings {
                    request: request_id,
                    loading: true,
                    value: None,
                    expires: instant + Duration::from_secs(30),
                },
            );
            commands.push(TelegramCommand::ResolveAlertSettings { key, request_id });
            if commands.len() == capacity {
                break;
            }
        }
        commands
    }

    #[must_use]
    pub fn next_alert_deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        self.alerts
            .groups
            .values()
            .flat_map(|group| [group.due, group.expires])
            .chain(
                self.alerts
                    .settings
                    .values()
                    .filter(|settings| !settings.loading)
                    .map(|settings| settings.expires),
            )
            .chain(self.alerts.submitted.values().map(|sent| sent.expires))
            .filter(|time| *time > now)
            .min()
    }

    pub fn take_alerts(&mut self) -> Vec<Alert> {
        self.take_alerts_at(chrono::Utc::now().timestamp(), Instant::now())
    }

    fn take_alerts_at(&mut self, now: i64, instant: Instant) -> Vec<Alert> {
        self.prune_alerts(now, instant);
        let mut ready = Vec::new();
        let mut waiting = BTreeMap::new();
        for (chat, group) in std::mem::take(&mut self.alerts.groups) {
            if group.due > instant
                || group.messages.iter().any(|message| {
                    self.alerts
                        .settings
                        .get(&key(message))
                        .is_none_or(|settings| settings.value.is_none())
                })
            {
                waiting.insert(chat, group);
                continue;
            }
            let eligible: Vec<_> = group
                .messages
                .iter()
                .filter_map(|message| {
                    let settings = self.alerts.settings.get(&key(message))?.value?;
                    (settings.chat.mute_until <= now
                        || settings.sender_mute_until.is_some_and(|until| until <= now))
                    .then_some((message, settings))
                })
                .collect();
            let Some((last, settings)) = eligible.last() else {
                continue;
            };
            let metadata = last.notification.as_ref().expect("live candidate metadata");
            let show_preview =
                self.keymap.notifications.previews && settings.chat.previews && !metadata.protected;
            let excerpt = last.preview_text();
            let body = if show_preview {
                format!("{}: {}", last.sender, excerpt)
            } else {
                "New message".to_owned()
            };
            let body = if eligible.len() > 1 {
                format!("{} messages · {body}", eligible.len())
            } else {
                body
            };
            let title = self
                .chats
                .iter()
                .find(|item| item.id == chat)
                .map_or_else(|| "Telegram".to_owned(), |item| item.title.clone());
            let title = format!(
                "Termgram · {} · {title}",
                self.user_name.as_deref().unwrap_or("Account")
            );
            let sound = self.keymap.notifications.sound
                && eligible.iter().any(|(message, settings)| {
                    settings.chat.sound
                        && message
                            .notification
                            .as_ref()
                            .is_some_and(|meta| !meta.silent)
                });
            let valid = Arc::new(AtomicBool::new(true));
            let ids = eligible
                .iter()
                .map(|(message, _)| message.id)
                .collect::<Vec<_>>();
            self.alerts.submitted.insert(
                (chat, last.id),
                Submitted {
                    ids: ids.clone(),
                    valid: valid.clone(),
                    expires: group.expires,
                },
            );
            ready.push(Alert {
                account: self.account_user_id.expect("known account"),
                chat,
                ids,
                title: crate::model::sanitize_terminal_line(&title)
                    .chars()
                    .take(160)
                    .collect(),
                body: crate::model::sanitize_terminal_line(&body)
                    .chars()
                    .take(320)
                    .collect(),
                sound,
                valid,
                expires: group.expires,
            });
        }
        self.alerts.groups = waiting;
        ready
    }

    pub fn alert_finished(&mut self, alert: &Alert) {
        if let Some(id) = alert.ids.last()
            && self
                .alerts
                .submitted
                .get(&(alert.chat, *id))
                .is_some_and(|sent| Arc::ptr_eq(&sent.valid, &alert.valid))
        {
            self.alerts.submitted.remove(&(alert.chat, *id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{Chat, ChatKind, Delivery, Mention},
        notifications::{Metadata, Preferences},
    };
    use chrono::{TimeZone, Utc};

    fn app(instant: Instant) -> App {
        let mut app = App::with_ephemeral_settings(crate::config::Settings::default());
        app.screen = Screen::Main;
        app.connection = ConnectionStatus::Online;
        app.account_user_id = Some(7);
        app.user_name = Some("Ada".into());
        app.terminal_focused = false;
        app.chats.push(Chat {
            id: 42,
            title: "Group".into(),
            kind: ChatKind::Group,
            unread: 1,
            read_inbox_max_id: Some(0),
            last_message_id: None,
            last_message: String::new(),
            last_activity: None,
            membership: crate::folders::ChatMembership::default(),
        });
        app.observe_alerts_at(
            &NetworkEvent::Ready {
                user_name: "Ada".into(),
            },
            1000,
            instant,
        );
        app
    }

    fn message(id: i32, date: i64) -> Message {
        Message {
            id,
            chat_id: 42,
            sender_username: None,
            sender: "Sender".into(),
            text: "private text".into(),
            timestamp: Utc.timestamp_opt(date, 0).unwrap(),
            outgoing: false,
            delivery: Delivery::Sent,
            mention: None,
            reactions: None,
            poll: None,
            entities: Vec::new(),
            notification: Some(Metadata {
                sender: Some(90),
                silent: false,
                protected: false,
            }),
            pinned: false,
            reply_to: None,
            edited_at: None,
            attachment: None,
            links: Vec::new(),
            buttons: Vec::new(),
        }
    }

    #[test]
    fn replay_duplicates_and_visible_or_remotely_read_messages_stay_silent() {
        let instant = Instant::now();
        let mut app = app(instant);
        app.observe_alerts_at(&NetworkEvent::NewMessage(message(1, 999)), 1010, instant);
        assert!(app.alerts.groups.is_empty(), "startup catch-up is silent");
        let live = message(2, 1010);
        app.observe_alerts_at(&NetworkEvent::NewMessage(live.clone()), 1010, instant);
        app.observe_alerts_at(&NetworkEvent::NewMessage(live), 1010, instant);
        assert_eq!(app.alerts.groups[&42].messages.len(), 1);
        let cached: Message =
            serde_json::from_str(&serde_json::to_string(&message(3, 1010)).unwrap()).unwrap();
        assert!(
            cached.notification.is_none(),
            "delivery metadata is never replayed from disk"
        );
        app.observe_alerts_at(&NetworkEvent::NewMessage(cached), 1010, instant);
        assert_eq!(app.alerts.groups[&42].messages.len(), 1);
        app.observe_alerts_at(
            &NetworkEvent::UnreadChanged {
                chat_id: 42,
                max_id: 2,
                unread: 0,
            },
            1010,
            instant,
        );
        assert!(app.request_alert_settings_at(1010, instant).is_empty());
        app.observe_alerts_at(
            &NetworkEvent::Status(ConnectionStatus::Reconnecting),
            1011,
            instant,
        );
        app.observe_alerts_at(&NetworkEvent::NewMessage(message(4, 1012)), 1012, instant);
        app.observe_alerts_at(
            &NetworkEvent::Status(ConnectionStatus::Online),
            1013,
            instant,
        );
        app.observe_alerts_at(&NetworkEvent::NewMessage(message(5, 1012)), 1014, instant);
        assert!(
            app.alerts.groups.is_empty(),
            "messages missed while offline do not replay as alerts"
        );
        app.observe_alerts_at(&NetworkEvent::NewMessage(message(6, 1014)), 1014, instant);
        app.terminal_focused = true;
        app.active_chat_id = Some(42);
        app.set_visible_read_boundary(42, Some(6));
        assert!(app.request_alert_settings_at(1014, instant).is_empty());
        assert!(
            app.alerts.groups.is_empty(),
            "drawn messages are silent even before their read RPC completes"
        );
        app.observe_alerts_at(&NetworkEvent::NewMessage(message(7, 1014)), 1014, instant);
        app.keymap.notifications.when = When::Unfocused;
        assert!(app.request_alert_settings_at(1014, instant).is_empty());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn grouped_mentions_obey_fresh_settings_preview_sound_and_cancellation() {
        let instant = Instant::now();
        let mut app = app(instant);
        let mut first = message(10, 1010);
        first.mention = Some(Mention {
            unread: true,
            requires_playback: false,
        });
        first.notification.as_mut().unwrap().silent = true;
        let mut second = first.clone();
        second.id = 11;
        app.observe_alerts_at(&NetworkEvent::NewMessage(first), 1010, instant);
        app.observe_alerts_at(&NetworkEvent::NewMessage(second), 1010, instant);
        let commands = app.request_alert_settings_at(1010, instant);
        let [TelegramCommand::ResolveAlertSettings { key, request_id }] = commands.as_slice()
        else {
            panic!("one shared settings query")
        };
        assert_eq!(key.sender, Some(90));
        let key = *key;
        let settings = Resolved {
            chat: Preferences {
                mute_until: i64::MAX,
                previews: false,
                sound: true,
            },
            sender_mute_until: Some(0),
        };
        app.observe_alerts_at(&NetworkEvent::NotificationSettingsChanged, 1010, instant);
        app.observe_alerts_at(
            &NetworkEvent::AlertSettingsReady {
                key,
                request_id: *request_id,
                result: Ok(settings),
            },
            1010,
            instant,
        );
        assert!(
            app.alerts.settings.is_empty(),
            "stale RPC cannot restore notification preferences"
        );
        let commands = app.request_alert_settings_at(1010, instant);
        let [TelegramCommand::ResolveAlertSettings { request_id, .. }] = commands.as_slice() else {
            panic!("retry fresh settings")
        };
        app.observe_alerts_at(
            &NetworkEvent::AlertSettingsReady {
                key,
                request_id: *request_id,
                result: Ok(settings),
            },
            1010,
            instant,
        );
        assert!(
            app.take_alerts_at(1010, instant + Duration::from_millis(799))
                .is_empty()
        );
        let alerts = app.take_alerts_at(1011, instant + Duration::from_millis(801));
        let [alert] = alerts.as_slice() else {
            panic!("one grouped alert")
        };
        assert_eq!(alert.ids, [10, 11]);
        assert!(alert.body.contains("2 messages"));
        assert!(!alert.body.contains("private text"));
        assert!(
            !alert.sound,
            "silent messages never gain sound while grouping"
        );
        app.observe_alerts_at(
            &NetworkEvent::MessagesDeleted {
                channel_id: Some(-1_000_000_000_042),
                message_ids: vec![11],
            },
            1011,
            instant,
        );
        assert!(alert.valid.load(Ordering::Acquire));
        app.observe_alerts_at(
            &NetworkEvent::MessagesDeleted {
                channel_id: None,
                message_ids: vec![11],
            },
            1011,
            instant,
        );
        assert!(
            !alert.valid.load(Ordering::Acquire),
            "queued native alert is cancellable"
        );
        app.observe_alerts_at(&NetworkEvent::NewMessage(message(12, 1012)), 1012, instant);
        let commands = app.request_alert_settings_at(1012, instant);
        let [TelegramCommand::ResolveAlertSettings { key, request_id }] = commands.as_slice()
        else {
            panic!("normal group settings")
        };
        app.observe_alerts_at(
            &NetworkEvent::AlertSettingsReady {
                key: *key,
                request_id: *request_id,
                result: Ok(Resolved {
                    sender_mute_until: None,
                    ..settings
                }),
            },
            1012,
            instant,
        );
        assert!(
            app.take_alerts_at(1013, instant + Duration::from_secs(1))
                .is_empty(),
            "ordinary messages in a muted chat stay silent"
        );
        assert!(app.alerts.groups.is_empty());
        app.observe_alerts_at(&NetworkEvent::NotificationSettingsChanged, 1014, instant);
        let mut protected = message(13, 1014);
        protected.notification.as_mut().unwrap().protected = true;
        app.observe_alerts_at(&NetworkEvent::NewMessage(protected), 1014, instant);
        let commands = app.request_alert_settings_at(1014, instant);
        let [TelegramCommand::ResolveAlertSettings { key, request_id }] = commands.as_slice()
        else {
            panic!("reload after settings change")
        };
        app.observe_alerts_at(
            &NetworkEvent::AlertSettingsReady {
                key: *key,
                request_id: *request_id,
                result: Ok(Resolved {
                    chat: Preferences {
                        mute_until: 0,
                        previews: true,
                        sound: true,
                    },
                    sender_mute_until: None,
                }),
            },
            1014,
            instant,
        );
        let alerts = app.take_alerts_at(1015, instant + Duration::from_secs(1));
        let [alert] = alerts.as_slice() else {
            panic!("new unmuted alert")
        };
        assert!(
            !alert.body.contains("private text"),
            "protected content ignores enabled previews"
        );
        assert!(alert.valid.load(Ordering::Acquire));
        app.reset_for_account_switch(2);
        assert!(
            !alert.valid.load(Ordering::Acquire),
            "old account work cannot deliver after a switch"
        );
        assert!(
            crate::keymap::Keymap::parse("return { notifications = { group_delay_ms = 0 } }")
                .is_err()
        );
        assert!(crate::keymap::Keymap::parse("return { notifications = { backend = 'osc9', when = 'unfocused', previews = false } }").is_ok());
    }
}
