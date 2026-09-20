use super::{App, Path, PathBuf, TelegramCommand, reveal_path, sanitize_terminal_line};
use crate::model::ChatId;

impl App {
    pub(super) fn preview_selected_media(&mut self) -> Vec<TelegramCommand> {
        let message = self.selected_message.and_then(|id| {
            self.active_messages()
                .iter()
                .find(|message| message.id == id)
                .cloned()
        });
        let Some(message) = message.filter(|message| {
            message.id > 0
                && message
                    .attachment
                    .as_ref()
                    .is_some_and(crate::model::Attachment::supports_preview)
        }) else {
            self.status_message = Some("Select an image or sticker to preview".to_owned());
            return Vec::new();
        };
        self.activate_attachment(message.chat_id, message.id, &message)
    }

    pub(super) fn queue_download(
        &mut self,
        chat_id: ChatId,
        message_id: i32,
        media_id: Option<i64>,
    ) -> Vec<TelegramCommand> {
        if self
            .downloading_attachments
            .contains_key(&(chat_id, message_id))
        {
            self.status_message = Some("Attachment is downloading…".to_owned());
            return Vec::new();
        }
        let request_id = self.next_download_request_id;
        self.next_download_request_id = request_id.wrapping_add(1).max(1);
        self.downloading_attachments
            .insert((chat_id, message_id), request_id);
        self.status_message = Some("Downloading attachment…".to_owned());
        vec![TelegramCommand::DownloadAttachment {
            chat_id,
            message_id,
            request_id,
            media_id,
        }]
    }

    pub(super) fn reveal_selected_attachment(&mut self) -> Vec<TelegramCommand> {
        let message = self
            .active_messages()
            .iter()
            .find(|message| Some(message.id) == self.selected_message)
            .cloned();
        let Some(message) =
            message.filter(|message| message.id > 0 && message.attachment.is_some())
        else {
            self.status_message =
                Some("Select a downloaded or downloadable attachment first".to_owned());
            return Vec::new();
        };
        let key = (message.chat_id, message.id);
        let path: Option<PathBuf> = self.downloaded_attachments.get(&key).cloned().or_else(|| {
            message
                .attachment
                .as_ref()
                .filter(|attachment| !attachment.preview_uses_thumbnail())
                .and_then(|_| self.media_previews.get(&key))
                .and_then(|preview| preview.path.clone())
        });
        if let Some(path) = path.filter(|path| path.is_file()) {
            self.reveal_download(&path);
            return Vec::new();
        }
        self.downloaded_attachments.remove(&key);
        self.reveal_after_download.insert(key);
        self.queue_download(
            message.chat_id,
            message.id,
            message
                .attachment
                .and_then(|attachment| attachment.source_id),
        )
    }

    pub(super) fn reveal_download(&mut self, path: &Path) {
        self.status_message = Some(match reveal_path(path) {
            Ok(()) => format!(
                "Revealed {}",
                sanitize_terminal_line(&path.display().to_string())
            ),
            Err(error) => format!(
                "Could not reveal attachment: {}",
                sanitize_terminal_line(&error.to_string())
            ),
        });
    }
}
