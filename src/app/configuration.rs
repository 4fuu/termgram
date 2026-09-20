//! One validated configuration snapshot is swapped between frames.
use super::App;
use crate::{keymap::Keymap, model::sanitize_terminal_line};
use std::{path::PathBuf, time::Instant};

#[derive(Clone, Default)]
pub struct State {
    pub path: Option<PathBuf>,
    pub loading: bool,
    pub revision: u64,
    pub error: Option<String>,
    pub loaded_at: Option<Instant>,
    pub terminal_clipboard: bool,
    pub status_scroll: usize,
    requested: bool,
}

impl App {
    pub(super) fn request_config_reload(&mut self) {
        if self.configuration.loading {
            return;
        }
        if self.configuration.path.is_none() {
            self.configuration_failed("No Lua configuration path is available");
            return;
        }
        self.configuration.requested = true;
        self.configuration.loading = true;
        self.status_message = Some("Reloading Lua configuration…".to_owned());
    }

    pub fn take_config_reload(&mut self) -> Option<PathBuf> {
        if !std::mem::take(&mut self.configuration.requested) {
            return None;
        }
        self.configuration.path.clone()
    }

    /// The runtime prepares terminal changes before installing this snapshot.
    pub fn install_configuration(&mut self, mut keymap: Keymap) {
        self.cancel_alerts();
        keymap.reset();
        self.keymap = keymap;
        self.metrics.latency = None;
        self.metrics.reset_at = Instant::now();
        self.help.scroll = 0;
        self.commands.invalidate_completion();
        self.clear_message_hit_regions();
        self.configuration.loading = false;
        self.configuration.requested = false;
        self.configuration.error = None;
        self.configuration.loaded_at = Some(Instant::now());
        self.configuration.revision += 1;
        self.force_redraw = true;
        self.status_message = Some("Lua configuration reloaded".to_owned());
    }

    pub fn configuration_failed(&mut self, error: &str) {
        let error = sanitize_terminal_line(error);
        self.configuration.loading = false;
        self.configuration.requested = false;
        self.configuration.error = Some(error.clone());
        self.status_message = Some(format!("Configuration unchanged: {error}"));
    }
}
