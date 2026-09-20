use super::entities;
use crate::polls::{Answer, Count, Definition, Poll, Results, ResultsPatch, Text, Update};
use anyhow::{Context, Result, ensure};
use grammers_client::{Client, message::Message, tl};
use grammers_session::types::PeerRef;

fn text(value: &tl::enums::TextWithEntities) -> Text {
    let tl::enums::TextWithEntities::Entities(value) = value;
    let (text, entities) = entities::map(&value.text, &value.entities);
    Text { text, entities }
}

fn definition(raw: &tl::types::Poll, has_media: bool) -> Definition {
    Definition {
        id: raw.id,
        hash: raw.hash,
        question: text(&raw.question),
        answers: raw
            .answers
            .iter()
            .filter_map(|answer| {
                let tl::enums::PollAnswer::Answer(answer) = answer else {
                    return None;
                };
                Some(Answer {
                    option: answer.option.clone(),
                    text: text(&answer.text),
                    has_media: answer.media.is_some(),
                })
            })
            .collect(),
        closed: raw.closed,
        public_voters: raw.public_voters,
        multiple_choice: raw.multiple_choice,
        quiz: raw.quiz,
        revoting_disabled: raw.revoting_disabled,
        hide_results_until_close: raw.hide_results_until_close,
        subscribers_only: raw.subscribers_only,
        countries: raw
            .countries_iso2
            .iter()
            .flatten()
            .map(|country| crate::model::sanitize_terminal_line(country))
            .collect(),
        close_date: raw.close_date.map(i64::from),
        has_media,
    }
}

fn results(raw: &tl::types::PollResults) -> ResultsPatch {
    ResultsPatch {
        min: raw.min,
        counts: raw.results.as_ref().map(|counts| {
            counts
                .iter()
                .map(|count| {
                    let tl::enums::PollAnswerVoters::Voters(count) = count;
                    Count {
                        option: count.option.clone(),
                        voters: count.voters.and_then(|n| u32::try_from(n).ok()),
                        chosen: count.chosen,
                        correct: count.correct,
                    }
                })
                .collect()
        }),
        total: raw.total_voters.and_then(|n| u32::try_from(n).ok()),
        solution: raw.solution.as_ref().map(|value| {
            let (text, entities) =
                entities::map(value, raw.solution_entities.as_deref().unwrap_or_default());
            Text { text, entities }
        }),
    }
}

pub(super) fn map(message: &Message) -> Option<Poll> {
    let tl::enums::Message::Message(message) = &message.raw else {
        return None;
    };
    let tl::enums::MessageMedia::Poll(media) = message.media.as_ref()? else {
        return None;
    };
    let tl::enums::Poll::Poll(raw) = &media.poll;
    let tl::enums::PollResults::Results(raw_results) = &media.results;
    let mut poll = Poll {
        definition: definition(raw, media.attached_media.is_some()),
        results: Results::default(),
        stale: false,
    };
    poll.apply(&Update {
        id: raw.id,
        stale: false,
        definition: None,
        results: results(raw_results),
    });
    Some(poll)
}

pub(super) fn update(raw: &tl::types::UpdateMessagePoll) -> Update {
    let tl::enums::PollResults::Results(raw_results) = &raw.results;
    Update {
        id: raw.poll_id,
        stale: false,
        definition: raw.poll.as_ref().map(|poll| {
            let tl::enums::Poll::Poll(poll) = poll;
            definition(poll, false)
        }),
        results: results(raw_results),
    }
}

pub(super) async fn load(client: &Client, peer: PeerRef, message_id: i32) -> Result<Poll> {
    ensure!(message_id > 0, "Select a delivered poll");
    let message = super::message_actions::fetch(client, peer, message_id).await?;
    map(&message).context("The message is no longer an available poll")
}

pub(super) async fn vote(
    client: &Client,
    peer: PeerRef,
    message_id: i32,
    poll_id: i64,
    revision: u64,
    options: &[Vec<u8>],
) -> Result<()> {
    let poll = load(client, peer, message_id).await?;
    ensure!(poll.definition.id == poll_id, "The poll changed; reopen it");
    poll.validate_vote(revision, options, chrono::Utc::now().timestamp())
        .map_err(anyhow::Error::msg)?;
    client
        .invoke(&tl::functions::messages::SendVote {
            peer: peer.into(),
            msg_id: message_id,
            options: options.to_vec(),
        })
        .await?;
    // The SDK puts this RPC's Updates into the existing ordered update stream.
    Ok(())
}

pub(super) async fn refresh(
    client: &Client,
    peer: PeerRef,
    message_id: i32,
    hash: i64,
) -> Result<()> {
    client
        .invoke(&tl::functions::messages::GetPollResults {
            peer: peer.into(),
            msg_id: message_id,
            poll_hash: hash,
        })
        .await?;
    Ok(())
}
