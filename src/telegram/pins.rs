//! Native pin RPC adapters. Filter edits start from fresh, complete server data.

use anyhow::{Context, Result, ensure};
use grammers_client::{
    Client,
    peer::{Peer, User},
    tl,
};
use grammers_session::types::{PeerId, PeerRef};

use crate::pins::{DialogAction, DialogPins, DialogScope, move_pin};

pub(super) async fn change_message(
    client: &Client,
    peer: PeerRef,
    id: i32,
    action: crate::pins::MessageAction,
) -> Result<()> {
    use crate::pins::MessageAction;
    match action {
        MessageAction::Pin { notify, only_self } => {
            ensure!(id > 0, "Only sent messages can be pinned");
            client
                .invoke(&tl::functions::messages::UpdatePinnedMessage {
                    silent: !notify,
                    unpin: false,
                    pm_oneside: only_self,
                    peer: peer.into(),
                    id,
                })
                .await?;
        }
        MessageAction::Unpin => {
            client.unpin_message(peer, id).await?;
        }
        MessageAction::UnpinAll => {
            // The server can split large unpin operations into several batches.
            loop {
                let tl::enums::messages::AffectedHistory::History(result) = client
                    .invoke(&tl::functions::messages::UnpinAllMessages {
                        peer: peer.into(),
                        top_msg_id: None,
                        saved_peer_id: None,
                    })
                    .await?;
                if result.offset == 0 {
                    break;
                }
            }
        }
    }
    Ok(())
}

pub(super) async fn load_dialogs(client: &Client) -> Result<DialogPins> {
    let (main, archive) = tokio::try_join!(get_dialogs(client, 0), get_dialogs(client, 1))?;
    Ok(DialogPins {
        main: dialog_ids(&main),
        archive: dialog_ids(&archive),
    })
}

async fn get_dialogs(client: &Client, folder_id: i32) -> Result<tl::types::messages::PeerDialogs> {
    let tl::enums::messages::PeerDialogs::Dialogs(dialogs) = client
        .invoke(&tl::functions::messages::GetPinnedDialogs { folder_id })
        .await?;
    Ok(dialogs)
}

fn dialog_ids(dialogs: &tl::types::messages::PeerDialogs) -> Vec<i64> {
    dialogs
        .dialogs
        .iter()
        .filter_map(|dialog| match dialog {
            tl::enums::Dialog::Dialog(dialog) => {
                PeerId::from(dialog.peer.clone()).bot_api_dialog_id()
            }
            tl::enums::Dialog::Folder(_) => None,
        })
        .collect()
}

pub(super) async fn change_dialog(
    client: &Client,
    peer: PeerRef,
    scope: DialogScope,
    action: DialogAction,
    self_id: i64,
) -> Result<()> {
    if let DialogScope::Filter(id) = scope {
        return change_filter(client, peer, id, action, self_id).await;
    }
    if let DialogAction::Set(pinned) = action {
        ensure!(
            client
                .invoke(&tl::functions::messages::ToggleDialogPin {
                    pinned,
                    peer: tl::types::InputDialogPeer { peer: peer.into() }.into(),
                })
                .await?,
            "Telegram did not change the chat pin"
        );
        return Ok(());
    }
    let folder_id = i32::from(scope == DialogScope::Archive);
    let dialogs = get_dialogs(client, folder_id).await?;
    let ids = dialog_ids(&dialogs);
    let position = ids
        .iter()
        .position(|id| *id == peer.id.bot_api_dialog_id_unchecked())
        .context("This chat is no longer pinned; refresh and try again")?;
    let peers = dialogs
        .users
        .into_iter()
        .map(|user| Peer::User(User::from_raw(client, user)))
        .chain(
            dialogs
                .chats
                .into_iter()
                .map(|chat| Peer::from_raw(client, chat)),
        )
        .map(|peer| (peer.id(), peer))
        .collect::<std::collections::HashMap<_, _>>();
    let mut order = Vec::new();
    for dialog in dialogs.dialogs {
        match dialog {
            tl::enums::Dialog::Dialog(dialog) => {
                let id = PeerId::from(dialog.peer);
                let reference = peers
                    .get(&id)
                    .context("Pinned peer is unavailable")?
                    .to_ref()
                    .await
                    .map_err(anyhow::Error::from_boxed)?
                    .context("Pinned peer has no access reference")?;
                order.push(
                    tl::types::InputDialogPeer {
                        peer: reference.into(),
                    }
                    .into(),
                );
            }
            tl::enums::Dialog::Folder(_) => {}
        }
    }
    move_pin(&mut order, position, action);
    ensure!(
        client
            .invoke(&tl::functions::messages::ReorderPinnedDialogs {
                force: false,
                folder_id,
                order,
            })
            .await?,
        "Telegram did not reorder pinned chats"
    );
    Ok(())
}

async fn change_filter(
    client: &Client,
    peer: PeerRef,
    id: i32,
    action: DialogAction,
    self_id: i64,
) -> Result<()> {
    let tl::enums::messages::DialogFilters::Filters(filters) = client
        .invoke(&tl::functions::messages::GetDialogFilters {})
        .await?;
    let mut filter = filters
        .filters
        .into_iter()
        .find(|filter| match filter {
            tl::enums::DialogFilter::Filter(filter) => filter.id == id,
            tl::enums::DialogFilter::Chatlist(filter) => filter.id == id,
            tl::enums::DialogFilter::Default => false,
        })
        .context("This Telegram folder no longer exists")?;
    edit_filter_pins(&mut filter, peer.into(), action, self_id)?;
    ensure!(
        client
            .invoke(&tl::functions::messages::UpdateDialogFilter {
                id,
                filter: Some(filter),
            })
            .await?,
        "Telegram did not update folder pins"
    );
    Ok(())
}

fn edit_filter_pins(
    filter: &mut tl::enums::DialogFilter,
    peer: tl::enums::InputPeer,
    action: DialogAction,
    self_id: i64,
) -> Result<()> {
    let (pinned, included) = match filter {
        tl::enums::DialogFilter::Filter(filter) => {
            (&mut filter.pinned_peers, &mut filter.include_peers)
        }
        tl::enums::DialogFilter::Chatlist(filter) => {
            (&mut filter.pinned_peers, &mut filter.include_peers)
        }
        tl::enums::DialogFilter::Default => anyhow::bail!("Default folder pins use the dialog API"),
    };
    let id = super::folders::peer_id(&peer, self_id);
    let position = pinned
        .iter()
        .position(|candidate| super::folders::peer_id(candidate, self_id) == id);
    // Desktop's `always` set is the union of include_peers and pinned_peers.
    // Unpinning retains explicit membership; serialization excludes pins from
    // include_peers (tdesktop 4d4da471, data/data_chat_filters.cpp).
    for candidate in pinned.iter() {
        let id = super::folders::peer_id(candidate, self_id);
        if !included
            .iter()
            .any(|peer| super::folders::peer_id(peer, self_id) == id)
        {
            included.push(candidate.clone());
        }
    }
    match action {
        DialogAction::Set(true) if position.is_none() => {
            pinned.insert(0, peer.clone());
            if !included
                .iter()
                .any(|candidate| super::folders::peer_id(candidate, self_id) == id)
            {
                included.push(peer);
            }
        }
        DialogAction::Set(false) => {
            if let Some(position) = position {
                pinned.remove(position);
            }
        }
        DialogAction::Set(true) => {}
        _ => move_pin(
            pinned,
            position.context("This chat is no longer pinned in this folder")?,
            action,
        ),
    }
    included.retain(|peer| {
        !pinned.iter().any(|candidate| {
            super::folders::peer_id(peer, self_id) == super::folders::peer_id(candidate, self_id)
        })
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpin_keeps_explicit_membership_and_preserves_server_filter_fields() {
        let peer = |id| tl::enums::InputPeer::Chat(tl::types::InputPeerChat { chat_id: id });
        let original = tl::types::DialogFilter {
            contacts: false,
            non_contacts: true,
            groups: true,
            broadcasts: true,
            bots: false,
            exclude_muted: true,
            exclude_read: true,
            exclude_archived: true,
            title_noanimate: true,
            id: 7,
            title: tl::types::TextWithEntities {
                text: "Work".into(),
                entities: vec![],
            }
            .into(),
            emoticon: Some("x".into()),
            color: Some(4),
            pinned_peers: vec![peer(1), peer(2)],
            include_peers: vec![peer(3)],
            exclude_peers: vec![peer(4)],
        };
        let mut filter: tl::enums::DialogFilter = original.clone().into();
        edit_filter_pins(&mut filter, peer(2), DialogAction::Set(false), 10).unwrap();
        let mut expected = original.clone();
        expected.pinned_peers = vec![peer(1)];
        expected.include_peers = vec![peer(3), peer(2)];
        assert_eq!(filter, expected.into());
        edit_filter_pins(&mut filter, peer(2), DialogAction::Set(true), 10).unwrap();
        edit_filter_pins(&mut filter, peer(2), DialogAction::MoveDown, 10).unwrap();
        assert_eq!(filter, original.into());
    }
}
