//! One bounded native writer, retaining arboard's Linux clipboard owner.
//! Terminal output stays with the existing main-thread broker.
use std::sync::mpsc::{SyncSender, sync_channel};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

pub(crate) const MAX_TEXT: usize = 100_000;

pub(crate) struct Job {
    pub account: (u8, Option<i64>),
    pub text: String,
    pub remote: bool,
    pub multiplexed: bool,
}

pub struct Completion {
    pub(crate) account: (u8, Option<i64>),
    pub(crate) terminal: Option<String>,
    pub(crate) native: Result<(), String>,
}

pub(crate) struct Writer {
    sender: SyncSender<Job>,
    receiver: UnboundedReceiver<Completion>,
    busy: bool,
}

impl Writer {
    pub fn start() -> std::io::Result<Self> {
        let (sender, receiver) = sync_channel::<Job>(1);
        let (completed, results) = unbounded_channel();
        std::thread::Builder::new()
            .name("clipboard-copy".to_owned())
            .spawn(move || {
                // On X11 and some Wayland compositors, dropping this handle makes
                // copied content disappear. Keep it across operations/accounts.
                let mut owner: Option<arboard::Clipboard> = None;
                while let Ok(job) = receiver.recv() {
                    let native = if job.remote {
                        Err("Remote sessions use the attached terminal clipboard".to_owned())
                    } else {
                        native_copy(&mut owner, &job.text).or_else(|error| {
                            if cfg!(target_os = "linux") && super::wsl::available() {
                                super::wsl::copy(&job.text)
                                    .map_err(|fallback| format!("{error}; WSL: {fallback:#}"))
                            } else {
                                Err(error)
                            }
                        })
                    };
                    let terminal =
                        (job.remote || job.multiplexed || native.is_err()).then_some(job.text);
                    if completed
                        .send(Completion {
                            account: job.account,
                            terminal,
                            native,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender,
            receiver: results,
            busy: false,
        })
    }

    pub fn submit(&mut self, job: Job) -> Result<(), String> {
        if self.busy {
            return Err("A clipboard write is still in progress".to_owned());
        }
        if job.text.is_empty() {
            return Err("Nothing to copy".to_owned());
        }
        if job.text.len() > MAX_TEXT {
            return Err("Clipboard text exceeds 100,000 bytes".to_owned());
        }
        self.sender
            .try_send(job)
            .map_err(|_| "Clipboard writer unavailable".to_owned())?;
        self.busy = true;
        Ok(())
    }

    pub async fn receive(&mut self) -> Completion {
        if !self.busy {
            return std::future::pending().await;
        }
        let result = self.receiver.recv().await.unwrap_or_else(|| Completion {
            account: (0, None),
            terminal: None,
            native: Err("Clipboard writer stopped".to_owned()),
        });
        self.busy = false;
        result
    }
}

fn native_copy(owner: &mut Option<arboard::Clipboard>, text: &str) -> Result<(), String> {
    let _access = super::NATIVE_ACCESS
        .lock()
        .map_err(|_| "Native clipboard lock failed".to_owned())?;
    if owner.is_none() {
        *owner = Some(arboard::Clipboard::new().map_err(|error| error.to_string())?);
    }
    owner
        .as_mut()
        .expect("initialized clipboard")
        .set_text(text)
        .map_err(|error| error.to_string())
}
