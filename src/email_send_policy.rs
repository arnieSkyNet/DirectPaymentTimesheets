//! Accidental-abuse safeguards, not a boundary against a modified local client.
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
pub const MAX_BATCH: usize = 50;
pub const LARGE_BATCH: usize = 10;
pub fn validate_batch(count: usize, acknowledged: bool) -> Result<(), &'static str> {
    if count > MAX_BATCH {
        return Err("Maximum 50 messages per batch; select a smaller batch.");
    }
    if count > LARGE_BATCH && !acknowledged {
        return Err("Explicitly confirm this batch of more than 10 messages.");
    }
    Ok(())
}
#[derive(Default)]
struct Limits {
    sends: VecDeque<Instant>,
    last_test: Option<Instant>,
}
impl Limits {
    fn claim(&mut self, now: Instant, test: bool) -> Result<(), &'static str> {
        while self
            .sends
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= Duration::from_secs(600))
        {
            self.sends.pop_front();
        }
        if self.sends.len() >= 100 {
            return Err("Email safety limit reached: 100 attempts per 10 minutes. Wait before sending again.");
        }
        if test
            && self
                .last_test
                .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(30))
        {
            return Err("Wait 30 seconds between test email attempts.");
        }
        self.sends.push_back(now);
        if test {
            self.last_test = Some(now);
        }
        Ok(())
    }
}
#[cfg(not(test))]
pub fn claim(test: bool) -> Result<(), &'static str> {
    static LIMITS: OnceLock<Mutex<Limits>> = OnceLock::new();
    LIMITS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Email safety limiter unavailable.")?
        .claim(Instant::now(), test)
}
#[cfg(test)]
thread_local! {static TEST_LIMITS: std::cell::RefCell<Option<Limits>>=const {std::cell::RefCell::new(None)};}
#[cfg(test)]
pub fn claim(test: bool) -> Result<(), &'static str> {
    TEST_LIMITS.with(|limits| match limits.borrow_mut().as_mut() {
        Some(limits) => limits.claim(Instant::now(), test),
        None => Ok(()),
    })
}
#[cfg(test)]
pub(crate) fn with_test_limits(run: impl FnOnce()) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_LIMITS.with(|l| *l.borrow_mut() = None);
        }
    }
    TEST_LIMITS.with(|l| *l.borrow_mut() = Some(Limits::default()));
    let _reset = Reset;
    run();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_boundaries_and_rejected_attempts_do_not_extend_cooldown() {
        let t = Instant::now();
        let mut limits = Limits::default();
        limits.claim(t, true).unwrap();
        assert!(limits
            .claim(t + Duration::from_millis(29999), true)
            .is_err());
        assert_eq!(limits.sends.len(), 1);
        limits.claim(t + Duration::from_secs(30), true).unwrap();
        assert_eq!(limits.sends.len(), 2);
        for n in [0, 1, 10] {
            assert!(validate_batch(n, false).is_ok());
        }
        for n in [11, 50] {
            assert!(validate_batch(n, false).is_err());
            assert!(validate_batch(n, true).is_ok());
        }
        assert!(validate_batch(51, true).is_err());
        let mut full = Limits::default();
        for _ in 0..100 {
            full.claim(t, false).unwrap();
        }
        assert!(full.claim(t + Duration::from_secs(599), false).is_err());
        full.claim(t + Duration::from_secs(600), false).unwrap();
    }
    #[test]
    fn cooldown_volume_and_expiry() {
        let t = Instant::now();
        let mut l = Limits::default();
        l.claim(t, true).unwrap();
        assert!(l.claim(t, true).is_err());
        l.claim(t + Duration::from_secs(30), true).unwrap();
        for _ in 2..100 {
            l.claim(t + Duration::from_secs(30), false).unwrap();
        }
        assert!(l.claim(t + Duration::from_secs(30), false).is_err());
        l.claim(t + Duration::from_secs(631), true).unwrap();
        assert!(validate_batch(11, false).is_err());
        assert!(validate_batch(11, true).is_ok());
        assert!(validate_batch(51, true).is_err());
    }
}
