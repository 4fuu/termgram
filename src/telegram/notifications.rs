use anyhow::{Result, ensure};
use grammers_client::{Client, peer::Peer, tl};
use grammers_session::types::PeerRef;

pub(super) async fn set_mute(
    client: &Client,
    peer: PeerRef,
    mute: crate::notifications::Mute,
) -> Result<i64> {
    let target = tl::enums::InputNotifyPeer::Peer(tl::types::InputNotifyPeer { peer: peer.into() });
    let tl::enums::PeerNotifySettings::Settings(current) = client
        .invoke(&tl::functions::account::GetNotifySettings {
            peer: target.clone(),
        })
        .await?;
    let settings = with_mute(current, mute.until(chrono::Utc::now().timestamp()));
    ensure!(
        client
            .invoke(&tl::functions::account::UpdateNotifySettings {
                peer: target.clone(),
                settings: settings.into()
            })
            .await?,
        "Telegram did not accept the notification setting"
    );
    let tl::enums::PeerNotifySettings::Settings(current) = client
        .invoke(&tl::functions::account::GetNotifySettings { peer: target })
        .await?;
    if let Some(until) = current.mute_until {
        return Ok(i64::from(until));
    }
    // Another client may have restored inheritance while this request ran.
    let scope = match client.resolve_peer(peer).await? {
        Peer::User(_) => tl::enums::InputNotifyPeer::InputNotifyUsers,
        Peer::Group(_) => tl::enums::InputNotifyPeer::InputNotifyChats,
        Peer::Channel(_) => tl::enums::InputNotifyPeer::InputNotifyBroadcasts,
    };
    let tl::enums::PeerNotifySettings::Settings(defaults) = client
        .invoke(&tl::functions::account::GetNotifySettings { peer: scope })
        .await?;
    Ok(i64::from(defaults.mute_until.unwrap_or(0)))
}

fn with_mute(
    current: tl::types::PeerNotifySettings,
    until: i32,
) -> tl::types::InputPeerNotifySettings {
    // Desktop serializes the whole peer override. Preserve preview, sounds,
    // silent-post and story preferences instead of resetting them on mute.
    tl::types::InputPeerNotifySettings {
        show_previews: current.show_previews,
        silent: current.silent,
        mute_until: Some(until),
        sound: current.other_sound,
        stories_muted: current.stories_muted,
        stories_hide_sender: current.stories_hide_sender,
        stories_sound: current.stories_other_sound,
    }
}

pub(super) fn requires_playback(message: &grammers_client::message::Message) -> bool {
    let tl::enums::Message::Message(message) = &message.raw else {
        return false;
    };
    match &message.media {
        Some(tl::enums::MessageMedia::Photo(photo)) => photo.ttl_seconds.is_some(),
        Some(tl::enums::MessageMedia::Document(media)) => {
            media.ttl_seconds.is_some()
                || match &media.document {
                    Some(tl::enums::Document::Document(document)) => {
                        document.attributes.iter().any(|attribute| match attribute {
                            tl::enums::DocumentAttribute::Audio(audio) => audio.voice,
                            tl::enums::DocumentAttribute::Video(video) => video.round_message,
                            _ => false,
                        })
                    }
                    _ => false,
                }
        }
        _ => false,
    }
}

pub(super) async fn read_mentions(client: &Client, peer: PeerRef, ids: &[i32]) -> Result<()> {
    ensure!(
        !ids.is_empty() && ids.len() <= 100 && ids.iter().all(|id| *id > 0),
        "Mention receipts require 1–100 delivered message IDs"
    );
    if peer.id.kind() == grammers_session::types::PeerKind::Channel {
        ensure!(
            client
                .invoke(&tl::functions::channels::ReadMessageContents {
                    channel: peer.into(),
                    id: ids.to_vec()
                })
                .await?,
            "Telegram did not acknowledge the mentions"
        );
    } else {
        client
            .invoke(&tl::functions::messages::ReadMessageContents { id: ids.to_vec() })
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_mute_preserves_other_notification_overrides() {
        let old = tl::types::PeerNotifySettings {
            show_previews: Some(false),
            silent: Some(true),
            mute_until: Some(0),
            ios_sound: None,
            android_sound: None,
            other_sound: Some(tl::enums::NotificationSound::Ringtone(
                tl::types::NotificationSoundRingtone { id: 123 },
            )),
            stories_muted: Some(true),
            stories_hide_sender: Some(true),
            stories_ios_sound: None,
            stories_android_sound: None,
            stories_other_sound: Some(tl::enums::NotificationSound::None),
        };
        let muted = with_mute(old.clone(), crate::notifications::Mute::Hour.until(1000));
        assert_eq!(muted.mute_until, Some(4600));
        assert_eq!(muted.show_previews, Some(false));
        assert_eq!(muted.silent, Some(true));
        assert_eq!(muted.sound, old.other_sound);
        assert_eq!(muted.stories_sound, old.stories_other_sound);
        assert_eq!(muted.stories_hide_sender, Some(true));
        assert_eq!(crate::notifications::Mute::Forever.until(1000), i32::MAX);
        assert_eq!(crate::notifications::Mute::Off.until(1000), 0);
    }
}
