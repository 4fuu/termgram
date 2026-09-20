//! Optional observations on the existing primary connection. This task neither
//! consumes updates nor replaces Grammers' own keepalive/reconnection machinery.

use super::{Client, NetworkEvent, Session, SqliteSession, mpsc, tl};
use crate::statusline::Latency;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) async fn observe(
    client: Client,
    session: Arc<SqliteSession>,
    events: mpsc::Sender<NetworkEvent>,
    mut measure_latency: tokio::sync::watch::Receiver<bool>,
) {
    let mut ping_id = 0_i64;
    loop {
        let enabled = *measure_latency.borrow_and_update();
        let dc_id = session.home_dc_id().ok();
        // Publish DC without waiting for the optional measurement.
        if events
            .send(NetworkEvent::Telemetry {
                dc_id,
                latency: None,
            })
            .await
            .is_err()
        {
            return;
        }
        if !enabled {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(60)) => {},
                changed = measure_latency.changed() => { if changed.is_err() { return; } },
            }
            continue;
        }
        let started = Instant::now();
        ping_id = ping_id.wrapping_add(1);
        let request = tl::functions::Ping { ping_id };
        let invoke = client.invoke(&request);
        tokio::pin!(invoke);
        let result = tokio::time::timeout(Duration::from_secs(5), &mut invoke).await;
        let timed_out = result.is_err();
        let current_dc = session.home_dc_id().ok();
        let latency = (matches!(result, Ok(Ok(_))) && *measure_latency.borrow())
            .then_some(Latency {
                started,
                elapsed: started.elapsed(),
            })
            .filter(|_| dc_id.is_some() && dc_id == current_dc);
        if events
            .send(NetworkEvent::Telemetry {
                dc_id: current_dc,
                latency,
            })
            .await
            .is_err()
        {
            return;
        }
        // Grammers cannot cancel an already-enqueued RPC when its future is
        // dropped. Keep one probe alive after reporting it unavailable instead
        // of accumulating requests during a half-open/offline connection.
        // Worker shutdown owns both this task and the underlying sender pool.
        if timed_out {
            let _ = invoke.await;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(60)) => {},
            changed = measure_latency.changed() => { if changed.is_err() { return; } },
        }
    }
}
