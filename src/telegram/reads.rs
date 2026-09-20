use crate::read_state::Snapshot;
use anyhow::{Result, ensure};
use grammers_client::{Client, tl};
use grammers_session::types::{PeerKind, PeerRef};

pub(super) async fn mark(client: &Client, peer: PeerRef, max_id: i32) -> Result<Option<Snapshot>> {
    ensure!(
        max_id > 0,
        "Automatic read receipts require a positive visible message ID"
    );
    // Same typed requests as Grammers Message::mark_as_read. The UI retains
    // only message metadata, so no redundant getMessage RPC is needed first.
    if peer.id.kind() == PeerKind::Channel {
        client
            .invoke(&tl::functions::channels::ReadHistory {
                channel: peer.into(),
                max_id,
            })
            .await?;
    } else {
        client
            .invoke(&tl::functions::messages::ReadHistory {
                peer: peer.into(),
                max_id,
            })
            .await?;
    }
    Ok(snapshot(client, peer).await)
}

pub(super) async fn set_unread(
    client: &Client,
    peer: PeerRef,
    unread: bool,
    read_history: bool,
) -> Result<Option<Snapshot>> {
    if read_history {
        // Only an explicit :read action may acknowledge the whole history.
        client.mark_as_read(peer).await?;
    }
    client
        .invoke(&tl::functions::messages::MarkDialogUnread {
            unread,
            parent_peer: None,
            peer: tl::types::InputDialogPeer { peer: peer.into() }.into(),
        })
        .await?;
    Ok(snapshot(client, peer).await)
}

async fn snapshot(client: &Client, peer: PeerRef) -> Option<Snapshot> {
    // Retrieve this peer's actual remaining count, without a full dialog scan.
    // Count lookup failure does not undo a successful read acknowledgement.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.invoke(&tl::functions::messages::GetPeerDialogs {
            peers: vec![tl::types::InputDialogPeer { peer: peer.into() }.into()],
        }),
    )
    .await;
    let Ok(Ok(tl::enums::messages::PeerDialogs::Dialogs(dialogs))) = result else {
        return None;
    };
    dialogs.dialogs.into_iter().find_map(|dialog| match dialog {
        tl::enums::Dialog::Dialog(dialog) => Some(Snapshot {
            max_id: dialog.read_inbox_max_id.max(0),
            unread: u32::try_from(dialog.unread_count.max(0)).unwrap_or_default(),
            top_message: dialog.top_message,
        }),
        tl::enums::Dialog::Folder(_) => None,
    })
}
