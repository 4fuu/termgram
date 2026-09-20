use crate::{
    entities::{Entity, Kind},
    model::sanitize_terminal_line,
};
use grammers_client::tl;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

/// Telegram entities address the original text in UTF-16. Sanitize once while
/// recording boundaries so tabs and terminal-control removal cannot shift a
/// later entity onto unrelated text. Ignore malformed/split-surrogate ranges.
pub(super) fn map(text: &str, entities: &[tl::enums::MessageEntity]) -> (String, Vec<Entity>) {
    if entities.is_empty() {
        return (crate::model::sanitize_terminal_text(text), Vec::new());
    }
    let mut boundaries = BTreeMap::new();
    let clean = crate::model::sanitize_text_with_offsets(text, |units, byte| {
        boundaries.insert(units, byte);
    });
    let graphemes: Vec<_> = clean
        .grapheme_indices(true)
        .map(|(byte, _)| byte)
        .chain(std::iter::once(clean.len()))
        .collect();
    let entities = entities
        .iter()
        .filter_map(|entity| {
            let start = usize::try_from(entity.offset()).ok()?;
            let length = usize::try_from(entity.length()).ok()?;
            let end = start.checked_add(length)?;
            let mut range = *boundaries.get(&start)?..*boundaries.get(&end)?;
            if range.is_empty() {
                return None;
            }
            // A formatting boundary must never split a terminal grapheme. This
            // also conceals a combining mark's base instead of revealing it.
            range.start = graphemes[graphemes
                .partition_point(|byte| *byte <= range.start)
                .saturating_sub(1)];
            range.end = graphemes[graphemes.partition_point(|byte| *byte < range.end)];
            let kind = match entity {
                tl::enums::MessageEntity::Bold(_) => Kind::Bold,
                tl::enums::MessageEntity::Italic(_) => Kind::Italic,
                tl::enums::MessageEntity::Underline(_) => Kind::Underline,
                tl::enums::MessageEntity::Strike(_) => Kind::Strike,
                tl::enums::MessageEntity::Code(_) => Kind::Code,
                tl::enums::MessageEntity::Pre(value) => Kind::Pre {
                    language: sanitize_terminal_line(&value.language)
                        .chars()
                        .take(32)
                        .collect(),
                },
                tl::enums::MessageEntity::Blockquote(value) => Kind::Quote {
                    collapsed: value.collapsed,
                },
                tl::enums::MessageEntity::Spoiler(_) => Kind::Spoiler,
                tl::enums::MessageEntity::TextUrl(_)
                | tl::enums::MessageEntity::Url(_)
                | tl::enums::MessageEntity::Email(_)
                | tl::enums::MessageEntity::Phone(_) => Kind::Link,
                tl::enums::MessageEntity::Mention(_)
                | tl::enums::MessageEntity::MentionName(_)
                | tl::enums::MessageEntity::InputMessageEntityMentionName(_) => Kind::Mention,
                tl::enums::MessageEntity::Hashtag(_)
                | tl::enums::MessageEntity::Cashtag(_)
                | tl::enums::MessageEntity::BotCommand(_) => Kind::Tag,
                // Custom emoji keep Telegram's Unicode fallback. Newer entity
                // types remain readable without inventing unsupported actions.
                _ => return None,
            };
            Some(Entity { range, kind })
        })
        .collect();
    (clean, entities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_entities_survive_controls_tabs_astral_text_and_combining_marks() {
        let text = "🙂\t\u{1b}[31me\u{301} secret\u{1b}[0m";
        let (clean, entities) = map(
            text,
            &[
                tl::types::MessageEntityBold {
                    offset: 8,
                    length: 2,
                }
                .into(),
                tl::types::MessageEntityItalic {
                    offset: 9,
                    length: 1,
                }
                .into(),
                tl::types::MessageEntitySpoiler {
                    offset: 11,
                    length: 6,
                }
                .into(),
                tl::types::MessageEntityUnderline {
                    offset: 1,
                    length: 1,
                }
                .into(),
                tl::types::MessageEntityCode {
                    offset: -1,
                    length: 3,
                }
                .into(),
                tl::types::MessageEntityCode {
                    offset: 100,
                    length: i32::MAX,
                }
                .into(),
            ],
        );
        assert_eq!(clean, "🙂    e\u{301} secret");
        assert_eq!(entities.len(), 3);
        assert_eq!(&clean[entities[0].range.clone()], "e\u{301}");
        assert_eq!(entities[0].range, entities[1].range);
        assert_eq!(&clean[entities[2].range.clone()], "secret");
        assert_eq!(
            crate::entities::conceal(&clean, &entities),
            "🙂    e\u{301} ▨▨▨▨▨▨"
        );
        let (clean, entities) = map(
            "x\u{200b}y",
            &[tl::types::MessageEntitySpoiler {
                offset: 1,
                length: 1,
            }
            .into()],
        );
        assert_eq!(crate::entities::conceal(&clean, &entities), "xy");
    }
}
