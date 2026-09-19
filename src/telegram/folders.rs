//! Small protocol adapter for native folders and inherited notification settings.
use crate::{
    folders::{ChatMembership, Folder},
    model::ChatId,
    model::sanitize_terminal_line,
};
use anyhow::Result;
use grammers_client::{Client, tl};
use grammers_session::types::PeerId;

pub(super) async fn load(client: &Client, self_id: i64) -> Result<Vec<Folder>> {
    let tl::enums::messages::DialogFilters::Filters(result) = client
        .invoke(&tl::functions::messages::GetDialogFilters {})
        .await?;
    let mut folders: Vec<_> = result
        .filters
        .into_iter()
        .map(|filter| match filter {
            tl::enums::DialogFilter::Default => Folder::all(),
            tl::enums::DialogFilter::Filter(filter) => {
                let tl::enums::TextWithEntities::Entities(title) = filter.title;
                Folder {
                    id: filter.id,
                    title: sanitize_terminal_line(&title.text),
                    pinned: peer_ids(filter.pinned_peers, self_id),
                    include: peer_ids(filter.include_peers, self_id),
                    exclude: peer_ids(filter.exclude_peers, self_id),
                    contacts: filter.contacts,
                    non_contacts: filter.non_contacts,
                    bots: filter.bots,
                    groups: filter.groups,
                    broadcasts: filter.broadcasts,
                    exclude_muted: filter.exclude_muted,
                    exclude_read: filter.exclude_read,
                    exclude_archived: filter.exclude_archived,
                }
            }
            tl::enums::DialogFilter::Chatlist(filter) => {
                let tl::enums::TextWithEntities::Entities(title) = filter.title;
                Folder {
                    id: filter.id,
                    title: sanitize_terminal_line(&title.text),
                    pinned: peer_ids(filter.pinned_peers, self_id),
                    include: peer_ids(filter.include_peers, self_id),
                    ..Folder::default()
                }
            }
        })
        .collect();
    if !folders.iter().any(|folder| folder.id == 0) {
        folders.insert(0, Folder::all());
    }
    Ok(folders)
}

fn peer_ids(peers: Vec<tl::enums::InputPeer>, self_id: i64) -> Vec<ChatId> {
    peers
        .into_iter()
        .filter_map(|peer| match peer {
            tl::enums::InputPeer::User(peer) => PeerId::user(peer.user_id),
            tl::enums::InputPeer::Chat(peer) => PeerId::chat(peer.chat_id),
            tl::enums::InputPeer::Channel(peer) => PeerId::channel(peer.channel_id),
            tl::enums::InputPeer::UserFromMessage(peer) => PeerId::user(peer.user_id),
            tl::enums::InputPeer::ChannelFromMessage(peer) => PeerId::channel(peer.channel_id),
            tl::enums::InputPeer::PeerSelf => PeerId::user(self_id),
            tl::enums::InputPeer::Empty => None,
        })
        .filter_map(PeerId::bot_api_dialog_id)
        .collect()
}

pub(super) async fn default_mutes(client: &Client) -> Result<(i64, i64)> {
    let (users, groups) = tokio::try_join!(
        client.invoke(&tl::functions::account::GetNotifySettings {
            peer: tl::enums::InputNotifyPeer::InputNotifyUsers
        }),
        client.invoke(&tl::functions::account::GetNotifySettings {
            peer: tl::enums::InputNotifyPeer::InputNotifyChats
        }),
    )?;
    let tl::enums::PeerNotifySettings::Settings(users) = users;
    let tl::enums::PeerNotifySettings::Settings(groups) = groups;
    Ok((
        i64::from(users.mute_until.unwrap_or(0)),
        i64::from(groups.mute_until.unwrap_or(0)),
    ))
}

pub(super) fn membership(
    dialog: &grammers_client::peer::Dialog,
    defaults: (i64, i64),
) -> ChatMembership {
    let tl::enums::Dialog::Dialog(raw) = &dialog.raw else {
        return ChatMembership::default();
    };
    let user = match dialog.peer() {
        grammers_client::peer::Peer::User(user) => Some(user),
        _ => None,
    };
    let tl::enums::PeerNotifySettings::Settings(notify) = &raw.notify_settings;
    ChatMembership {
        contact: user.is_some_and(grammers_client::peer::User::contact),
        bot: user.is_some_and(grammers_client::peer::User::is_bot),
        archived: raw.folder_id == Some(1),
        unread_mark: raw.unread_mark,
        mentions: raw.unread_mentions_count > 0,
        mute_until: notify.mute_until.map_or(
            if user.is_some() {
                defaults.0
            } else {
                defaults.1
            },
            i64::from,
        ),
    }
}
