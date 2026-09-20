use crate::chat_info::{Content, Info, Restriction};
use anyhow::{Context, Result};
use grammers_client::{Client, InvocationError, peer::Peer, tl};
use grammers_session::types::{PeerKind, PeerRef};

pub(super) async fn load(client: &Client, peer: PeerRef) -> Result<Info> {
    if peer.id.kind() == PeerKind::User {
        user_info(client, peer).await
    } else {
        group_info(client, peer).await
    }
}

async fn user_info(client: &Client, peer: PeerRef) -> Result<Info> {
    let tl::enums::users::UserFull::Full(full) = client
        .invoke(&tl::functions::users::GetFullUser { id: peer.into() })
        .await?;
    let raw = full
        .users
        .into_iter()
        .find(|user| user.id() == peer.id.bare_id_unchecked())
        .context("User details did not include this user")?;
    let resolved = Peer::User(grammers_client::peer::User::from_raw(client, raw));
    let tl::enums::UserFull::Full(full) = full.full_user;
    let mut info = Info {
        title: super::peer_display_name(&resolved),
        username: resolved.username().map(str::to_owned),
        about: full.about.unwrap_or_default(),
        role: "Private conversation".to_owned(),
        ..Info::default()
    };
    if full.blocked {
        block(&mut info, "Unblock this user in Telegram before sending");
    }
    if full.send_paid_messages_stars.is_some_and(|n| n > 0) {
        block(
            &mut info,
            "This user requires paid messages; use the official client",
        );
    }
    if let Peer::User(user) = resolved
        && matches!(user.raw, tl::enums::User::User(u) if u.deleted)
    {
        block(&mut info, "This account was deleted");
    }
    Ok(info)
}

async fn group_info(client: &Client, peer: PeerRef) -> Result<Info> {
    let tl::enums::messages::ChatFull::Full(full) = if peer.id.kind() == PeerKind::Channel {
        client
            .invoke(&tl::functions::channels::GetFullChannel {
                channel: peer.into(),
            })
            .await?
    } else {
        client
            .invoke(&tl::functions::messages::GetFullChat {
                chat_id: peer.id.bare_id_unchecked(),
            })
            .await?
    };
    let raw = full
        .chats
        .into_iter()
        .find(|chat| chat.id() == peer.id.bare_id_unchecked())
        .context("Chat details did not include its current permissions")?;
    let boosted = matches!(&full.full_chat, tl::enums::ChatFull::ChannelFull(full) if full.boosts_unrestrict.is_some_and(|required| required > 0 && full.boosts_applied.unwrap_or(0) >= required));
    let mut info = group_permissions(&raw, boosted);
    match full.full_chat {
        tl::enums::ChatFull::Full(full) => info.about = full.about,
        tl::enums::ChatFull::ChannelFull(full) => {
            info.about = full.about;
            info.members = full.participants_count.and_then(|n| u32::try_from(n).ok());
            let exempt = matches!(&raw, tl::enums::Chat::Channel(c) if c.creator || c.admin_rights.is_some())
                || full.boosts_unrestrict.is_some_and(|required| {
                    required > 0 && full.boosts_applied.unwrap_or(0) >= required
                });
            if !exempt {
                info.slow_seconds = u32::try_from(full.slowmode_seconds.unwrap_or(0)).unwrap_or(0);
                info.next_send_at = i64::from(full.slowmode_next_send_date.unwrap_or(0));
            }
            if full.send_paid_messages_stars.is_some_and(|n| n > 0) {
                block(
                    &mut info,
                    "This group requires paid messages; use the official client",
                );
            }
        }
    }
    Ok(info)
}

fn group_permissions(raw: &tl::enums::Chat, boosted: bool) -> Info {
    let mut info = Info::default();
    match raw {
        tl::enums::Chat::Chat(chat) => {
            info.title.clone_from(&chat.title);
            info.members = u32::try_from(chat.participants_count).ok();
            let admin = chat.creator || chat.admin_rights.is_some();
            info.role = role(chat.creator, admin, chat.left);
            if chat.left || chat.deactivated {
                block(
                    &mut info,
                    "This group is no longer writable; reopen its current invite or migrated group",
                );
            }
            if !admin {
                restrictions(
                    &mut info,
                    chat.default_banned_rights.as_ref(),
                    "Group permissions",
                    false,
                );
            }
        }
        tl::enums::Chat::Channel(chat) => {
            info.title.clone_from(&chat.title);
            info.username.clone_from(&chat.username);
            let admin = chat.creator || chat.admin_rights.is_some();
            info.role = role(chat.creator, admin, chat.left);
            if chat.broadcast {
                block(&mut info, "Broadcast channels are not supported");
            }
            if chat.left && chat.join_to_send {
                block(&mut info, "Join this group before sending");
            }
            if chat.gigagroup && !admin {
                block(&mut info, "Only administrators can send in this group");
            }
            if !admin {
                restrictions(
                    &mut info,
                    chat.banned_rights.as_ref(),
                    "Your permissions",
                    true,
                );
            }
            if !admin && !boosted {
                restrictions(
                    &mut info,
                    chat.default_banned_rights.as_ref(),
                    "Group permissions",
                    false,
                );
            }
        }
        _ => {
            "Unavailable group".clone_into(&mut info.title);
            block(&mut info, "You cannot access this group");
        }
    }
    info
}

fn role(creator: bool, admin: bool, left: bool) -> String {
    if creator {
        "Owner"
    } else if admin {
        "Administrator"
    } else if left {
        "Not a member"
    } else {
        "Member"
    }
    .to_owned()
}

fn block(info: &mut Info, reason: &str) {
    info.restrictions.push(Restriction {
        content: None,
        reason: reason.to_owned(),
        until: 0,
    });
}

fn restrictions(
    info: &mut Info,
    rights: Option<&tl::enums::ChatBannedRights>,
    source: &str,
    personal: bool,
) {
    let Some(tl::enums::ChatBannedRights::Rights(rights)) = rights else {
        return;
    };
    let until = if personal {
        i64::from(rights.until_date).max(0)
    } else {
        0
    };
    let all = rights.view_messages || rights.send_messages;
    for (content, blocked, label) in [
        (Content::Text, all || rights.send_plain, "text messages"),
        (
            Content::Photo,
            all || rights.send_media || rights.send_photos,
            "photos",
        ),
        (
            Content::File,
            all || rights.send_media || rights.send_docs,
            "files",
        ),
    ] {
        if blocked {
            info.restrictions.push(Restriction {
                content: Some(content),
                reason: format!("{source}: sending {label} is restricted"),
                until,
            });
        }
    }
}

/// Preserve raw RPC detail for unknown failures, but describe common write restrictions.
pub(super) fn send_error(error: &anyhow::Error) -> String {
    let Some(InvocationError::Rpc(rpc)) = error.downcast_ref::<InvocationError>() else {
        return format!("{error:#}");
    };
    let hint = match rpc.name.as_str() {
        "SLOWMODE_WAIT" => {
            return format!(
                "Slow mode: wait {} s before sending again",
                rpc.value.unwrap_or(0)
            );
        }
        "CHAT_WRITE_FORBIDDEN" | "CHAT_SEND_PLAIN_FORBIDDEN" => {
            "You cannot send text in this chat; use :info to check permissions"
        }
        "CHAT_SEND_MEDIA_FORBIDDEN" | "CHAT_SEND_PHOTOS_FORBIDDEN" | "CHAT_SEND_DOCS_FORBIDDEN" => {
            "This media type is restricted in the chat; use :info to check permissions"
        }
        "USER_BANNED_IN_CHANNEL" => "You are restricted from sending in this group",
        "USER_IS_BLOCKED" | "YOU_BLOCKED_USER" => {
            "This user is blocked; check Telegram's block settings"
        }
        "CHAT_GUEST_SEND_FORBIDDEN" => "Join this group before sending",
        "PRIVACY_PREMIUM_REQUIRED" => "This recipient requires Telegram Premium to contact them",
        _ => return format!("{error:#}"),
    };
    format!("{hint} ({})", rpc.name)
}
