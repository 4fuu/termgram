use super::{App, Focus, KeyAction, Mode, TelegramCommand};
use crate::{
    actions::Action,
    drafts::Key,
    staging::{MAX_BYTES, MAX_FILES, Prepared, Request},
};

#[derive(Clone, Default)]
pub struct State {
    pending: Option<(Key, u64, Option<String>)>,
    next_request: u64,
    pub selected: usize,
    pub preview: bool,
    pub hit_regions: Vec<(u16, u16, u16, usize)>,
    previous_mode: Mode,
}

impl App {
    pub(super) fn prepare_attachments(
        &mut self,
        paths: String,
        fallback_text: bool,
    ) -> Vec<TelegramCommand> {
        let Some(chat) = self.active_chat_id else {
            self.status_message = Some("Open a chat before attaching files".to_owned());
            return Vec::new();
        };
        if self.attachment_draft.pending.is_some() {
            self.status_message = Some("An attachment is still being prepared".to_owned());
            return Vec::new();
        }
        let key = self.draft_key(chat);
        let attachments = self.draft_attachments();
        let available_files = MAX_FILES.saturating_sub(attachments.len());
        let available_bytes =
            MAX_BYTES.saturating_sub(attachments.iter().map(|file| file.size).sum());
        self.attachment_draft.next_request =
            self.attachment_draft.next_request.wrapping_add(1).max(1);
        let id = self.attachment_draft.next_request;
        self.attachment_draft.pending = Some((key, id, fallback_text.then(|| paths.clone())));
        self.mode = Mode::Compose;
        self.focus = Focus::Conversation;
        self.status_message = Some("Preparing attachments…".to_owned());
        vec![TelegramCommand::PrepareAttachments(Request {
            key,
            id,
            paths,
            available_files,
            available_bytes,
        })]
    }

    #[must_use]
    pub fn draft_attachments(&self) -> &[crate::staging::Attachment] {
        self.active_chat_id
            .and_then(|chat| self.draft_data(chat))
            .map_or(&[], |draft| draft.attachments.as_slice())
    }

    #[must_use]
    pub fn preparing_attachments(&self) -> bool {
        self.active_chat_id.is_some_and(|chat| {
            self.attachment_draft
                .pending
                .as_ref()
                .is_some_and(|(key, _, _)| *key == self.draft_key(chat))
        })
    }

    pub(super) fn attachments_prepared(
        &mut self,
        key: Key,
        request_id: u64,
        result: Result<Prepared, String>,
    ) {
        if !self
            .attachment_draft
            .pending
            .as_ref()
            .is_some_and(|(current, id, _)| *current == key && *id == request_id)
        {
            return;
        }
        let fallback = self
            .attachment_draft
            .pending
            .take()
            .and_then(|(_, _, text)| text);
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(error) => Prepared {
                attachments: Vec::new(),
                errors: vec![error],
            },
        };
        if prepared.attachments.is_empty()
            && let Some(text) = fallback
        {
            self.draft_at_mut(key).input.insert_str(&text);
            self.status_message = None;
            return;
        }
        let count = prepared.attachments.len();
        self.draft_at_mut(key)
            .attachments
            .extend(prepared.attachments);
        self.attachment_draft.selected = 0;
        let title = self
            .chats
            .iter()
            .find(|chat| chat.id == key.chat)
            .map_or_else(|| key.chat.to_string(), |chat| chat.title.clone());
        self.status_message = Some(format!(
            "{count} attachment(s) added to {title}{}",
            if prepared.errors.is_empty() {
                String::new()
            } else {
                format!(" · {}", prepared.errors.join(" · "))
            }
        ));
    }

    pub(super) fn open_attachments(&mut self) -> Vec<TelegramCommand> {
        if self.active_chat_id.is_none() {
            return Vec::new();
        }
        self.attachment_draft.previous_mode = self.mode;
        self.attachment_draft.selected = 0;
        self.attachment_draft.preview = false;
        self.mode = Mode::Attachments;
        self.status_message = None;
        Vec::new()
    }

    pub(super) fn attachment_binding(&mut self, action: &Action) -> Option<Vec<TelegramCommand>> {
        if self.mode != Mode::Attachments {
            return None;
        }
        let count = self.draft_attachments().len();
        match action {
            Action::Cancel if self.attachment_draft.preview => {
                self.attachment_draft.preview = false;
            }
            Action::Cancel => self.mode = self.attachment_draft.previous_mode,
            Action::Compose => {
                self.mode = Mode::Compose;
                self.focus = Focus::Conversation;
            }
            Action::Up => {
                self.attachment_draft.selected = self.attachment_draft.selected.saturating_sub(1);
            }
            Action::Down => {
                self.attachment_draft.selected = self
                    .attachment_draft
                    .selected
                    .saturating_add(1)
                    .min(count.saturating_sub(1));
            }
            Action::RemoveAttachment => {
                if self.preparing_attachments() {
                    self.attachment_draft.pending = None;
                    self.status_message = Some("Attachment preparation cancelled".to_owned());
                } else if let Some(chat) = self.active_chat_id {
                    let index = self.attachment_draft.selected;
                    let attachments = &mut self.draft_data_mut(chat).attachments;
                    if index < attachments.len() {
                        attachments.remove(index);
                    }
                    self.attachment_draft.selected = index.min(attachments.len().saturating_sub(1));
                }
                self.attachment_draft.preview = false;
            }
            Action::AttachmentFormat => {
                if let Some(chat) = self.active_chat_id {
                    let index = self.attachment_draft.selected;
                    if let Some(attachment) = self.draft_data_mut(chat).attachments.get_mut(index)
                        && attachment.photo_supported
                    {
                        attachment.as_photo = !attachment.as_photo;
                    }
                }
            }
            Action::Open | Action::Preview => {
                self.attachment_draft.preview = self
                    .draft_attachments()
                    .get(self.attachment_draft.selected)
                    .is_some_and(|attachment| attachment.photo_supported);
            }
            Action::Reveal => {
                if let Some(file) = self
                    .draft_attachments()
                    .get(self.attachment_draft.selected)
                    .cloned()
                {
                    self.reveal_download(&file.path);
                }
            }
            Action::Attach => {
                self.mode = Mode::Navigate;
                self.begin_command();
                self.commands.input.set_value("attach ");
            }
            Action::Quit | Action::NextAccount | Action::AddAccount => return None,
            Action::Redraw => {
                self.handle_action(KeyAction::Redraw);
            }
            _ => {}
        }
        self.force_redraw = true;
        Some(Vec::new())
    }
}
