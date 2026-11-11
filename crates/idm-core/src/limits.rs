//! Limits on how much of the network the engine may use.
//!
//! The reference download manager reads a global connection cap from
//! `HKCU\Software\DownloadManager\MCN` (`MaxConnectionsNumber`, observed
//! default 8) and keeps per-site overrides in the same key, as subkeys with
//! a `number` value. When the cap is hit it tells the user: "Too many open
//! network connections. Please wait for some downloads to finish."
//!
//! The evidence for the cap and for its wording is the string the reference
//! product shows; `planning/09-remaining-work.md` tracks what the clone still
//! does not do with it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// The number of connections the whole application may have open at once.
#[derive(Debug, Clone)]
pub struct ConnectionBudget {
    permits: Arc<Semaphore>,
    /// How many permits the budget was last set to.
    ///
    /// The semaphore only knows how many permits are free, so the total is
    /// kept here. Resizing against the free count added the permits in use on
    /// top of the new cap, so the cap grew every time the settings were
    /// saved, and the application ended up with far more connections than the
    /// reader asked for.
    total: Arc<AtomicUsize>,
}

impl ConnectionBudget {
    pub fn new(max: u32) -> Self {
        let max = max.max(1) as usize;
        Self {
            permits: Arc::new(Semaphore::new(max)),
            total: Arc::new(AtomicUsize::new(max)),
        }
    }

    /// Take a connection slot, or `None` when the budget is used up.
    #[cfg(test)]
    pub fn try_take(&self) -> Option<OwnedSemaphorePermit> {
        self.permits.clone().try_acquire_owned().ok()
    }

    /// Take a connection slot, waiting for one to come free.
    ///
    /// This is what a download uses: the reference download manager says "Too
    /// many open network connections. Please wait for some downloads to
    /// finish." rather than failing the download.
    pub async fn acquire(&self) -> Option<OwnedSemaphorePermit> {
        self.permits.clone().acquire_owned().await.ok()
    }

    /// Change how many connections the application may have open at once.
    ///
    /// The settings window owns this number, so the cap can be lowered and
    /// raised while the application runs.
    pub fn resize(&self, max: u32) {
        let target = max.max(1) as usize;
        let current = self.total.swap(target, Ordering::SeqCst);
        if target > current {
            self.permits.add_permits(target - current);
        } else if target < current {
            self.permits.forget_permits(current - target);
        }
    }
}

/// Per-site connection overrides, keyed by host.
#[derive(Debug, Default, Clone)]
pub struct SiteLimits {
    hosts: Arc<Mutex<HashMap<String, u32>>>,
}

impl SiteLimits {
    pub fn set(&self, host: &str, connections: u32) {
        self.hosts
            .lock()
            .expect("hosts lock poisoned")
            .insert(host.to_ascii_lowercase(), connections.max(1));
    }

    /// The override for a host, when there is one.
    pub fn get(&self, host: &str) -> Option<u32> {
        self.hosts
            .lock()
            .expect("hosts lock poisoned")
            .get(&host.to_ascii_lowercase())
            .copied()
    }

    /// The connection count to use for a host: its override, else the default.
    pub fn resolve(&self, host: &str, default: u32) -> u32 {
        self.get(host).unwrap_or(default).max(1)
    }
}

/// The host of a URL, for the per-site lookup.
pub fn host_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    parsed.host_str().map(|host| host.to_ascii_lowercase())
}

/// What one host allowed the last time it was asked.
///
/// The reference manager keeps a per-site number in the registry; this is the
/// same idea, written by the server's own answer rather than by the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedCap {
    /// The most connections the host honoured.
    pub connections: u32,
    /// When it was last seen, in seconds since the epoch.
    pub seen_at: u64,
}

/// How many days a learned cap is trusted.
///
/// A host that was capped years ago may have been replaced; a cap that lives
/// forever would keep a fixed server slow forever.
pub const LEARNED_CAP_DAYS: u64 = 30;

/// The caps learned from what each server actually allowed.
///
/// A host that answered two ranges and refused the rest is asked for two the
/// next time, instead of being asked for eight and failing eight times. The
/// user's own setting stays the ceiling: learning can only lower what a host
/// gets, never raise it past what the reader asked for.
#[derive(Debug, Clone, Default)]
pub struct LearnedCaps {
    hosts: Arc<Mutex<HashMap<String, LearnedCap>>>,
    /// Where the caps are kept between runs, when they are kept at all.
    path: Arc<Mutex<Option<PathBuf>>>,
}

impl LearnedCaps {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep the caps in a file, and read what is there already.
    ///
    /// A file that cannot be read is not an error: the caps are an
    /// optimisation, and a first run has none.
    pub fn open(&self, path: &Path) {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(hosts) = serde_json::from_str::<HashMap<String, LearnedCap>>(&text) {
                *self.hosts.lock().expect("learned caps lock poisoned") = hosts;
            }
        }
        *self.path.lock().expect("learned path lock poisoned") = Some(path.to_path_buf());
    }

    /// The cap learned for a host, when it is still fresh.
    pub fn get(&self, host: &str) -> Option<u32> {
        let key = host.to_ascii_lowercase();
        let now = now_seconds();
        let mut hosts = self.hosts.lock().expect("learned caps lock poisoned");
        let cap = hosts.get(&key).copied()?;
        if now.saturating_sub(cap.seen_at) > LEARNED_CAP_DAYS * 24 * 60 * 60 {
            hosts.remove(&key);
            return None;
        }
        Some(cap.connections.max(1))
    }

    /// The number of connections to start with for a host.
    ///
    /// Never more than the user asked for, at least one.
    pub fn resolve(&self, host: &str, asked: u32) -> u32 {
        let asked = asked.max(1);
        self.get(host).map(|cap| cap.min(asked)).unwrap_or(asked)
    }

    /// Record what a host allowed: `honoured` connections of `asked`.
    ///
    /// Nothing is learned from a plan of one range: a small file that fits in
    /// one connection says nothing about how many the host would allow.
    pub fn observe(&self, host: &str, asked: u32, honoured: u32) {
        let asked = asked.max(1);
        if asked < 2 {
            return;
        }
        let connections = honoured.clamp(1, asked);
        let key = host.to_ascii_lowercase();
        let was = self.get(&key);
        if was == Some(connections) {
            return;
        }
        {
            let mut hosts = self.hosts.lock().expect("learned caps lock poisoned");
            hosts.insert(
                key,
                LearnedCap {
                    connections,
                    seen_at: now_seconds(),
                },
            );
        }
        self.save();
        crate::log::write(
            "info",
            &format!(
                "{host} honoured {connections} of {asked} connections, the next download starts there"
            ),
        );
    }

    /// Forget what was learned about a host.
    pub fn reset(&self, host: &str) {
        let removed = self
            .hosts
            .lock()
            .expect("learned caps lock poisoned")
            .remove(&host.to_ascii_lowercase())
            .is_some();
        if removed {
            self.save();
        }
    }

    /// Write the caps down, when there is somewhere to write them.
    fn save(&self) {
        let path = self
            .path
            .lock()
            .expect("learned path lock poisoned")
            .clone();
        let Some(path) = path else {
            return;
        };
        let hosts = self.hosts.lock().expect("learned caps lock poisoned").clone();
        let Ok(text) = serde_json::to_string_pretty(&hosts) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let temporary = path.with_extension("json.new");
        if std::fs::write(&temporary, text).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
    }
}

/// The moment now, in seconds since the epoch.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The limits that apply to the whole application.
#[derive(Debug, Clone)]
pub struct Limits {
    /// How many connections may be open at once, over all downloads.
    pub budget: ConnectionBudget,
    /// Per-site connection overrides.
    pub sites: SiteLimits,
    /// What each host allowed the last time it was asked.
    pub learned: LearnedCaps,
    /// The bandwidth cap that applies to every download.
    pub global_speed: SpeedLimiter,
    /// The proxy that new downloads go through, when the user set one.
    pub proxy: Arc<Mutex<Option<String>>>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            budget: ConnectionBudget::new(DEFAULT_MAX_CONNECTIONS),
            sites: SiteLimits::default(),
            learned: LearnedCaps::default(),
            global_speed: SpeedLimiter::unlimited(),
            proxy: Arc::new(Mutex::new(None)),
        }
    }
}

impl Limits {
    /// Set the proxy that new downloads go through. `None` means no proxy.
    pub fn set_proxy(&self, url: Option<&str>) {
        let mut proxy = self.proxy.lock().expect("proxy lock poisoned");
        *proxy = url
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty());
    }

    /// The proxy that new downloads go through.
    pub fn proxy(&self) -> Option<String> {
        self.proxy.lock().expect("proxy lock poisoned").clone()
    }
}

/// The global connection cap, the cap the reference download manager keeps in
/// `HKCU\Software\DownloadManager\MCN`.
pub const DEFAULT_MAX_CONNECTIONS: u32 = 32;

/// Paces the bytes written to disk, so a download cannot use more bandwidth
/// than the user allowed.
///
/// This is a token bucket: a burst of one second's worth of bytes is allowed
/// through, and after that the bytes are handed out at the configured rate.
#[derive(Debug, Clone)]
pub struct SpeedLimiter {
    state: Arc<Mutex<Bucket>>,
}

#[derive(Debug)]
struct Bucket {
    rate: u64,
    tokens: u64,
    last: Instant,
}

impl SpeedLimiter {
    /// A limiter that does not limit anything.
    pub fn unlimited() -> Self {
        Self::new(0)
    }

    /// A limiter that lets `rate` bytes per second through.
    pub fn new(rate: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(Bucket {
                rate,
                tokens: rate,
                last: Instant::now(),
            })),
        }
    }

    /// Change the rate. A rate of zero means unlimited.
    pub fn set_rate(&self, rate: u64) {
        let mut bucket = self.state.lock().expect("bucket lock poisoned");
        bucket.tokens = refill(bucket.tokens, bucket.rate, bucket.last.elapsed());
        let was_unlimited = bucket.rate == 0;
        bucket.rate = rate;
        // A limiter that starts limiting gets a full second of burst, so the
        // first bytes are not held back.
        bucket.tokens = if was_unlimited && rate > 0 {
            rate
        } else {
            bucket.tokens.min(rate)
        };
        bucket.last = Instant::now();
    }

    /// The rate in bytes per second. Zero means unlimited.
    pub fn rate(&self) -> u64 {
        self.state.lock().expect("bucket lock poisoned").rate
    }

    /// Wait until `bytes` may be written.
    ///
    /// The bucket never holds more than one second of bytes, so a request
    /// larger than the rate is paid for in pieces: the bytes that fit are
    /// taken at once and the rest waits for the bucket to fill again. Asking
    /// for the whole request at once would never be satisfied at a rate below
    /// the size of a single read, and the download would stop instead of slow
    /// down.
    pub async fn reserve(&self, bytes: u64) {
        let mut left = bytes;
        while left > 0 {
            let wait = {
                let mut bucket = self.state.lock().expect("bucket lock poisoned");
                if bucket.rate == 0 {
                    return;
                }
                bucket.tokens = refill(bucket.tokens, bucket.rate, bucket.last.elapsed());
                bucket.last = Instant::now();

                if bucket.tokens > 0 {
                    let take = left.min(bucket.tokens);
                    bucket.tokens -= take;
                    left -= take;
                    if left == 0 {
                        return;
                    }
                }

                let target = left.min(bucket.rate).max(1);
                let missing = target.saturating_sub(bucket.tokens);
                Duration::from_secs_f64(missing as f64 / bucket.rate as f64)
            };

            // The sleep is never zero, so a limiter that is being reconfigured
            // cannot spin the task while it waits.
            tokio::time::sleep(wait.max(Duration::from_millis(1))).await;
        }
    }
}

/// Add the bytes that accrued since the last look to the bucket.
///
/// shortcut: the fraction of a second is kept in whole nanoseconds, so the
/// rate is accurate to about one part in a billion.
fn refill(tokens: u64, rate: u64, elapsed: Duration) -> u64 {
    if rate == 0 {
        return tokens;
    }
    let accrued = (rate as u128).saturating_mul(elapsed.as_nanos()) / 1_000_000_000;
    tokens
        .saturating_add(accrued.min(u64::MAX as u128) as u64)
        .min(rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_hands_out_a_limited_number_of_slots() {
        let budget = ConnectionBudget::new(2);
        let first = budget.try_take().expect("first slot");
        let second = budget.try_take().expect("second slot");
        assert!(
            budget.try_take().is_none(),
            "the third slot must be refused"
        );
        drop(first);
        assert!(budget.try_take().is_some());
        drop(second);
    }

    #[test]
    fn the_budget_can_be_resized_while_it_runs() {
        let budget = ConnectionBudget::new(4);
        budget.resize(2);
        let first = budget.try_take().expect("first slot");
        let second = budget.try_take().expect("second slot");
        assert!(
            budget.try_take().is_none(),
            "the third slot must be refused"
        );
        drop(first);
        budget.resize(6);
        assert!(budget.try_take().is_some(), "the cap must have grown");
        drop(second);
    }

    #[test]
    fn the_budget_keeps_the_cap_it_was_given() {
        // The cap must be the number the settings ask for, not that number
        // plus the connections that are already open: a cap that grows every
        // time the settings are saved opens more and more connections.
        let budget = ConnectionBudget::new(4);
        let first = budget.try_take().expect("first slot");
        let second = budget.try_take().expect("second slot");
        budget.resize(4);
        assert_eq!(budget.permits.available_permits(), 2);
        budget.resize(6);
        assert_eq!(budget.permits.available_permits(), 4);
        // Lowering the cap again takes the permits back, held ones included.
        budget.resize(2);
        assert_eq!(budget.permits.available_permits(), 0);
        drop(first);
        drop(second);
    }

    #[test]
    fn a_site_override_wins_over_the_default() {
        let limits = SiteLimits::default();
        assert_eq!(limits.resolve("example.com", 8), 8);
        limits.set("Example.COM", 2);
        assert_eq!(limits.resolve("example.com", 8), 2);
        assert_eq!(limits.resolve("other.com", 8), 8);
    }

    #[test]
    fn a_learned_cap_lowers_what_a_host_is_asked_for() {
        let caps = LearnedCaps::new();
        caps.observe("Example.com", 8, 3);
        assert_eq!(caps.get("example.com"), Some(3));
        assert_eq!(caps.resolve("EXAMPLE.COM", 8), 3);
        // The user's setting is a ceiling, not a floor: a later plan of four
        // does not raise the cap the host refused.
        assert_eq!(caps.resolve("example.com", 4), 3);
        assert_eq!(caps.resolve("other.example", 8), 8);
    }

    #[test]
    fn a_plan_of_one_range_teaches_nothing() {
        let caps = LearnedCaps::new();
        caps.observe("example.com", 1, 1);
        assert_eq!(caps.get("example.com"), None);
    }

    #[test]
    fn a_cap_that_was_refused_six_ways_is_remembered_as_one() {
        let caps = LearnedCaps::new();
        caps.observe("example.com", 8, 0);
        assert_eq!(caps.get("example.com"), Some(1));
    }

    #[test]
    fn a_cap_is_forgotten_when_the_reader_asks() {
        let caps = LearnedCaps::new();
        caps.observe("example.com", 8, 2);
        caps.reset("example.com");
        assert_eq!(caps.get("example.com"), None);
    }

    #[test]
    fn a_cap_survives_the_file_it_was_written_to() {
        let dir = std::env::temp_dir().join(format!(
            "bdm-learned-caps-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let path = dir.join("host-caps.json");

        let caps = LearnedCaps::new();
        caps.open(&path);
        caps.observe("example.com", 8, 2);

        let again = LearnedCaps::new();
        again.open(&path);
        assert_eq!(again.get("example.com"), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_host_of_a_url_is_read_out_of_it() {
        assert_eq!(
            host_of("https://Example.com/a/b"),
            Some("example.com".into())
        );
        assert_eq!(host_of("not a url"), None);
    }

    #[test]
    fn the_bucket_fills_at_the_configured_rate() {
        let one_second = Duration::from_secs(1);
        assert_eq!(refill(0, 1000, one_second), 1000);
        assert_eq!(
            refill(500, 1000, one_second),
            1000,
            "the bucket holds one second"
        );
    }

    #[test]
    fn the_bucket_never_goes_above_one_second_of_bytes() {
        let tokens = refill(900, 100, Duration::from_secs(60));
        assert_eq!(tokens, 100);
    }

    #[tokio::test]
    async fn an_unlimited_limiter_never_waits() {
        let limiter = SpeedLimiter::unlimited();
        limiter.reserve(1024 * 1024).await;
        assert_eq!(limiter.rate(), 0);
    }

    #[tokio::test]
    async fn a_request_larger_than_the_bucket_is_still_let_through() {
        // The bucket holds one second of bytes, so a read of 6 KB against a
        // cap of 4 KB/s can only be paid for in pieces. Asking for the whole
        // read at once used to wait forever at this size.
        let limiter = SpeedLimiter::new(4096);
        let started = Instant::now();
        tokio::time::timeout(Duration::from_secs(5), limiter.reserve(6144))
            .await
            .expect("a reserve larger than the bucket must finish");
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(300),
            "the cap must slow the bytes down, it took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn a_limiter_that_is_turned_off_releases_a_waiting_reserve() {
        let limiter = SpeedLimiter::new(1024);
        let waiting = limiter.clone();
        let task = tokio::spawn(async move { waiting.reserve(4096).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        limiter.set_rate(0);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("the waiting request must be released")
            .expect("the task must not panic");
    }
}
