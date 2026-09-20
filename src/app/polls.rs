use super::{App, Focus, Mode, Screen, TelegramCommand};
use crate::{
    actions::Action,
    event::{ConnectionStatus, NetworkEvent},
    model::ChatId,
    polls::{Poll, Update},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

type Key = (ChatId, i32);

#[derive(Clone)]
struct Refresh {
    request: Option<u64>,
    due: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct State {
    pub review: Option<Review>,
    pub hit_rows: Vec<(u16, u16, u16, usize)>,
    visible: Vec<Key>,
    refresh: BTreeMap<Key, Refresh>,
    next_request: u64,
}

#[derive(Clone)]
pub struct Review {
    pub chat: ChatId,
    pub message: i32,
    pub title: String,
    pub poll: Poll,
    pub selected: usize,
    pub top: usize,
    pub follow: bool,
    pub choices: BTreeSet<Vec<u8>>,
    pub dirty: bool,
    pub ready: bool,
    pub error: Option<String>,
    pub loading: Option<u64>,
    pub sending: Option<u64>,
}

impl Review {
    fn fail(&mut self, error: impl Into<String>) {
        self.error = Some(crate::model::sanitize_terminal_line(&error.into()));
        self.top = 0;
        self.follow = false;
    }

    fn update(&mut self, poll: Poll, loaded: bool) {
        let changed = self.poll.definition != poll.definition;
        self.poll = poll;
        self.selected = self
            .selected
            .min(self.poll.definition.answers.len().saturating_sub(1));
        if changed || loaded || !self.dirty {
            self.choices = self
                .poll
                .results
                .counts
                .iter()
                .filter(|count| count.chosen)
                .map(|count| count.option.clone())
                .collect();
            self.dirty = false;
        }
        if loaded {
            self.ready = !self.poll.stale;
            self.error = None;
        } else if changed {
            self.ready = false;
            self.fail("Poll options changed; refresh before voting");
        }
    }
}

impl App {
    fn next_poll_request(&mut self) -> u64 {
        self.polls.next_request = self.polls.next_request.wrapping_add(1).max(1);
        self.polls.next_request
    }

    pub(super) fn begin_poll(&mut self) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main
            || self.mode != Mode::Navigate
            || self.focus != Focus::Conversation
        {
            return Vec::new();
        }
        if self
            .polls
            .review
            .as_ref()
            .is_some_and(|review| review.sending.is_some())
        {
            self.status_message = Some("Wait for the pending vote to finish".to_owned());
            return Vec::new();
        }
        let Some(message) = self.inspected_message().filter(|message| message.id > 0) else {
            return Vec::new();
        };
        let Some(poll) = message.poll.clone() else {
            self.status_message = Some("Select a poll first".to_owned());
            return Vec::new();
        };
        let choices = poll
            .results
            .counts
            .iter()
            .filter(|count| count.chosen)
            .map(|count| count.option.clone())
            .collect();
        self.polls.review = Some(Review {
            chat: message.chat_id,
            message: message.id,
            title: self
                .active_chat()
                .map_or_else(|| "Poll".to_owned(), |chat| chat.title.clone()),
            poll,
            choices,
            selected: 0,
            top: 0,
            follow: false,
            dirty: false,
            ready: false,
            error: None,
            loading: None,
            sending: None,
        });
        self.mode = Mode::Poll;
        self.status_message = None;
        self.load_poll_review()
    }

    fn load_poll_review(&mut self) -> Vec<TelegramCommand> {
        if self.connection != ConnectionStatus::Online {
            if let Some(review) = &mut self.polls.review {
                review.fail("Cached poll · connect to refresh or vote");
            }
            return Vec::new();
        }
        let request_id = self.next_poll_request();
        let Some(review) = &mut self.polls.review else {
            return Vec::new();
        };
        if review.loading.is_some() || review.sending.is_some() {
            return Vec::new();
        }
        review.ready = false;
        review.loading = Some(request_id);
        review.error = None;
        vec![TelegramCommand::LoadPoll {
            chat_id: review.chat,
            message_id: review.message,
            request_id,
        }]
    }

    pub(super) fn poll_binding(
        &mut self,
        action: &Action,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        if *action == Action::Poll {
            return Some(self.begin_poll());
        }
        if self.mode != Mode::Poll {
            return None;
        }
        let mut commands = Vec::new();
        match action {
            Action::Cancel => {
                self.mode = Mode::Navigate;
                if self
                    .polls
                    .review
                    .as_ref()
                    .is_none_or(|review| review.sending.is_none())
                {
                    self.polls.review = None;
                }
            }
            Action::Refresh => commands = self.load_poll_review(),
            Action::Send => commands = self.submit_vote(),
            Action::Open | Action::TogglePollAnswer => self.toggle_poll_answer(),
            Action::RetractVote => {
                if let Some(review) = &mut self.polls.review
                    && review.ready
                    && review.sending.is_none()
                {
                    if let Some(reason) =
                        review.poll.read_only_reason(chrono::Utc::now().timestamp())
                    {
                        review.fail(reason);
                    } else if review.poll.voted() {
                        review.choices.clear();
                        review.dirty = true;
                        review.error = None;
                    }
                }
            }
            Action::PageUp | Action::PageDown | Action::Home | Action::End => {
                if let Some(review) = &mut self.polls.review {
                    review.follow = false;
                    review.top = match action {
                        Action::PageUp => review.top.saturating_sub(8),
                        Action::PageDown => review.top.saturating_add(8),
                        Action::Home => 0,
                        _ => usize::MAX,
                    };
                }
            }
            Action::Up | Action::Down => {
                if let Some(review) = &mut self.polls.review {
                    review.follow = true;
                    if *action == Action::Up {
                        review.selected = review.selected.saturating_sub(count);
                    } else {
                        review.selected = review
                            .selected
                            .saturating_add(count)
                            .min(review.poll.definition.answers.len().saturating_sub(1));
                    }
                }
            }
            Action::Spoilers => {
                self.toggle_text_details(true);
            }
            Action::Quit | Action::Redraw => return None,
            _ => {}
        }
        Some(commands)
    }

    pub(super) fn toggle_poll_answer(&mut self) {
        if self
            .inspected_message()
            .is_some_and(|message| message.has_spoilers() && !self.spoilers_revealed(message))
        {
            self.toggle_text_details(true);
            return;
        }
        let Some(review) = &mut self.polls.review else {
            return;
        };
        if !review.ready || review.loading.is_some() || review.sending.is_some() {
            return;
        }
        if let Some(reason) = review.poll.read_only_reason(chrono::Utc::now().timestamp()) {
            review.fail(reason);
            return;
        }
        if !self
            .polls
            .hit_rows
            .iter()
            .any(|row| row.3 == review.selected)
        {
            review.follow = true;
            return;
        }
        let Some(answer) = review.poll.definition.answers.get(review.selected) else {
            return;
        };
        if !review.choices.remove(&answer.option) {
            if !review.poll.definition.multiple_choice {
                review.choices.clear();
            }
            review.choices.insert(answer.option.clone());
        }
        review.dirty = true;
        review.error = None;
    }

    fn submit_vote(&mut self) -> Vec<TelegramCommand> {
        if self.connection != ConnectionStatus::Online {
            if let Some(review) = &mut self.polls.review {
                review.fail("Connect to Telegram before voting");
            }
            return Vec::new();
        }
        let request_id = self.next_poll_request();
        let Some(review) = &mut self.polls.review else {
            return Vec::new();
        };
        if !review.ready || review.loading.is_some() || review.sending.is_some() || !review.dirty {
            return Vec::new();
        }
        let options: Vec<_> = review.choices.iter().cloned().collect();
        let revision = review.poll.definition.revision();
        if let Err(error) =
            review
                .poll
                .validate_vote(revision, &options, chrono::Utc::now().timestamp())
        {
            review.fail(error);
            return Vec::new();
        }
        review.sending = Some(request_id);
        review.error = None;
        vec![TelegramCommand::VotePoll {
            chat_id: review.chat,
            message_id: review.message,
            poll_id: review.poll.definition.id,
            revision,
            options,
            request_id,
        }]
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn observe_polls(&mut self, event: &NetworkEvent) -> Option<Vec<TelegramCommand>> {
        match event {
            NetworkEvent::PollChanged(update) => {
                self.apply_poll_update(update);
                return Some(Vec::new());
            }
            NetworkEvent::PollLoading { .. } => return Some(Vec::new()),
            NetworkEvent::PollLoaded {
                chat_id,
                message_id,
                request_id,
                result,
            } => {
                if let Ok(poll) = result {
                    self.replace_poll(poll);
                }
                self.finish_poll_refresh(
                    (*chat_id, *message_id),
                    *request_id,
                    result.as_ref().err(),
                );
                if let Some(review) = &mut self.polls.review
                    && review.loading == Some(*request_id)
                    && (review.chat, review.message) == (*chat_id, *message_id)
                {
                    review.loading = None;
                    match result {
                        Ok(poll) if poll.definition.id == review.poll.definition.id => {
                            review.update(poll.clone(), true);
                        }
                        Ok(_) => {
                            review.ready = false;
                            review.fail("The original poll is no longer available");
                        }
                        Err(error) => {
                            review.fail(error);
                        }
                    }
                }
                return Some(Vec::new());
            }
            NetworkEvent::PollFinished {
                chat_id,
                message_id,
                request_id,
                voting,
                error,
            } => {
                if !voting {
                    self.finish_poll_refresh((*chat_id, *message_id), *request_id, error.as_ref());
                } else if let Some(review) = &mut self.polls.review
                    && review.sending == Some(*request_id)
                    && (review.chat, review.message) == (*chat_id, *message_id)
                {
                    review.sending = None;
                    if let Some(error) = error {
                        review.ready = false;
                        review.fail(format!("{error} · refresh before retrying"));
                        if self.mode != Mode::Poll {
                            self.status_message.clone_from(&review.error);
                        }
                    } else {
                        review.dirty = false;
                        self.status_message = Some("Vote saved".to_owned());
                        if self.mode == Mode::Poll {
                            return Some(self.load_poll_review());
                        }
                        self.polls.review = None;
                    }
                }
                return Some(Vec::new());
            }
            NetworkEvent::MessageUpdated(message) => {
                if let Some(review) = &mut self.polls.review
                    && (review.chat, review.message) == (message.chat_id, message.id)
                {
                    if let Some(poll) = &message.poll {
                        review.update(poll.clone(), false);
                    } else {
                        review.ready = false;
                        review.fail("The poll was removed");
                    }
                }
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            } => {
                if let Some(review) = &mut self.polls.review
                    && channel_id.map_or(review.chat > -1_000_000_000_000, |id| id == review.chat)
                    && message_ids.contains(&review.message)
                {
                    review.ready = false;
                    review.loading = None;
                    review.fail("The poll message was deleted");
                }
            }
            NetworkEvent::CacheInvalidated { chat_id } => {
                if let Some(review) = &mut self.polls.review
                    && chat_id.is_none_or(|id| id == review.chat)
                {
                    review.ready = false;
                    review.loading = None;
                    review.fail("History changed; reopen this poll");
                }
            }
            _ => {}
        }
        None
    }

    fn apply_poll_update(&mut self, update: &Update) {
        self.visit_messages(|message| {
            if let Some(poll) = &mut message.poll {
                poll.apply(update);
            }
        });
        if let Some(review) = &mut self.polls.review
            && review.poll.definition.id == update.id
        {
            let mut poll = review.poll.clone();
            poll.apply(update);
            review.update(poll, false);
        }
    }

    fn replace_poll(&mut self, fresh: &Poll) {
        self.visit_messages(|message| {
            if message
                .poll
                .as_ref()
                .is_some_and(|poll| poll.definition.id == fresh.definition.id)
            {
                message.poll = Some(fresh.clone());
            }
        });
    }

    pub fn set_visible_polls(&mut self, ids: Vec<Key>) {
        self.polls.visible = ids;
    }

    fn polls_visible(&self) -> bool {
        self.screen == Screen::Main
            && self.terminal_focused
            && self.connection == ConnectionStatus::Online
            && matches!(self.mode, Mode::Navigate | Mode::Compose | Mode::Poll)
    }

    pub fn request_visible_polls(&mut self) -> Vec<TelegramCommand> {
        if !self.polls_visible() {
            self.polls.refresh.retain(|_, slot| slot.request.is_some());
            return Vec::new();
        }
        let mut visible = if self.mode == Mode::Poll {
            self.polls
                .review
                .as_ref()
                .map(|review| vec![(review.chat, review.message)])
                .unwrap_or_default()
        } else {
            self.polls.visible.clone()
        };
        visible.truncate(32);
        self.polls
            .refresh
            .retain(|key, slot| visible.contains(key) || slot.request.is_some());
        let now = Instant::now();
        let mut in_flight = self
            .polls
            .refresh
            .values()
            .filter(|slot| slot.request.is_some())
            .count();
        let mut commands = Vec::new();
        for key in visible {
            if self.polls.review.as_ref().is_some_and(|review| {
                (review.chat, review.message) == key
                    && (review.loading.is_some() || review.sending.is_some())
            }) {
                continue;
            }
            let Some(poll) = self
                .messages
                .get(&key.0)
                .and_then(|messages| messages.iter().find(|message| message.id == key.1))
                .and_then(|message| message.poll.as_ref())
            else {
                continue;
            };
            let hash = if poll.stale { 0 } else { poll.definition.hash };
            let stale = poll.stale;
            let slot = self.polls.refresh.entry(key).or_insert(Refresh {
                request: None,
                due: Some(now),
            });
            if in_flight >= 2 || slot.request.is_some() || slot.due.is_none_or(|due| due > now) {
                continue;
            }
            self.polls.next_request = self.polls.next_request.wrapping_add(1).max(1);
            let request_id = self.polls.next_request;
            slot.request = Some(request_id);
            commands.push(if stale {
                TelegramCommand::LoadPoll {
                    chat_id: key.0,
                    message_id: key.1,
                    request_id,
                }
            } else {
                TelegramCommand::RefreshPoll {
                    chat_id: key.0,
                    message_id: key.1,
                    hash,
                    request_id,
                }
            });
            in_flight += 1;
        }
        commands
    }

    fn finish_poll_refresh(&mut self, key: Key, request: u64, error: Option<&String>) {
        let poll = self
            .messages
            .get(&key.0)
            .and_then(|messages| messages.iter().find(|message| message.id == key.1))
            .and_then(|message| message.poll.as_ref());
        let Some(slot) = self
            .polls
            .refresh
            .get_mut(&key)
            .filter(|slot| slot.request == Some(request))
        else {
            return;
        };
        slot.request = None;
        let now = chrono::Utc::now().timestamp();
        slot.due = if error.is_none() && poll.is_some_and(|poll| poll.closed(now) && !poll.stale) {
            None
        } else {
            Some(
                Instant::now()
                    + Duration::from_secs(
                        poll.and_then(|poll| poll.definition.close_date)
                            .filter(|date| *date > now)
                            .map_or(30, |date| u64::try_from(date - now).unwrap_or(30).min(30)),
                    ),
            )
        };
    }

    #[must_use]
    pub fn next_poll_deadline(&self) -> Option<Instant> {
        if !self.polls_visible()
            || self
                .polls
                .refresh
                .values()
                .filter(|slot| slot.request.is_some())
                .count()
                >= 2
        {
            return None;
        }
        self.polls
            .refresh
            .iter()
            .filter(|(key, slot)| {
                slot.request.is_none()
                    && !self.polls.review.as_ref().is_some_and(|review| {
                        (review.chat, review.message) == **key
                            && (review.loading.is_some() || review.sending.is_some())
                    })
            })
            .filter_map(|(_, slot)| slot.due)
            .min()
    }
}
