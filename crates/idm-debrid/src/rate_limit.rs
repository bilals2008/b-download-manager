//! How many requests the app may send to a service in a minute.
//!
//! Every service publishes a limit; a client that goes over it is shut out
//! for a while, and an account that is shut out often loses its access. The
//! app therefore keeps itself well under the published number, and stops
//! altogether for a while when a service says it has had enough.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The window the sent requests are counted in.
const WINDOW: Duration = Duration::from_secs(60);

/// The requests a minute the app allows itself to make to one service.
#[derive(Debug)]
pub struct RateLimiter {
    /// How many requests the window allows.
    limit: usize,
    /// How long a request counts.
    window: Duration,
    /// The sent requests, newest last.
    state: Mutex<State>,
}

/// What the limiter remembers between requests.
#[derive(Debug, Default)]
struct State {
    /// When each request was sent.
    sent: VecDeque<Instant>,
    /// Until when the service asked us to stop.
    paused_until: Option<Instant>,
}

impl RateLimiter {
    /// A limiter that lets `limit` requests through per minute.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            window: WINDOW,
            state: Mutex::new(State::default()),
        }
    }

    /// A limiter with a window of its own, for the tests that must not wait a
    /// minute for the window to pass.
    #[cfg(test)]
    fn with_window(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(State::default()),
        }
    }

    /// Take a request slot, waiting for one when the window is full.
    ///
    /// Returns how long the call had to wait, so the caller can say so.
    pub async fn acquire(&self) -> Duration {
        let start = Instant::now();
        while let Some(wait) = self.try_acquire() {
            tokio::time::sleep(wait).await;
        }
        start.elapsed()
    }

    /// Take a request slot if one is free, or say how long to wait for one.
    ///
    /// `None` means the request may go out now.
    pub fn try_acquire(&self) -> Option<Duration> {
        let now = Instant::now();
        let mut state = self.lock();
        if let Some(until) = state.paused_until {
            if until > now {
                return Some(until - now);
            }
            state.paused_until = None;
        }
        while state
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            state.sent.pop_front();
        }
        if state.sent.len() < self.limit {
            state.sent.push_back(now);
            return None;
        }
        Some(self.window - now.duration_since(state.sent[0]))
    }

    /// The service said "too many requests": nothing goes to it until then.
    pub fn pause(&self, wait: Duration) {
        let until = Instant::now() + wait;
        let mut state = self.lock();
        state.paused_until = Some(state.paused_until.map_or(until, |u| u.max(until)));
    }

    /// The lock, even when a thread died holding it: the budget is a
    /// convenience, not a reason to stop the downloads.
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_allows_only_its_share() {
        let limiter = RateLimiter::with_window(3, Duration::from_secs(60));
        assert!(limiter.try_acquire().is_none());
        assert!(limiter.try_acquire().is_none());
        assert!(limiter.try_acquire().is_none());
        let wait = limiter.try_acquire().expect("the fourth request waits");
        assert!(wait > Duration::from_secs(50), "{wait:?}");
    }

    #[tokio::test]
    async fn a_request_waits_for_the_window_to_pass() {
        let limiter = RateLimiter::with_window(1, Duration::from_millis(50));
        assert!(limiter.try_acquire().is_none());
        let waited = limiter.acquire().await;
        assert!(waited >= Duration::from_millis(40), "{waited:?}");
    }

    #[test]
    fn a_pause_holds_the_requests_back() {
        let limiter = RateLimiter::with_window(10, Duration::from_secs(60));
        limiter.pause(Duration::from_secs(30));
        let wait = limiter.try_acquire().expect("a paused limiter waits");
        assert!(wait > Duration::from_secs(25), "{wait:?}");
    }
}
