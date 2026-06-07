use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

struct PrefetchConfig {
    min_interval_ms: u64,
    max_batch: usize,
    backoff_multiplier: u32,
    max_backoff_ms: u64,
}

impl Default for PrefetchConfig {
    fn default() -> Self {
        Self {
            min_interval_ms: 500,
            max_batch: 5,
            backoff_multiplier: 2,
            max_backoff_ms: 60_000,
        }
    }
}

struct ExponentialBackoff {
    base_interval: Duration,
    multiplier: u32,
    max_interval: Duration,
    failure_count: AtomicU32,
}

impl ExponentialBackoff {
    fn new(base_interval: Duration, multiplier: u32, max_interval: Duration) -> Self {
        Self {
            base_interval,
            multiplier,
            max_interval,
            failure_count: AtomicU32::new(0),
        }
    }

    fn get_delay(&self) -> Duration {
        let count = self.failure_count.load(Ordering::Relaxed);
        if count == 0 {
            return self.base_interval;
        }

        let delay = self.base_interval * self.multiplier.pow(count);
        delay.min(self.max_interval)
    }

    fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
    }
}

struct PrefetchRateLimiter {
    last_prefetch: std::sync::Mutex<Instant>,
    min_interval: Duration,
    max_batch: usize,
}

impl PrefetchRateLimiter {
    fn new(min_interval: Duration, max_batch: usize) -> Self {
        Self {
            last_prefetch: std::sync::Mutex::new(
                Instant::now() - min_interval - Duration::from_millis(1),
            ),
            min_interval,
            max_batch,
        }
    }

    fn can_prefetch(&self) -> bool {
        let last = *self.last_prefetch.lock().unwrap();
        Instant::now().duration_since(last) >= self.min_interval
    }

    fn record_prefetch(&self) {
        *self.last_prefetch.lock().unwrap() = Instant::now();
    }

    fn get_batch_size(&self, total_expired: usize) -> usize {
        total_expired.min(self.max_batch)
    }
}

#[test]
fn test_prefetch_rate_limiter_initial() {
    let limiter = PrefetchRateLimiter::new(Duration::from_millis(500), 5);
    assert!(limiter.can_prefetch());
}

#[test]
fn test_prefetch_rate_limiter_respects_interval() {
    let limiter = PrefetchRateLimiter::new(Duration::from_millis(500), 5);

    limiter.record_prefetch();

    std::thread::sleep(Duration::from_millis(250));
    assert!(!limiter.can_prefetch());

    std::thread::sleep(Duration::from_millis(350));
    assert!(limiter.can_prefetch());
}

#[test]
fn test_prefetch_batch_limiting() {
    let limiter = PrefetchRateLimiter::new(Duration::from_millis(500), 5);

    assert_eq!(limiter.get_batch_size(3), 3);
    assert_eq!(limiter.get_batch_size(5), 5);
    assert_eq!(limiter.get_batch_size(10), 5);
    assert_eq!(limiter.get_batch_size(0), 0);
}

#[test]
fn test_exponential_backoff_initial() {
    let backoff = ExponentialBackoff::new(Duration::from_millis(100), 2, Duration::from_secs(60));

    assert_eq!(backoff.get_delay(), Duration::from_millis(100));
}

#[test]
fn test_exponential_backoff_growth() {
    let backoff = ExponentialBackoff::new(Duration::from_millis(100), 2, Duration::from_secs(60));

    backoff.record_failure();
    assert_eq!(backoff.get_delay(), Duration::from_millis(200));

    backoff.record_failure();
    assert_eq!(backoff.get_delay(), Duration::from_millis(400));

    backoff.record_failure();
    assert_eq!(backoff.get_delay(), Duration::from_millis(800));
}

#[test]
fn test_exponential_backoff_capped() {
    let backoff =
        ExponentialBackoff::new(Duration::from_millis(100), 2, Duration::from_millis(500));

    backoff.record_failure();
    backoff.record_failure();
    backoff.record_failure();
    backoff.record_failure();

    assert_eq!(backoff.get_delay(), Duration::from_millis(500));
}

#[test]
fn test_exponential_backoff_resets_on_success() {
    let backoff = ExponentialBackoff::new(Duration::from_millis(100), 2, Duration::from_secs(60));

    backoff.record_failure();
    backoff.record_failure();
    assert_eq!(backoff.get_delay(), Duration::from_millis(400));

    backoff.record_success();
    assert_eq!(backoff.get_delay(), Duration::from_millis(100));
}

#[test]
fn test_prefetch_config_defaults() {
    let config = PrefetchConfig::default();

    assert_eq!(config.min_interval_ms, 500);
    assert_eq!(config.max_batch, 5);
    assert_eq!(config.backoff_multiplier, 2);
    assert_eq!(config.max_backoff_ms, 60_000);
}

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "env var set_var/var round-trip unreliable on Windows CI"
)]
fn test_env_var_parsing() {
    unsafe {
        std::env::set_var("PREFETCH_MIN_INTERVAL", "1000");
        std::env::set_var("PREFETCH_MAX_BATCH", "10");
    }

    let min_interval: u64 = std::env::var("PREFETCH_MIN_INTERVAL")
        .as_deref()
        .unwrap_or("500")
        .parse()
        .unwrap_or(500);

    let max_batch: usize = std::env::var("PREFETCH_MAX_BATCH")
        .as_deref()
        .unwrap_or("5")
        .parse()
        .unwrap_or(5);

    assert_eq!(min_interval, 1000);
    assert_eq!(max_batch, 10);

    unsafe {
        std::env::remove_var("PREFETCH_MIN_INTERVAL");
        std::env::remove_var("PREFETCH_MAX_BATCH");
    }
}

#[test]
fn test_env_var_fallback() {
    unsafe {
        std::env::remove_var("PREFETCH_MIN_INTERVAL");
        std::env::remove_var("PREFETCH_MAX_BATCH");
    }

    let min_interval: u64 = std::env::var("PREFETCH_MIN_INTERVAL")
        .as_deref()
        .unwrap_or("500")
        .parse()
        .unwrap_or(500);

    let max_batch: usize = std::env::var("PREFETCH_MAX_BATCH")
        .as_deref()
        .unwrap_or("5")
        .parse()
        .unwrap_or(5);

    assert_eq!(min_interval, 500);
    assert_eq!(max_batch, 5);
}

#[test]
fn test_concurrent_prefetch_control() {
    use std::sync::Arc;
    use std::thread;

    let limiter = Arc::new(PrefetchRateLimiter::new(Duration::from_millis(100), 2));
    let counter = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();

    for _ in 0..10 {
        let limiter_clone = Arc::clone(&limiter);
        let counter_clone = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                if limiter_clone.can_prefetch() {
                    limiter_clone.record_prefetch();
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
                thread::sleep(Duration::from_millis(50));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert!(counter.load(Ordering::Relaxed) > 0);
}
