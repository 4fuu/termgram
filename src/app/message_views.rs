use super::App;
use crate::model::Message;

impl App {
    /// Partial content updates address one Telegram identity in every loaded
    /// view, including replies and in-flight history overlays.
    pub(super) fn visit_messages(&mut self, mut visit: impl FnMut(&mut Message)) {
        for message in self.messages.values_mut().flatten() {
            visit(message);
        }
        for message in self.history_changes.values_mut().flatten() {
            visit(message);
        }
        for message in &mut self.message_pins.page.messages {
            visit(message);
        }
        if let Some((_, page)) = &mut self.message_pins.head {
            for message in &mut page.messages {
                visit(message);
            }
        }
        if let Some(page) = &mut self.search.page {
            for message in page.messages_mut() {
                visit(message);
            }
        }
        self.replies.visit_messages(&mut visit);
        if let Some(plan) = self
            .forwarding
            .review
            .as_mut()
            .and_then(|review| review.plan.as_mut())
        {
            visit(&mut plan.message);
        }
        if let Some(plan) = self
            .deletion
            .prompt
            .as_mut()
            .and_then(|prompt| prompt.plan.as_mut())
        {
            visit(&mut plan.message);
        }
        for chat in &mut self.chats {
            if let Some(message) = self.messages.get(&chat.id).and_then(|messages| {
                messages
                    .iter()
                    .find(|message| Some(message.id) == chat.last_message_id)
            }) {
                chat.last_message = message.preview_text();
            }
        }
    }
}
