//! In-memory admission/GUI latch only. No journal, recovery I/O or termination.
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Failure {
    NativeOpenUnconfirmed = 1,
    DestructorCloseUnconfirmed = 2,
    BoundaryCloseUnconfirmed = 3,
    CloseUnwound = 4,
}

impl Failure {
    pub(crate) fn message(self) -> &'static str {
        "Database closure or ownership could not be confirmed. Further work is disabled. Close DirectPaymentTimesheets normally, preserve the application data and backups, and restart before retrying. If the database is damaged, restore a known-good backup with assistance; do not reset or delete the damaged data."
    }
}

struct State(AtomicU8);
impl State {
    const fn new() -> Self {
        Self(AtomicU8::new(0))
    }

    fn record(&self, failure: Failure) {
        // First observation wins. No diagnostics, allocations or unwinding.
        let _ = self
            .0
            .compare_exchange(0, failure as u8, Ordering::AcqRel, Ordering::Acquire);
    }

    fn current(&self) -> Option<Failure> {
        match self.0.load(Ordering::Acquire) {
            0 => None,
            1 => Some(Failure::NativeOpenUnconfirmed),
            2 => Some(Failure::DestructorCloseUnconfirmed),
            3 => Some(Failure::BoundaryCloseUnconfirmed),
            _ => Some(Failure::CloseUnwound),
        }
    }
}

static STATE: State = State::new();

pub(crate) fn current() -> Option<Failure> {
    STATE.current()
}

pub(crate) fn record(failure: Failure) {
    STATE.record(failure);
}

/// A panic during close must not leave the GUI believing ownership is certain.
/// Completion includes a returned error: explicit close may still be retried by
/// its caller while it retains the entire owner. No panic is caught or resumed.
pub(crate) struct OwnershipAttempt<'a> {
    state: &'a State,
    completed: bool,
    failure: Failure,
}

impl OwnershipAttempt<'static> {
    pub(crate) fn new(failure: Failure) -> Self {
        Self {
            state: &STATE,
            completed: false,
            failure,
        }
    }
}

impl OwnershipAttempt<'_> {
    pub(crate) fn completed(&mut self) {
        self.completed = true;
    }
}

impl Drop for OwnershipAttempt<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.state.record(self.failure);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_failure_is_monotonic_and_shared_across_threads() {
        let state = std::sync::Arc::new(State::new());
        assert!(state.current().is_none());
        state.record(Failure::BoundaryCloseUnconfirmed);
        let observer = state.clone();
        std::thread::spawn(move || {
            assert_eq!(observer.current(), Some(Failure::BoundaryCloseUnconfirmed));
            observer.record(Failure::NativeOpenUnconfirmed);
        })
        .join()
        .unwrap();
        assert_eq!(state.current(), Some(Failure::BoundaryCloseUnconfirmed));
    }

    #[test]
    fn completed_close_attempt_does_not_latch_failure() {
        let state = State::new();
        let mut attempt = OwnershipAttempt {
            state: &state,
            completed: false,
            failure: Failure::CloseUnwound,
        };
        attempt.completed();
        drop(attempt);
        assert!(state.current().is_none());
    }

    #[test]
    fn ordinary_rust_unwind_latches_without_native_execution() {
        let state = State::new();
        let result = std::panic::catch_unwind(|| {
            let _attempt = OwnershipAttempt {
                state: &state,
                completed: false,
                failure: Failure::CloseUnwound,
            };
            panic!("synthetic Rust unwind; no SQLite call");
        });
        assert!(result.is_err());
        assert_eq!(state.current(), Some(Failure::CloseUnwound));
    }

    #[test]
    fn messages_contain_no_runtime_error_payload() {
        for failure in [
            Failure::NativeOpenUnconfirmed,
            Failure::DestructorCloseUnconfirmed,
            Failure::BoundaryCloseUnconfirmed,
            Failure::CloseUnwound,
        ] {
            assert!(failure.message().contains("known-good backup"));
            assert!(!failure.message().contains("SQLITE_"));
        }
    }
}
