//! Small typed-RPC adapter; Grammers delivers join updates on its one ordered stream.
use crate::{
    invites::{Outcome, Preview},
    model::{Chat, ChatKind},
};
use anyhow::{Context, Result, ensure};
use grammers_client::{Client, peer::Peer, tl};
use grammers_session::types::PeerRef;

pub(super) async fn preview(client: &Client, hash: &str) -> Result<(Preview, Option<PeerRef>)> {
    match client
        .invoke(&tl::functions::messages::CheckChatInvite {
            hash: hash.to_owned(),
        })
        .await?
    {
        tl::enums::ChatInvite::Invite(invite) => Ok((
            Preview {
                title: invite.title,
                about: invite.about.unwrap_or_default(),
                participants: Some(u32::try_from(invite.participants_count).unwrap_or(0)),
                request_needed: invite.request_needed,
                warning: (invite.scam || invite.fake)
                    .then(|| "Telegram marks this chat as scam or fake".to_owned()),
                blocked: if invite.subscription_pricing.is_some()
                    || invite.subscription_form_id.is_some()
                {
                    Some("Subscription invites require the official Telegram client".to_owned())
                } else if invite.broadcast || (invite.channel && !invite.megagroup) {
                    Some("Broadcast channels are not supported".to_owned())
                } else {
                    None
                },
                joined: None,
            },
            None,
        )),
        tl::enums::ChatInvite::Already(invite) => from_chat(client, invite.chat, true).await,
        tl::enums::ChatInvite::Peek(invite) => from_chat(client, invite.chat, false).await,
    }
}

async fn from_chat(
    client: &Client,
    raw: tl::enums::Chat,
    joined: bool,
) -> Result<(Preview, Option<PeerRef>)> {
    let request_needed = matches!(&raw, tl::enums::Chat::Channel(c) if c.join_request);
    let resolved = Peer::from_raw(client, raw);
    let blocked = matches!(resolved, Peer::Channel(_))
        .then(|| "Broadcast channels are not supported".to_owned());
    let title = super::peer_display_name(&resolved);
    let mut result = Preview {
        title,
        about: String::new(),
        participants: None,
        request_needed,
        warning: None,
        blocked,
        joined: None,
    };
    let peer = if joined && result.blocked.is_none() {
        let (chat, peer) = chat(resolved).await?;
        result.joined = Some(chat);
        Some(peer)
    } else {
        None
    };
    Ok((result, peer))
}

async fn chat(resolved: Peer) -> Result<(Chat, PeerRef)> {
    ensure!(
        !matches!(resolved, Peer::Channel(_)),
        "Broadcast channels are not supported"
    );
    let peer = resolved
        .to_ref()
        .await
        .map_err(anyhow::Error::from_boxed)?
        .context("Chat cannot be addressed")?;
    Ok((
        Chat {
            id: peer
                .id
                .bot_api_dialog_id()
                .context("Chat has no stable identity")?,
            title: super::peer_display_name(&resolved),
            kind: ChatKind::Group,
            membership: crate::folders::ChatMembership::default(),
            unread: 0,
            read_inbox_max_id: None,
            last_message: String::new(),
            last_message_id: None,
            last_activity: None,
        },
        peer,
    ))
}

pub(super) async fn join(
    client: &Client,
    hash: &str,
    title: &str,
    request_needed: bool,
) -> Result<(Outcome, Option<PeerRef>)> {
    let (fresh, peer) = preview(client, hash).await?;
    ensure!(
        fresh.blocked.is_none(),
        "{}",
        fresh.blocked.as_deref().unwrap_or_default()
    );
    if let Some(chat) = fresh.joined {
        return Ok((Outcome::Joined(chat), peer));
    }
    ensure!(
        crate::model::sanitize_terminal_line(&fresh.title) == title
            && fresh.request_needed == request_needed,
        "Invite changed; close and preview it again"
    );
    let joined = match client
        .invoke(&tl::functions::messages::ImportChatInvite {
            hash: hash.to_owned(),
        })
        .await
    {
        Ok(result) => result,
        Err(error) if error.is("INVITE_REQUEST_SENT") => return Ok((Outcome::Requested, None)),
        Err(error) => return Err(error.into()),
    };
    match joined {
        tl::enums::messages::ChatInviteJoinResult::WebView(_) => Ok((Outcome::Verification, None)),
        tl::enums::messages::ChatInviteJoinResult::Ok(_) => {
            // Recheck membership instead of assuming the first Updates chat is the joined peer.
            let (fresh, peer) = preview(client, hash).await?;
            Ok((
                Outcome::Joined(fresh.joined.context(
                    "Join returned without confirmed membership; reopen the invite to check",
                )?),
                peer,
            ))
        }
    }
}
