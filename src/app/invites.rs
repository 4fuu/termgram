use super::{App, Mode, Screen, TelegramCommand};
use crate::{
    actions::Action,
    event::{ConnectionStatus, NetworkEvent},
    invites::{Outcome, Preview},
    model::{sanitize_terminal_line, sanitize_terminal_text},
};

#[derive(Clone, Default)]
pub struct State {
    pub preview: Option<Preview>,
    pub error: Option<String>,
    pub selected: usize,
    pub hit_regions: Vec<(u16, u16, u16, usize)>,
    hash: String,
    next_request: u64,
    pending: Option<(u64, bool)>,
    settled: bool,
}

impl App {
    pub(super) fn begin_invite(&mut self, hash: String) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main || self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram to preview this invite".to_owned());
            return Vec::new();
        }
        if self.invites.pending.is_some_and(|(_, joining)| joining) {
            self.status_message = Some("A join request is still running".to_owned());
            return Vec::new();
        }
        let request_id = self.invites.next_request + 1;
        self.invites = State {
            hash: hash.clone(),
            next_request: request_id,
            pending: Some((request_id, false)),
            ..State::default()
        };
        self.pending_telegram_link = None;
        self.mode = Mode::Invite;
        self.status_message = None;
        vec![TelegramCommand::PreviewInvite { hash, request_id }]
    }

    #[must_use]
    pub fn invite_action(&self) -> Option<&'static str> {
        if self.invites.settled {
            return None;
        }
        let preview = self
            .invites
            .preview
            .as_ref()
            .filter(|p| p.blocked.is_none())?;
        Some(if preview.joined.is_some() {
            "Open chat"
        } else if preview.request_needed {
            "Request to join"
        } else {
            "Join group"
        })
    }

    #[must_use]
    pub fn invite_pending(&self) -> bool {
        self.invites.pending.is_some()
    }

    pub(super) fn invite_binding(&mut self, action: &Action) -> Option<Vec<TelegramCommand>> {
        if self.mode != Mode::Invite {
            return None;
        }
        match action {
            Action::Cancel => self.close_invite(),
            Action::Up => self.invites.selected = 0,
            Action::Down if self.invite_action().is_some() => self.invites.selected = 1,
            Action::Open if self.invites.selected == 0 => self.close_invite(),
            Action::Open if !self.invite_pending() => {
                if self.invite_action().is_none() {
                    return Some(Vec::new());
                }
                let preview = self.invites.preview.as_ref().expect("available action");
                if let Some(chat) = preview.joined.clone() {
                    self.close_invite();
                    return Some(self.open_resolved_link(chat, None));
                }
                if self.connection != ConnectionStatus::Online {
                    self.invites.error = Some("Connect to Telegram before joining".to_owned());
                    return Some(Vec::new());
                }
                self.invites.next_request += 1;
                let request_id = self.invites.next_request;
                self.invites.pending = Some((request_id, true));
                self.invites.error = None;
                return Some(vec![TelegramCommand::JoinInvite {
                    hash: self.invites.hash.clone(),
                    title: preview.title.clone(),
                    request_needed: preview.request_needed,
                    request_id,
                }]);
            }
            Action::Quit | Action::NextAccount | Action::AddAccount | Action::Redraw => {
                return None;
            }
            _ => {}
        }
        Some(Vec::new())
    }

    fn close_invite(&mut self) {
        self.mode = Mode::Navigate;
        self.force_redraw = true;
        if self.invites.pending.is_some_and(|(_, joining)| joining) {
            self.status_message = Some("Join request continues in the background".to_owned());
        } else {
            self.invites = State {
                next_request: self.invites.next_request,
                ..State::default()
            };
        }
    }

    pub(super) fn observe_invite(&mut self, event: NetworkEvent) -> Vec<TelegramCommand> {
        match event {
            NetworkEvent::InviteReady { request_id, result }
                if self.invites.pending == Some((request_id, false)) =>
            {
                self.invites.pending = None;
                match result {
                    Ok(mut preview) => {
                        preview.title = sanitize_terminal_line(&preview.title);
                        preview.about = sanitize_terminal_text(&preview.about);
                        if let Some(chat) = &mut preview.joined {
                            chat.title = sanitize_terminal_line(&chat.title);
                        }
                        self.invites.preview = Some(preview);
                    }
                    Err(error) => self.invites.error = Some(sanitize_terminal_line(&error)),
                }
            }
            NetworkEvent::InviteJoined { request_id, result }
                if self.invites.pending == Some((request_id, true)) =>
            {
                self.invites.pending = None;
                self.invites.selected = 0;
                self.invites.settled = true;
                let notice = match result {
                    Ok(Outcome::Joined(chat)) => {
                        if self.mode == Mode::Invite {
                            self.close_invite();
                            return self.open_resolved_link(chat, None);
                        }
                        "Joined group; open it from your chats".to_owned()
                    }
                    Ok(Outcome::Requested) => {
                        "Join request submitted; waiting for an administrator".to_owned()
                    }
                    Ok(Outcome::Verification) => {
                        "Complete the required verification in the official Telegram client"
                            .to_owned()
                    }
                    Err(error) => sanitize_terminal_line(&error),
                };
                self.invites.error = Some(notice.clone());
                self.status_message = Some(notice);
            }
            _ => {}
        }
        Vec::new()
    }
}
