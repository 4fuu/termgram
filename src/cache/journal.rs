//! Bounded patches retained only while message snapshots are in flight.
use std::collections::VecDeque;

pub(super) struct Journal<T> {
    updates: VecDeque<(i64, T)>,
    overflow: i64,
}

impl<T> Default for Journal<T> {
    fn default() -> Self {
        Self {
            updates: VecDeque::new(),
            overflow: 0,
        }
    }
}

impl<T> Journal<T> {
    pub fn record(&mut self, revision: i64, update: T) {
        self.updates.push_back((revision, update));
        if self.updates.len() > 512 {
            self.overflow = self.updates.pop_front().expect("over limit").0;
        }
    }

    pub fn since(&self, started: i64) -> impl Iterator<Item = &T> {
        self.updates
            .iter()
            .filter(move |(revision, _)| *revision > started)
            .map(|(_, update)| update)
    }

    pub fn stale(&self, started: i64) -> bool {
        started < self.overflow
    }

    pub fn prune(&mut self, oldest: i64) {
        self.updates.retain(|(revision, _)| *revision > oldest);
        if oldest >= self.overflow {
            self.overflow = 0;
        }
    }
}
