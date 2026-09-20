//! Bounded searches use the SDK iterator and the coordinator's single update stream.
use anyhow::{Context, Result, ensure};
use grammers_client::{Client, message::Message, tl::enums::MessagesFilter};
use grammers_session::types::PeerRef;

use crate::cloud_search::{Media, PAGE_SIZE, Request, Sender};

pub(super) struct Page {
    pub messages: Vec<Message>,
    pub next: Option<i32>,
    pub total: usize,
    pub sender: Option<PeerRef>,
}

pub(super) async fn search(
    client: &Client,
    peer: PeerRef,
    request: &Request,
    known_sender: Option<PeerRef>,
) -> Result<Page> {
    ensure!(
        request.query.len() <= 4096,
        "Search text is longer than 4096 bytes"
    );
    let mut iter = client
        .search_messages(peer)
        .query(&request.query)
        .filter(filter(request.filters.media))
        .offset_id(request.before_id)
        .limit(PAGE_SIZE + 1);
    let sender = match &request.filters.sender {
        Some(Sender::Me) => {
            iter = iter.sent_by_self();
            None
        }
        Some(Sender::Username(name)) => {
            let sender = client
                .resolve_username(name)
                .await?
                .context("Sender username was not found")?
                .to_ref()
                .await
                .map_err(anyhow::Error::from_boxed)?
                .context("Sender cannot be addressed")?;
            iter = iter.sent_by(sender);
            Some(sender)
        }
        Some(Sender::Id(id)) => {
            let sender = known_sender
                .filter(|peer| peer.id.bot_api_dialog_id() == Some(*id))
                .context("Sender ID is not known; use @username or me")?;
            iter = iter.sent_by(sender);
            Some(sender)
        }
        None => None,
    };
    if let Some(date) = request.filters.after {
        iter = iter.min_date(
            &date
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc()
                .fixed_offset(),
        );
    }
    if let Some(date) = request.filters.before {
        iter = iter.max_date(
            &date
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc()
                .fixed_offset(),
        );
    }
    let mut messages = Vec::new();
    while let Some(message) = iter.next().await? {
        ensure!(
            message.peer_id() == peer.id,
            "Search returned a message from another chat"
        );
        messages.push(message);
    }
    let total = iter.total().await?;
    let more = messages.len() > PAGE_SIZE;
    messages.truncate(PAGE_SIZE);
    let next = more.then(|| messages.last().expect("full page").id());
    Ok(Page {
        messages,
        next,
        total,
        sender,
    })
}

pub(super) async fn context(client: &Client, peer: PeerRef, id: i32) -> Result<Vec<Message>> {
    let target = super::message_actions::fetch(client, peer, id).await?;
    let mut iter = client
        .iter_messages(peer)
        .offset_id(id)
        .limit(super::HISTORY_LIMIT - 1);
    let mut messages = vec![target];
    while let Some(message) = iter.next().await? {
        ensure!(
            message.peer_id() == peer.id,
            "History returned a message from another chat"
        );
        messages.push(message);
    }
    messages.reverse();
    Ok(messages)
}

fn filter(media: Media) -> MessagesFilter {
    match media {
        Media::All => MessagesFilter::InputMessagesFilterEmpty,
        Media::Photo => MessagesFilter::InputMessagesFilterPhotos,
        Media::Video => MessagesFilter::InputMessagesFilterVideo,
        Media::File => MessagesFilter::InputMessagesFilterDocument,
        Media::Music => MessagesFilter::InputMessagesFilterMusic,
        Media::Voice => MessagesFilter::InputMessagesFilterVoice,
        Media::Round => MessagesFilter::InputMessagesFilterRoundVideo,
        Media::Gif => MessagesFilter::InputMessagesFilterGif,
        Media::Link => MessagesFilter::InputMessagesFilterUrl,
        Media::Poll => MessagesFilter::InputMessagesFilterPoll,
    }
}
