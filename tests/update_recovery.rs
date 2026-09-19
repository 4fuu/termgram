use grammers_client::tl;
use grammers_session::{
    types::{ChannelState, UpdatesState},
    updates::{MessageBoxes, UpdatesLike},
};

#[test]
fn startup_recovers_common_state_without_scanning_every_channel() {
    let state = UpdatesState {
        pts: 12,
        qts: 0,
        date: 123,
        seq: 1,
        channels: (1..=500).map(|id| ChannelState { id, pts: 20 }).collect(),
    };
    let boxes = MessageBoxes::load(state);
    assert!(boxes.get_difference().is_some());
    assert!(boxes.get_channel_difference().is_none());
    assert_eq!(boxes.session_state().channels.len(), 500);
}

#[test]
fn channel_gap_recovers_from_the_local_cursor() {
    let mut boxes = MessageBoxes::new();
    boxes.try_set_channel_state(42, 20);
    boxes
        .process_updates(UpdatesLike::Updates(tl::enums::Updates::UpdateShort(
            tl::types::UpdateShort {
                update: tl::types::UpdateChannelTooLong {
                    channel_id: 42,
                    pts: Some(30),
                }
                .into(),
                date: 123,
            },
        )))
        .unwrap();
    let request = boxes
        .get_channel_difference()
        .expect("channel gap must trigger recovery");
    assert_eq!(request.pts, 20);
}
