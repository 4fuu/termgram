//! Explicit chat lookup accepts Telegram identities, never external commands.

/// # Errors
/// Rejects empty names, whitespace and targets outside Telegram.
pub fn target(value: &str) -> Result<String, String> {
    let value = value.trim();
    if let Some(username) = value.strip_prefix('@') {
        if username.is_empty()
            || username.len() > 64
            || !username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(
                "Enter a Telegram @username using letters, numbers or underscores".to_owned(),
            );
        }
        return Ok(format!("https://t.me/{username}"));
    }
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err("Use :open @username or a Telegram chat/message link".to_owned());
    }
    crate::app::telegram_link(value).ok_or_else(|| {
        "Use a Telegram @username or t.me link; :chat opens cached names and IDs".to_owned()
    })
}
