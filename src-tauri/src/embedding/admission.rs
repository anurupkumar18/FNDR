//! Admission to one shared text-model backend call. Waiting does not cancel work.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmbeddingPriority {
    Foreground,
    Background,
}

#[derive(Default)]
pub(super) struct AdmissionGate {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    foreground: VecDeque<u64>,
    background: VecDeque<u64>,
    next_ticket: u64,
    active: bool,
    foreground_streak: usize,
}

impl State {
    fn next(&self) -> Option<(EmbeddingPriority, u64)> {
        if self.active {
            return None;
        }
        // Prefer foreground work, but let a waiting background call through
        // after four foreground admissions. Queue fronts preserve class FIFO.
        match (self.foreground.front(), self.background.front()) {
            (Some(&foreground), background)
                if background.is_none() || self.foreground_streak < 4 =>
            {
                Some((EmbeddingPriority::Foreground, foreground))
            }
            (_, Some(&background)) => Some((EmbeddingPriority::Background, background)),
            _ => None,
        }
    }

    fn start(&mut self, priority: EmbeddingPriority) {
        match priority {
            EmbeddingPriority::Foreground => {
                self.foreground.pop_front();
                self.foreground_streak = (self.foreground_streak + 1).min(4);
            }
            EmbeddingPriority::Background => {
                self.background.pop_front();
                self.foreground_streak = 0;
            }
        }
        self.active = true;
    }
}

impl AdmissionGate {
    pub(super) fn enter(&self, priority: EmbeddingPriority) -> Result<Permit<'_>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Embedding admission lock poisoned")?;
        let ticket = state.next_ticket;
        state.next_ticket = ticket
            .checked_add(1)
            .ok_or("Embedding admission tickets exhausted")?;
        match priority {
            EmbeddingPriority::Foreground => state.foreground.push_back(ticket),
            EmbeddingPriority::Background => state.background.push_back(ticket),
        }
        while state.next() != Some((priority, ticket)) {
            state = self
                .changed
                .wait(state)
                .map_err(|_| "Embedding admission lock poisoned")?;
        }
        state.start(priority);
        Ok(Permit { gate: self })
    }
}

pub(super) struct Permit<'a> {
    gate: &'a AdmissionGate,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.active = false;
        self.gate.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Barrier,
    };
    use std::time::Duration;

    #[test]
    fn foreground_passes_an_older_waiting_background_request() {
        let state = State {
            foreground: VecDeque::from([1, 2]),
            background: VecDeque::from([0]),
            ..Default::default()
        };
        assert_eq!(state.next(), Some((EmbeddingPriority::Foreground, 1)));
    }

    #[test]
    fn waiting_background_runs_after_four_foreground_admissions() {
        let mut state = State {
            foreground: VecDeque::from([0, 1, 2, 3, 4, 5]),
            background: VecDeque::from([6, 7]),
            ..Default::default()
        };
        for ticket in 0..4 {
            assert_eq!(state.next(), Some((EmbeddingPriority::Foreground, ticket)));
            state.start(EmbeddingPriority::Foreground);
            assert_eq!(state.next(), None, "one active backend call");
            state.active = false;
        }
        assert_eq!(state.next(), Some((EmbeddingPriority::Background, 6)));
        state.start(EmbeddingPriority::Background);
        state.active = false;
        assert_eq!(state.next(), Some((EmbeddingPriority::Foreground, 4)));
    }

    #[test]
    fn each_priority_keeps_fifo_order_and_runs_without_the_other() {
        for priority in [EmbeddingPriority::Foreground, EmbeddingPriority::Background] {
            let mut state = State::default();
            match priority {
                EmbeddingPriority::Foreground => state.foreground = VecDeque::from([7, 8, 9]),
                EmbeddingPriority::Background => state.background = VecDeque::from([7, 8, 9]),
            }
            for ticket in [7, 8, 9] {
                assert_eq!(state.next(), Some((priority, ticket)));
                state.start(priority);
                state.active = false;
            }
            assert_eq!(state.next(), None);
        }
    }

    #[test]
    fn threads_never_hold_two_backend_permits() {
        let gate = Arc::new(AdmissionGate::default());
        let active = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(7));
        let (entered_tx, entered_rx) = mpsc::channel();
        let mut releases = Vec::new();
        let mut handles = Vec::new();
        for id in 0..6 {
            let gate = gate.clone();
            let active = active.clone();
            let barrier = barrier.clone();
            let entered = entered_tx.clone();
            let (release_tx, release_rx) = mpsc::channel();
            releases.push(release_tx);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let priority = if id % 2 == 0 {
                    EmbeddingPriority::Foreground
                } else {
                    EmbeddingPriority::Background
                };
                let _permit = gate.enter(priority).unwrap();
                assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
                entered.send(id).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(active.fetch_sub(1, Ordering::SeqCst), 1);
            }));
        }
        drop(entered_tx);
        barrier.wait();
        for _ in 0..6 {
            let id = entered_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("next backend permit");
            assert_eq!(active.load(Ordering::SeqCst), 1);
            releases[id].send(()).unwrap();
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn result_error_releases_the_next_waiter() {
        let gate = Arc::new(AdmissionGate::default());
        let held = gate.enter(EmbeddingPriority::Foreground).unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        let worker_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let _permit = worker_gate.enter(EmbeddingPriority::Background)?;
                Err("synthetic backend error".into())
            })();
            assert_eq!(result.unwrap_err(), "synthetic backend error");
            let _next = worker_gate.enter(EmbeddingPriority::Foreground).unwrap();
            done_tx.send(()).unwrap();
        });
        drop(held);
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("permit released after error");
        worker.join().unwrap();
    }
}
