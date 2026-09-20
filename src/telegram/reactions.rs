use crate::reactions::{Choice, Count, Kind, Review, Summary};
use anyhow::{Context, Result, ensure};
use grammers_client::{
    Client,
    message::{InputReactions, Message},
    tl,
};
use grammers_session::types::{PeerId, PeerKind, PeerRef};

fn kind(raw: &tl::enums::Reaction) -> Option<Kind> {
    match raw {
        tl::enums::Reaction::Emoji(value) => Some(Kind::Emoji(value.emoticon.clone())),
        tl::enums::Reaction::CustomEmoji(value) => Some(Kind::Custom(value.document_id)),
        tl::enums::Reaction::Paid => Some(Kind::Paid),
        tl::enums::Reaction::Empty => None,
    }
}

pub(super) fn summary(raw: &tl::enums::MessageReactions) -> Summary {
    let tl::enums::MessageReactions::Reactions(raw) = raw;
    Summary {
        counts: raw
            .results
            .iter()
            .filter_map(|count| {
                let tl::enums::ReactionCount::Count(count) = count;
                Some(Count {
                    kind: kind(&count.reaction)?,
                    count: u32::try_from(count.count).ok()?,
                    chosen: if raw.min { None } else { count.chosen_order },
                })
            })
            .collect(),
        choices_known: !raw.min,
        as_tags: raw.reactions_as_tags,
        stale: false,
    }
}

pub(super) fn map(message: &Message) -> Option<Summary> {
    match &message.raw {
        tl::enums::Message::Message(message) => message.reactions.as_ref().map(summary),
        tl::enums::Message::Service(message) => message.reactions.as_ref().map(summary),
        tl::enums::Message::Empty(_) => None,
    }
}

async fn own_choices(
    client: &Client,
    peer: PeerRef,
    id: i32,
    mut current: Summary,
) -> Result<Summary> {
    if current.choices_known {
        return Ok(current);
    }
    let updates = client
        .invoke(&tl::functions::messages::GetMessagesReactions {
            peer: peer.into(),
            id: vec![id],
        })
        .await?;
    let updates = match updates {
        tl::enums::Updates::Updates(updates) => updates.updates,
        tl::enums::Updates::Combined(updates) => updates.updates,
        tl::enums::Updates::UpdateShort(update) => vec![update.update],
        _ => Vec::new(),
    };
    for update in updates {
        if let tl::enums::Update::MessageReactions(update) = update
            && update.msg_id == id
            && PeerId::from(update.peer) == peer.id
        {
            current.apply(&summary(&update.reactions));
        }
    }
    ensure!(
        current.choices_known,
        "Telegram did not return your reaction choices; refresh"
    );
    Ok(current)
}

async fn allowed(
    client: &Client,
    peer: PeerRef,
) -> Result<(tl::enums::ChatReactions, Option<i32>)> {
    let full = match peer.id.kind() {
        PeerKind::User => {
            return Ok((
                tl::types::ChatReactionsAll { allow_custom: true }.into(),
                None,
            ));
        }
        PeerKind::Chat => {
            client
                .invoke(&tl::functions::messages::GetFullChat {
                    chat_id: peer.into(),
                })
                .await?
        }
        PeerKind::Channel => {
            client
                .invoke(&tl::functions::channels::GetFullChannel {
                    channel: peer.into(),
                })
                .await?
        }
    };
    let tl::enums::messages::ChatFull::Full(full) = full;
    let (reactions, limit) = match full.full_chat {
        tl::enums::ChatFull::Full(full) => (full.available_reactions, full.reactions_limit),
        tl::enums::ChatFull::ChannelFull(full) => (full.available_reactions, full.reactions_limit),
    };
    Ok((reactions.unwrap_or(tl::enums::ChatReactions::None), limit))
}

fn config_limit(config: &tl::enums::help::AppConfig, key: &str) -> Option<usize> {
    let tl::enums::help::AppConfig::Config(config) = config else {
        return None;
    };
    let tl::enums::Jsonvalue::JsonObject(object) = &config.config else {
        return None;
    };
    object.value.iter().find_map(|entry| {
        let tl::enums::JsonobjectValue::JsonObjectValue(entry) = entry;
        if entry.key != key {
            return None;
        }
        let tl::enums::Jsonvalue::JsonNumber(number) = &entry.value else {
            return None;
        };
        if !number.value.is_finite() || number.value.fract() != 0.0 {
            return None;
        }
        // Parse rather than truncating an unbounded floating-point value.
        number.value.to_string().parse().ok()
    })
}

pub(super) async fn review(
    client: &Client,
    peer: PeerRef,
    message_id: i32,
    self_id: i64,
) -> Result<Review> {
    let (message, me, available, config, (allowed, unique)) = tokio::try_join!(
        super::message_actions::fetch(client, peer, message_id),
        async { Ok::<_, anyhow::Error>(client.get_me().await?) },
        async {
            Ok::<_, anyhow::Error>(
                client
                    .invoke(&tl::functions::messages::GetAvailableReactions { hash: 0 })
                    .await?,
            )
        },
        async {
            Ok::<_, anyhow::Error>(
                client
                    .invoke(&tl::functions::help::GetAppConfig { hash: 0 })
                    .await?,
            )
        },
        allowed(client, peer),
    )?;
    let possible = match &message.raw {
        tl::enums::Message::Message(_) => true,
        tl::enums::Message::Service(message) => message.reactions_are_possible,
        tl::enums::Message::Empty(_) => false,
    };
    let summary = map(&message).unwrap_or(Summary {
        choices_known: true,
        ..Summary::default()
    });
    let summary = own_choices(client, peer, message_id, summary).await?;
    let premium = matches!(me.raw, tl::enums::User::User(user) if user.premium);
    let max_chosen = config_limit(
        &config,
        if premium {
            "reactions_user_max_premium"
        } else {
            "reactions_user_max_default"
        },
    )
    .unwrap_or(if premium { 3 } else { 1 });
    let max_unique = unique
        .and_then(|n| usize::try_from(n).ok())
        .or_else(|| config_limit(&config, "reactions_uniq_max"))
        .unwrap_or(usize::MAX);
    let tl::enums::messages::AvailableReactions::Reactions(available) = available else {
        anyhow::bail!("Telegram did not return available reactions; refresh");
    };
    let choices = available
        .reactions
        .into_iter()
        .filter_map(|reaction| {
            let tl::enums::AvailableReaction::Reaction(reaction) = reaction;
            let permitted = match &allowed {
                tl::enums::ChatReactions::None => false,
                tl::enums::ChatReactions::All(_) => true,
                tl::enums::ChatReactions::Some(allowed) => allowed
                    .reactions
                    .iter()
                    .any(|item| kind(item) == Some(Kind::Emoji(reaction.reaction.clone()))),
            };
            (permitted && !reaction.inactive && (!reaction.premium || premium)).then(|| Choice {
                emoji: reaction.reaction,
                title: crate::model::sanitize_terminal_line(&reaction.title),
            })
        })
        .collect();
    let read_only = if peer.id == PeerId::user_unchecked(self_id) || summary.as_tags {
        Some("Saved Messages use tags; manage them in an official client".to_owned())
    } else if !possible {
        Some("This service message does not accept reactions".to_owned())
    } else {
        None
    };
    Ok(Review {
        summary,
        choices,
        max_chosen,
        max_unique,
        read_only,
    })
}

pub(super) async fn change(
    client: &Client,
    peer: PeerRef,
    message_id: i32,
    self_id: i64,
    expected: &[Kind],
    emoji: Option<&str>,
) -> Result<()> {
    let latest = review(client, peer, message_id, self_id).await?;
    ensure!(
        latest.summary.chosen() == expected,
        "Your reactions changed in another client; refresh before retrying"
    );
    let desired = latest.toggle(emoji).map_err(anyhow::Error::msg)?;
    let reactions: Vec<tl::enums::Reaction> = desired
        .into_iter()
        .filter_map(|kind| match kind {
            Kind::Emoji(emoticon) => Some(tl::types::ReactionEmoji { emoticon }.into()),
            Kind::Custom(document_id) => {
                Some(tl::types::ReactionCustomEmoji { document_id }.into())
            }
            Kind::Paid => None,
        })
        .collect();
    client
        .send_reactions(
            peer,
            message_id,
            InputReactions::from(reactions).add_to_recent(),
        )
        .await
        .context("Could not change reactions")?;
    Ok(())
}

pub(super) async fn refresh(client: &Client, peer: PeerRef, ids: Vec<i32>) -> Result<()> {
    client
        .invoke(&tl::functions::messages::GetMessagesReactions {
            peer: peer.into(),
            id: ids,
        })
        .await?;
    Ok(())
}
