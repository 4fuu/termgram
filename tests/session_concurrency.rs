//! Separate OS processes exercise SQLite locking and simultaneous first-open.
use std::process::{Command, Stdio};

use grammers_session::{Session, storages::SqliteSession, types::UpdateState};

#[test]
fn concurrent_session_processes() {
    let directory = std::env::temp_dir().join(format!("termgram-session-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("shared.session");
    let children = (0..4)
        .map(|_| {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "session_writer", "--nocapture"])
                .env("TERMGRAM_TEST_SESSION", &path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    // Reap every child and retain its diagnostics, including native exit codes.
    let failures = children
        .into_iter()
        .enumerate()
        .filter_map(|(index, child)| {
            let output = child.wait_with_output().unwrap();
            (!output.status.success()).then(|| {
                format!(
                    "writer {index}: {}\nstdout:\n{}\nstderr:\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )
            })
        })
        .collect::<Vec<_>>();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        failures.is_empty(),
        "all processes must initialize and update the shared session:\n{}",
        failures.join("\n")
    );
}

#[test]
fn session_writer() {
    let Some(path) = std::env::var_os("TERMGRAM_TEST_SESSION") else {
        return;
    };
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let session = SqliteSession::open(path).await.unwrap();
            // Exercise initialization and repeated writes, not sustained writer
            // saturation: the production busy timeout deliberately caps waiting.
            for pts in 1..=3 {
                session
                    .set_update_state(UpdateState::Primary {
                        pts,
                        date: pts,
                        seq: pts,
                    })
                    .await
                    .unwrap();
                session.set_home_dc_id(2).await.unwrap();
                assert!(session.updates_state().await.unwrap().pts > 0);
            }
        });
}
