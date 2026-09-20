//! Bounded native delivery and terminal fallbacks. All TTY writes stay on the
//! main event-loop thread; the platform worker never touches the terminal.
use super::{Alert, Backend};
use crate::app::AppState;
use std::{
    io::Write,
    sync::{atomic::Ordering, mpsc},
    time::{Duration, Instant},
};
use yazi_emulator::{Brand, EMULATOR};
use yazi_tty::{TTY, sequence::TmuxPassthrough};

struct Job {
    alert: Alert,
    fallback: bool,
}
pub struct Completion {
    job: Job,
    result: Result<(), String>,
}
pub enum Activity {
    Deadline,
    Delivered(Box<Completion>),
}

pub struct Dispatcher {
    jobs: Option<mpsc::SyncSender<Job>>,
    done: tokio::sync::mpsc::Sender<Completion>,
    results: tokio::sync::mpsc::Receiver<Completion>,
    remote: bool,
    last_error: Option<Instant>,
}

impl Default for Dispatcher {
    fn default() -> Self {
        let (done, results) = tokio::sync::mpsc::channel(8);
        Self {
            jobs: None,
            done,
            results,
            remote: std::env::var_os("SSH_CONNECTION").is_some()
                || std::env::var_os("SSH_TTY").is_some(),
            last_error: None,
        }
    }
}

impl Dispatcher {
    pub fn flush(&mut self, app: &mut AppState) {
        let backend = app.keymap.notifications.backend;
        for alert in app.take_alerts() {
            let terminal = match backend {
                Backend::Osc9 => Some(true),
                Backend::Bell => Some(false),
                Backend::Auto if self.remote => Some(supports_osc9()),
                Backend::Auto | Backend::Native => None,
            };
            if let Some(osc9) = terminal {
                if let Err(error) = post_terminal(&alert, osc9) {
                    self.error(app, &error.to_string());
                }
                app.alert_finished(&alert);
                continue;
            }
            let fallback = backend == Backend::Auto;
            match self.worker() {
                Ok(sender) => match sender.try_send(Job { alert, fallback }) {
                    Ok(()) => {}
                    Err(error) => {
                        let job = match error {
                            mpsc::TrySendError::Full(job)
                            | mpsc::TrySendError::Disconnected(job) => job,
                        };
                        app.alert_finished(&job.alert);
                        self.error(app, "notification queue is unavailable");
                    }
                },
                Err(error) => {
                    if fallback {
                        let _ = post_terminal(&alert, supports_osc9());
                    }
                    app.alert_finished(&alert);
                    self.error(app, &error.to_string());
                }
            }
        }
    }

    fn worker(&mut self) -> std::io::Result<&mpsc::SyncSender<Job>> {
        if self.jobs.is_none() {
            let (sender, receiver) = mpsc::sync_channel::<Job>(8);
            let done = self.done.clone();
            std::thread::Builder::new()
                .name("termgram-notifications".to_owned())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        let result = if job.alert.valid.load(Ordering::Acquire)
                            && job.alert.expires > Instant::now()
                        {
                            post_native(&job.alert).map_err(|error| error.to_string())
                        } else {
                            Ok(())
                        };
                        if done.blocking_send(Completion { job, result }).is_err() {
                            break;
                        }
                    }
                })?;
            self.jobs = Some(sender);
        }
        Ok(self.jobs.as_ref().expect("notification worker started"))
    }

    pub async fn next_activity(&mut self, deadline: Option<Instant>) -> Activity {
        let timer = async {
            if let Some(deadline) = deadline {
                tokio::time::sleep_until(deadline.into()).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            () = timer => Activity::Deadline,
            result = self.results.recv() => match result {
                Some(result) => Activity::Delivered(Box::new(result)),
                None => std::future::pending().await,
            }
        }
    }

    pub fn activity(&mut self, activity: Activity, app: &mut AppState) {
        let Activity::Delivered(completion) = activity else {
            return;
        };
        let Completion { job, result } = *completion;
        if job.alert.valid.load(Ordering::Acquire)
            && app.account_user_id == Some(job.alert.account)
            && let Err(error) = result
        {
            if job.fallback
                && app.keymap.notifications.backend == Backend::Auto
                && let Err(fallback) = post_terminal(&job.alert, supports_osc9())
            {
                self.error(app, &fallback.to_string());
            }
            self.error(app, &error);
        }
        app.alert_finished(&job.alert);
    }

    fn error(&mut self, app: &mut AppState, error: &str) {
        let now = Instant::now();
        if self
            .last_error
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(60))
        {
            self.last_error = Some(now);
            app.status_message = Some(format!(
                "Desktop notification: {}",
                crate::model::sanitize_terminal_line(error)
            ));
        }
    }
}

// Same terminal capability allowlist as Codex; detection remains Yazi's owner.
fn supports_osc9() -> bool {
    matches!(
        EMULATOR.brand.get(),
        Brand::Ghostty | Brand::Iterm2 | Brand::Kitty | Brand::Warp | Brand::WezTerm
    )
}

fn post_terminal(alert: &Alert, osc9: bool) -> std::io::Result<()> {
    // Neither OSC 9 nor BEL can request a silent alert. Preserve Telegram's
    // silent-message and sound-none settings rather than sounding a fallback.
    if !alert.sound || !alert.valid.load(Ordering::Acquire) || alert.expires <= Instant::now() {
        return Ok(());
    }
    let mut tty = TTY.writer();
    if osc9 {
        let sequence = format!("\x1b]9;{} · {}\x07", alert.title, alert.body);
        write!(
            tty,
            "{}",
            TmuxPassthrough(sequence, EMULATOR.mux.get().is_some())
        )?;
    } else {
        tty.write_all(b"\x07")?;
    }
    tty.flush()
}

fn post_native(alert: &Alert) -> anyhow::Result<()> {
    let mut notification = notify_rust::Notification::new();
    notification
        .appname("Termgram")
        .summary(&alert.title)
        .timeout(8000);
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // XDG bodies support markup; message text stays literal.
        notification.body(&quick_xml::escape::partial_escape(alert.body.as_str()));
        notification.hint(notify_rust::Hint::Category("im.received".to_owned()));
        notification.hint(notify_rust::Hint::SuppressSound(!alert.sound));
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    notification.body(&alert.body);
    #[cfg(target_os = "macos")]
    {
        if alert.sound {
            notification.sound_name("default");
        }
        // Legacy show() defers to Drop and hides delivery errors. schedule_raw
        // sends immediately, returns errors, and leaves an empty handle.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs_f64();
        notification.schedule_raw(now)?;
    }
    #[cfg(target_os = "windows")]
    {
        if alert.sound {
            notification.sound_name("Default");
        }
        notification.show()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    notification.show()?;
    Ok(())
}
