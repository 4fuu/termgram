//! Existing Grammers edit RPC, with fresh permissions and a captured revision.
use super::message_actions::{fetch, revision};
use crate::{editing::Source, model::sanitize_terminal_text};
use anyhow::{Result, ensure};
use grammers_client::{
    Client,
    message::{InputMessage, Message},
    peer::Peer,
    tl,
};
use grammers_session::types::{PeerId, PeerRef};

async fn original(client: &Client, peer: PeerRef, id: i32, self_id: i64) -> Result<Message> {
    let message = fetch(client, peer, id).await?;
    let tl::enums::Message::Message(raw) = &message.raw else {
        anyhow::bail!("Service messages cannot be edited")
    };
    ensure!(
        raw.fwd_from.is_none() && raw.via_bot_id.is_none(),
        "Forwarded and inline-bot messages cannot be edited"
    );
    let channel = match message.peer() {
        Some(Peer::Channel(channel)) => Some(&channel.raw),
        Some(Peer::Group(group)) => match &group.raw {
            tl::enums::Chat::Channel(channel) => Some(channel),
            _ => None,
        },
        _ => None,
    };
    let saved = peer.id == PeerId::user_unchecked(self_id);
    let administer = channel.is_some_and(|channel| {
        channel.creator
            || channel.admin_rights.as_ref().is_some_and(|rights| {
                let tl::enums::ChatAdminRights::Rights(rights) = rights;
                rights.edit_messages
            })
    });
    ensure!(
        saved || raw.out || (raw.post && administer),
        "Only the author or an authorized channel administrator can edit this message"
    );
    let indefinite = saved
        || channel.is_some_and(|channel| {
            channel.creator
                || channel.admin_rights.as_ref().is_some_and(|rights| {
                    let tl::enums::ChatAdminRights::Rights(rights) = rights;
                    if channel.megagroup {
                        rights.pin_messages
                    } else {
                        rights.edit_messages
                    }
                })
        });
    if !indefinite {
        let tl::enums::Config::Config(config) =
            client.invoke(&tl::functions::help::GetConfig {}).await?;
        ensure!(
            i64::from(config.date) - i64::from(raw.date) < i64::from(config.edit_time_limit),
            "Telegram's editing time limit has expired"
        );
    }
    match &raw.media {
        None | Some(tl::enums::MessageMedia::Empty | tl::enums::MessageMedia::WebPage(_)) => {}
        Some(tl::enums::MessageMedia::Photo(media)) => ensure!(
            media.ttl_seconds.is_none(),
            "Expiring photos cannot be edited here"
        ),
        Some(tl::enums::MessageMedia::Document(media)) => {
            ensure!(
                media.ttl_seconds.is_none(),
                "Expiring media cannot be edited here"
            );
            if let Some(tl::enums::Document::Document(document)) = &media.document {
                ensure!(
                    !document.attributes.iter().any(|attribute| matches!(
                        attribute,
                        tl::enums::DocumentAttribute::Sticker(_)
                            | tl::enums::DocumentAttribute::Video(
                                tl::types::DocumentAttributeVideo {
                                    round_message: true,
                                    ..
                                }
                            )
                    )),
                    "This media does not support an editable caption"
                );
            }
        }
        _ => anyhow::bail!("This message type has no editable text or caption"),
    }
    Ok(message)
}

pub(super) async fn load(client: &Client, peer: PeerRef, id: i32, self_id: i64) -> Result<Source> {
    let message = original(client, peer, id, self_id).await?;
    Ok(Source {
        message_id: id,
        text: sanitize_terminal_text(message.text()),
        revision: revision(&message),
        caption: matches!(&message.raw, tl::enums::Message::Message(raw) if matches!(raw.media, Some(tl::enums::MessageMedia::Photo(_) | tl::enums::MessageMedia::Document(_)))),
    })
}

pub(super) async fn save(
    client: &Client,
    peer: PeerRef,
    id: i32,
    self_id: i64,
    expected: [u8; 32],
    text: &str,
) -> Result<Option<Box<Message>>> {
    let message = original(client, peer, id, self_id).await?;
    // A previous attempt may have succeeded just before disconnection.
    if message.text() == text {
        return Ok(Some(Box::new(message)));
    }
    ensure!(
        revision(&message) == expected,
        "Message changed in another client; keep your edit, or discard it and reopen the latest original"
    );
    let entities = remap_entities(
        message.text(),
        text,
        message.fmt_entities().cloned().unwrap_or_default(),
    );
    let mut input = InputMessage::new().text(text).fmt_entities(entities);
    if let tl::enums::Message::Message(raw) = &message.raw {
        input = input.invert_media(raw.invert_media).link_preview(matches!(
            raw.media,
            Some(tl::enums::MessageMedia::WebPage(_))
        ));
    }
    client.edit_message(peer, id, input).await?;
    // grammers-mtsender's process_own_update forwards the RPC's Updates to
    // the existing ordered stream, including its PTS. Do not manufacture a
    // second update from a later history snapshot or advance that cursor here.
    Ok(None)
}

fn remap_entities(
    old: &str,
    new: &str,
    entities: Vec<tl::enums::MessageEntity>,
) -> Vec<tl::enums::MessageEntity> {
    let prefix_chars = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let prefix: i32 = old
        .chars()
        .take(prefix_chars)
        .map(|c| i32::try_from(c.len_utf16()).unwrap_or(0))
        .sum();
    let old_rest: String = old.chars().skip(prefix_chars).collect();
    let new_rest: String = new.chars().skip(prefix_chars).collect();
    let suffix: i32 = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| i32::try_from(c.len_utf16()).unwrap_or(0))
        .sum();
    let old_end = i32::try_from(old.encode_utf16().count()).unwrap_or(i32::MAX) - suffix;
    let new_end = i32::try_from(new.encode_utf16().count()).unwrap_or(i32::MAX) - suffix;
    let delta = new_end - old_end;
    entities
        .into_iter()
        .filter_map(|mut entity| {
            let (offset, length) = (entity.offset(), entity.length());
            let end = offset.checked_add(length)?;
            let style = matches!(
                entity,
                tl::enums::MessageEntity::Bold(_)
                    | tl::enums::MessageEntity::Italic(_)
                    | tl::enums::MessageEntity::Underline(_)
                    | tl::enums::MessageEntity::Strike(_)
                    | tl::enums::MessageEntity::Code(_)
                    | tl::enums::MessageEntity::Pre(_)
                    | tl::enums::MessageEntity::Spoiler(_)
                    | tl::enums::MessageEntity::Blockquote(_)
            );
            let (offset, length) = if end <= prefix {
                (offset, length)
            } else if offset >= old_end {
                (offset + delta, length)
            } else if style && offset <= prefix && end >= old_end {
                (offset, length + delta)
            } else {
                return None;
            };
            if offset < 0 || length <= 0 {
                return None;
            }
            set_range(&mut entity, offset, length);
            Some(entity)
        })
        .collect()
}

fn set_range(entity: &mut tl::enums::MessageEntity, offset: i32, length: i32) {
    macro_rules! ranges {
        ($($variant:ident),+) => { match entity { $(tl::enums::MessageEntity::$variant(value) => { value.offset = offset; value.length = length; }),+ } };
    }
    ranges!(
        Unknown,
        Mention,
        Hashtag,
        BotCommand,
        Url,
        Email,
        Bold,
        Italic,
        Code,
        Pre,
        TextUrl,
        MentionName,
        InputMessageEntityMentionName,
        Phone,
        Cashtag,
        Underline,
        Strike,
        BankCard,
        Spoiler,
        CustomEmoji,
        Blockquote,
        FormattedDate,
        DiffInsert,
        DiffReplace,
        DiffDelete
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_edits_keep_untouched_entities_and_drop_changed_link_targets() {
        let entities = vec![
            tl::types::MessageEntityBold {
                offset: 0,
                length: 10,
            }
            .into(),
            tl::types::MessageEntityTextUrl {
                offset: 4,
                length: 2,
                url: "https://example.com".to_owned(),
            }
            .into(),
            tl::types::MessageEntityCustomEmoji {
                offset: 8,
                length: 2,
                document_id: 42,
            }
            .into(),
        ];
        let mapped = remap_entities("你好🙂abXY🙂", "你好🙂differentXY🙂", entities);
        assert_eq!(mapped.len(), 2);
        assert!(
            matches!(&mapped[0], tl::enums::MessageEntity::Bold(value) if value.offset == 0 && value.length == 17)
        );
        assert!(
            matches!(&mapped[1], tl::enums::MessageEntity::CustomEmoji(value) if value.offset == 15 && value.length == 2 && value.document_id == 42)
        );
        let after = remap_entities(
            "🙂Hello",
            "界🙂Hello",
            vec![
                tl::types::MessageEntityItalic {
                    offset: 2,
                    length: 5,
                }
                .into(),
            ],
        );
        assert_eq!((after[0].offset(), after[0].length()), (3, 5));
    }
}
