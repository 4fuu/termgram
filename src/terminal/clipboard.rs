//! OSC 5522 application adapter. Yazi parses and formats the protocol; this
//! broker only selects a MIME type and retains the user's original destination.
use crate::{
    app::{AppState, Focus, Mode, Screen},
    clipboard::Payload,
    event::{AppEvent, TelegramCommand},
    staging::{Input, Request},
};
use std::{collections::VecDeque, io::Write, time::Duration};
use tokio::time::Instant;
use yazi_term::event::{ClipboardData, ClipboardEvent};
use yazi_tty::{
    TTY,
    sequence::{ReadClipboard, SetClipboard},
};

const DEADLINE: Duration = Duration::from_secs(10);
const MAX_TEXT: usize = 1024 * 1024;

#[derive(Clone, Eq, PartialEq)]
struct TextTarget {
    account: u8,
    user: Option<i64>,
    chat: Option<i64>,
    edit_message: Option<i32>,
    screen: Screen,
    mode: Mode,
    focus: Focus,
}

impl TextTarget {
    fn capture(app: &AppState) -> Self {
        Self {
            account: app.active_account(),
            user: app.account_user_id,
            chat: app.active_chat_id,
            edit_message: app.message_edit().map(|edit| edit.source.message_id),
            screen: app.screen.clone(),
            mode: app.mode,
            focus: app.focus,
        }
    }
}

enum Destination {
    Draft(Request),
    Text(TextTarget),
}

struct Pending {
    id: String,
    mime: Option<&'static str>,
    destination: Destination,
    deadline: Instant,
    primary: bool,
    password: String,
}

pub enum Activity {
    PasteTimeout,
    Copied(Box<crate::clipboard::copy::Completion>),
}

#[derive(Default)]
pub struct Broker {
    supported: bool,
    next_id: u64,
    pending: Option<Pending>,
    outbound: VecDeque<String>,
    writer: Option<crate::clipboard::copy::Writer>,
}

impl Broker {
    pub fn configure(&mut self, supported: bool, app: &mut AppState) {
        self.supported = supported && app.keymap.attachments.terminal_clipboard;
        if !self.supported && self.pending.is_some() {
            self.fail(app, "Terminal clipboard is no longer available".to_owned());
        }
    }

    /// Intercept native clipboard intents only after positive capability detection.
    pub fn route(
        &mut self,
        app: &mut AppState,
        outgoing: Vec<TelegramCommand>,
    ) -> Vec<TelegramCommand> {
        let mut remaining = Vec::new();
        for command in outgoing {
            match command {
                TelegramCommand::CopyText(text) => self.copy(app, text),
                TelegramCommand::PrepareAttachments(request)
                    if self.supported && request.input == Input::Clipboard =>
                {
                    self.start(app, Destination::Draft(request), false, String::new(), None);
                }
                other => remaining.push(other),
            }
        }
        remaining
    }

    /// Flush alongside normal rendering on the main thread, through the same TTY.
    pub fn flush(&mut self, app: &mut AppState) {
        if self.outbound.is_empty() {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            let mut writer = TTY.writer();
            while let Some(sequence) = self.outbound.pop_front() {
                writer.write_all(sequence.as_bytes())?;
            }
            writer.flush()
        })();
        if let Err(error) = result {
            let error = format!("Terminal clipboard request failed: {error}");
            app.status_message = Some(error.clone());
            self.fail(app, error);
        }
    }

    /// Both clipboard directions remain events of the existing main loop.
    pub async fn next_activity(&mut self) -> Activity {
        let deadline = self.pending.as_ref().map(|pending| pending.deadline);
        let paste = async move {
            if let Some(deadline) = deadline {
                tokio::time::sleep_until(deadline).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        let copy = async {
            if let Some(writer) = &mut self.writer {
                writer.receive().await
            } else {
                std::future::pending().await
            }
        };
        tokio::select! {
            () = paste => Activity::PasteTimeout,
            result = copy => Activity::Copied(Box::new(result)),
        }
    }

    fn copy(&mut self, app: &mut AppState, text: String) {
        if self.writer.is_none() {
            match crate::clipboard::copy::Writer::start() {
                Ok(writer) => self.writer = Some(writer),
                Err(error) => {
                    app.status_message = Some(format!("Clipboard writer could not start: {error}"));
                    return;
                }
            }
        }
        let result =
            self.writer
                .as_mut()
                .expect("initialized writer")
                .submit(crate::clipboard::copy::Job {
                    account: (app.active_account(), app.account_user_id),
                    text,
                    remote: remote(),
                    multiplexed: std::env::var_os("TMUX").is_some()
                        || std::env::var_os("TMUX_PANE").is_some(),
                });
        app.status_message = Some(result.map_or_else(|error| error, |()| "Copying…".to_owned()));
    }

    pub fn activity(&mut self, app: &mut AppState, activity: Activity) {
        match activity {
            Activity::PasteTimeout => self.timeout(app),
            Activity::Copied(result) => {
                let status = if let Some(text) = &result.terminal {
                    self.outbound
                        .push_back(SetClipboard(text.as_bytes()).to_string());
                    if result.native.is_ok() {
                        "Copied · terminal forwarding requested"
                    } else {
                        "Clipboard request sent to terminal (OSC 52)"
                    }
                    .to_owned()
                } else {
                    result
                        .native
                        .map_or_else(|error| error, |()| "Copied".to_owned())
                };
                if result.account == (app.active_account(), app.account_user_id)
                    || result.account.0 == 0
                {
                    app.status_message = Some(status);
                }
            }
        }
    }

    pub fn timeout(&mut self, app: &mut AppState) {
        self.fail(
            app,
            "Terminal clipboard timed out; try again or use :attach".to_owned(),
        );
    }

    pub fn configuration_changed(&mut self, app: &mut AppState) {
        self.fail(
            app,
            "Terminal paste cancelled by configuration reload; paste again".to_owned(),
        );
        self.outbound.clear();
    }

    pub fn cancel(&mut self) {
        self.pending = None;
        self.outbound.clear();
        // Keep the sequence counter across account switches. Late replies can
        // never match a new operation, even when draft request IDs restart.
    }

    pub fn event(&mut self, app: &mut AppState, event: ClipboardEvent) -> Vec<TelegramCommand> {
        match event {
            ClipboardEvent::Read {
                id,
                primary,
                pw,
                data,
            } if id.is_empty() && data.get(".").is_some() => {
                if !self.supported {
                    return Vec::new();
                }
                if self.pending.is_some() {
                    app.status_message = Some("A terminal paste is still in progress".to_owned());
                    return Vec::new();
                }
                let media = app.screen == Screen::Main
                    && ((matches!(app.mode, Mode::Navigate | Mode::Compose)
                        && app.focus == Focus::Conversation)
                        || app.mode == Mode::Attachments);
                let destination = if media {
                    let Some(request) = app.reserve_terminal_paste() else {
                        return Vec::new();
                    };
                    Destination::Draft(request)
                } else if matches!(app.screen, Screen::Auth(_))
                    || (app.screen == Screen::Main
                        && matches!(
                            app.mode,
                            Mode::Command | Mode::Search | Mode::Filter | Mode::Edit
                        ))
                {
                    Destination::Text(TextTarget::capture(app))
                } else {
                    app.status_message =
                        Some("Select an input or open a conversation to paste".to_owned());
                    return Vec::new();
                };
                self.start(app, destination, primary, pw, data.get("."));
            }
            ClipboardEvent::Read { id, data, .. }
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == id) =>
            {
                if let Some(pending) = self.pending.take() {
                    return self.received(app, pending, data);
                }
            }
            ClipboardEvent::ReadError { id, code }
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == id) =>
            {
                self.fail(
                    app,
                    format!(
                        "Terminal clipboard: {}",
                        crate::model::sanitize_terminal_line(
                            &code.chars().take(80).collect::<String>()
                        )
                    ),
                );
            }
            _ => {}
        }
        Vec::new()
    }

    fn received(
        &mut self,
        app: &mut AppState,
        mut pending: Pending,
        data: ClipboardData,
    ) -> Vec<TelegramCommand> {
        let Some(mime) = pending.mime else {
            let result = data
                .get(".")
                .ok_or("Terminal did not provide its clipboard MIME list")
                .and_then(|types| {
                    select_mime(types, matches!(pending.destination, Destination::Draft(_)))
                });
            match result {
                Ok(mime) => {
                    pending.mime = Some(mime);
                    self.enqueue(pending);
                }
                Err(error) => {
                    Self::fail_destination(app, pending.destination, error.to_owned());
                }
            }
            return Vec::new();
        };
        let Some(bytes) = data.into_inner().remove(mime) else {
            Self::fail_destination(
                app,
                pending.destination,
                "Clipboard changed or its requested type is unavailable".to_owned(),
            );
            return Vec::new();
        };
        match pending.destination {
            Destination::Draft(mut request) => {
                request.input = Input::Terminal(Payload {
                    mime: mime.to_owned(),
                    bytes: bytes.into(),
                    remote: remote(),
                });
                return vec![TelegramCommand::PrepareAttachments(request)];
            }
            Destination::Text(target) => {
                if target != TextTarget::capture(app) {
                    app.status_message =
                        Some("Paste cancelled because the input destination changed".to_owned());
                } else if bytes.len() > MAX_TEXT {
                    app.status_message = Some("Clipboard text exceeds 1 MiB".to_owned());
                } else if let Ok(text) = String::from_utf8(bytes) {
                    return app.update(AppEvent::Paste(text));
                } else {
                    app.status_message = Some("Clipboard text is not valid UTF-8".to_owned());
                }
            }
        }
        Vec::new()
    }

    fn start(
        &mut self,
        app: &mut AppState,
        destination: Destination,
        primary: bool,
        password: String,
        types: Option<&[u8]>,
    ) {
        if self.pending.is_some() {
            Self::fail_destination(
                app,
                destination,
                "A terminal paste is still in progress".to_owned(),
            );
            return;
        }
        if password.len() > 4096 {
            Self::fail_destination(
                app,
                destination,
                "Terminal clipboard token exceeds its limit".to_owned(),
            );
            return;
        }
        let mime = match types
            .map(|types| select_mime(types, matches!(destination, Destination::Draft(_))))
            .transpose()
        {
            Ok(mime) => mime,
            Err(error) => {
                Self::fail_destination(app, destination, error.to_owned());
                return;
            }
        };
        self.enqueue(Pending {
            id: String::new(),
            mime,
            destination,
            deadline: Instant::now() + DEADLINE,
            primary,
            password,
        });
    }

    fn enqueue(&mut self, mut pending: Pending) {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        pending.id = format!("termgram-{}-{}", std::process::id(), self.next_id);
        pending.deadline = Instant::now() + DEADLINE;
        self.outbound.push_back(
            ReadClipboard::new(
                [pending.mime.unwrap_or(".")],
                &pending.password,
                if pending.password.is_empty() {
                    "Termgram"
                } else {
                    "Paste event"
                },
                pending.primary,
            )
            .with_id(&pending.id)
            .to_string(),
        );
        self.pending = Some(pending);
    }

    fn fail(&mut self, app: &mut AppState, error: String) {
        self.outbound.clear();
        if let Some(pending) = self.pending.take() {
            Self::fail_destination(app, pending.destination, error);
        }
    }

    fn fail_destination(app: &mut AppState, destination: Destination, error: String) {
        match destination {
            Destination::Draft(request) => {
                app.handle_network(request.failure(error));
            }
            Destination::Text(_) => app.status_message = Some(error),
        }
    }
}

fn select_mime(types: &[u8], media: bool) -> Result<&'static str, &'static str> {
    if types.len() > 64 * 1024 {
        return Err("Clipboard MIME list exceeds 64 KiB");
    }
    let types = std::str::from_utf8(types).map_err(|_| "Clipboard MIME list is not UTF-8")?;
    let allowed = if media {
        &[
            "text/uri-list",
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/gif",
            "text/plain",
            "text/plain;charset=utf-8",
        ][..]
    } else {
        &["text/plain", "text/plain;charset=utf-8"][..]
    };
    allowed
        .iter()
        .copied()
        .find(|mime| types.split_whitespace().any(|candidate| candidate == *mime))
        .ok_or("Clipboard contains no supported image, file list or plain text")
}

fn remote() -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(id: &str, mime: &str, bytes: &[u8]) -> ClipboardEvent {
        ClipboardEvent::Read {
            id: id.to_owned(),
            primary: false,
            pw: String::new(),
            data: [(mime.to_owned(), bytes.to_vec())].into_iter().collect(),
        }
    }

    #[test]
    fn mime_paste_keeps_original_draft_and_rejects_late_phase_replies() {
        let mut app = AppState::new();
        app.screen = Screen::Main;
        app.account_user_id = Some(100);
        app.active_chat_id = Some(7);
        app.focus = Focus::Conversation;
        let request = app.reserve_terminal_paste().unwrap();
        let original_key = request.key;
        let mut broker = Broker::default();
        broker.configure(true, &mut app);
        assert!(
            broker
                .route(&mut app, vec![TelegramCommand::PrepareAttachments(request)])
                .is_empty()
        );
        let manifest_id = broker.pending.as_ref().unwrap().id.clone();
        assert!(
            broker
                .event(&mut app, read(&manifest_id, ".", b"image/png text/plain"))
                .is_empty()
        );
        let payload_id = broker.pending.as_ref().unwrap().id.clone();
        assert_ne!(manifest_id, payload_id);
        app.active_chat_id = Some(8);
        assert!(
            broker
                .event(&mut app, read(&manifest_id, ".", b"text/plain"))
                .is_empty()
        );
        let commands = broker.event(&mut app, read(&payload_id, "image/png", b"encoded image"));
        let [TelegramCommand::PrepareAttachments(result)] = commands.as_slice() else {
            panic!("prepare only")
        };
        assert_eq!(result.key, original_key);
        assert!(
            matches!(&result.input, Input::Terminal(payload) if payload.mime == "image/png" && &*payload.bytes == b"encoded image")
        );
        assert!(broker.pending.is_none());
        app = AppState::new();
        app.screen = Screen::Main;
        app.account_user_id = Some(100);
        app.active_chat_id = Some(3);
        app.focus = Focus::Conversation;
        app.mode = Mode::Compose;
        broker.event(&mut app, read("", ".", b"image/png"));
        let obsolete = broker.pending.as_ref().unwrap().id.clone();
        broker.configuration_changed(&mut app);
        assert!(!app.preparing_attachments());
        assert!(
            broker
                .event(&mut app, read(&obsolete, "image/png", b"old"))
                .is_empty()
        );
    }

    #[test]
    fn changed_text_context_and_cancelled_request_cannot_receive_old_paste() {
        let mut app = AppState::new();
        app.screen = Screen::Main;
        app.mode = Mode::Search;
        let mut broker = Broker::default();
        broker.configure(true, &mut app);
        broker.event(&mut app, read("", ".", b"text/plain image/png"));
        let old_id = broker.pending.as_ref().unwrap().id.clone();
        app.mode = Mode::Filter;
        assert!(
            broker
                .event(&mut app, read(&old_id, "text/plain", b"private text"))
                .is_empty()
        );
        assert!(
            app.status_message
                .as_deref()
                .unwrap()
                .contains("destination changed")
        );
        assert!(app.filter.value().is_empty());
        broker.event(&mut app, read("", ".", b"text/plain"));
        let cancelled = broker.pending.as_ref().unwrap().id.clone();
        broker.cancel();
        broker.event(&mut app, read("", ".", b"text/plain"));
        let current = broker.pending.as_ref().unwrap().id.clone();
        assert_ne!(cancelled, current);
        assert!(
            broker
                .event(&mut app, read(&cancelled, "text/plain", b"old text"))
                .is_empty()
        );
        assert_eq!(broker.pending.as_ref().unwrap().id, current);
        broker.timeout(&mut app);
        assert!(
            broker
                .event(&mut app, read(&current, "text/plain", b"late text"))
                .is_empty()
        );
        assert!(app.filter.value().is_empty());
    }
    #[tokio::test]
    async fn remote_copy_is_bounded_and_uses_the_single_tty_output_queue() {
        use crate::clipboard::copy::{Job, MAX_TEXT, Writer};
        let mut app = AppState::new();
        app.account_user_id = Some(100);
        let account = (app.active_account(), app.account_user_id);
        let job = |text: String| Job {
            account,
            text,
            remote: true,
            multiplexed: false,
        };
        // remote=true deliberately avoids reading or modifying the real clipboard.
        let mut writer = Writer::start().unwrap();
        assert!(writer.submit(job(String::new())).is_err());
        assert!(writer.submit(job("x".repeat(MAX_TEXT + 1))).is_err());
        writer.submit(job("界🙂\ntext".to_owned())).unwrap();
        assert!(writer.submit(job("duplicate".to_owned())).is_err());
        let mut broker = Broker {
            writer: Some(writer),
            ..Broker::default()
        };
        let event = tokio::time::timeout(Duration::from_secs(2), broker.next_activity())
            .await
            .unwrap();
        broker.activity(&mut app, event);
        assert_eq!(
            broker.outbound.pop_front().unwrap(),
            SetClipboard("界🙂\ntext".as_bytes()).to_string()
        );
        assert!(
            app.status_message
                .as_ref()
                .unwrap()
                .contains("request sent")
        );
        broker.cancel();
        assert!(broker.outbound.is_empty());
        assert!(
            broker.writer.is_some(),
            "account changes retain the clipboard owner"
        );
    }
}
