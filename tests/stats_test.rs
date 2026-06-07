use std::sync::atomic::{AtomicU64, Ordering};

struct StatsCounter {
    total_queries: AtomicU64,
    total_query_time_ns: AtomicU64,
    bg_total_queries: AtomicU64,
    bg_total_query_time_ns: AtomicU64,
}

impl StatsCounter {
    fn new() -> Self {
        Self {
            total_queries: AtomicU64::new(0),
            total_query_time_ns: AtomicU64::new(0),
            bg_total_queries: AtomicU64::new(0),
            bg_total_query_time_ns: AtomicU64::new(0),
        }
    }

    fn record_query(&self, duration_ns: u64) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.total_query_time_ns
            .fetch_add(duration_ns, Ordering::Relaxed);
    }

    fn record_bg_query(&self, duration_ns: u64) {
        self.bg_total_queries.fetch_add(1, Ordering::Relaxed);
        self.bg_total_query_time_ns
            .fetch_add(duration_ns, Ordering::Relaxed);
    }

    fn total_queries(&self) -> u64 {
        self.total_queries.load(Ordering::Relaxed)
    }

    fn bg_total_queries(&self) -> u64 {
        self.bg_total_queries.load(Ordering::Relaxed)
    }

    fn avg_query_time_ms(&self) -> f64 {
        let total = self.total_queries.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let time_ns = self.total_query_time_ns.load(Ordering::Relaxed);
        time_ns as f64 / total as f64 / 1_000_000.0
    }

    fn bg_avg_query_time_ms(&self) -> f64 {
        let total = self.bg_total_queries.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let time_ns = self.bg_total_query_time_ns.load(Ordering::Relaxed);
        time_ns as f64 / total as f64 / 1_000_000.0
    }
}

#[test]
fn test_empty_stats() {
    let counter = StatsCounter::new();

    assert_eq!(counter.total_queries(), 0);
    assert_eq!(counter.bg_total_queries(), 0);
    assert_eq!(counter.avg_query_time_ms(), 0.0);
    assert_eq!(counter.bg_avg_query_time_ms(), 0.0);
}

#[test]
fn test_single_query() {
    let counter = StatsCounter::new();

    counter.record_query(1_000_000);

    assert_eq!(counter.total_queries(), 1);
    assert_eq!(counter.avg_query_time_ms(), 1.0);
}

#[test]
fn test_multiple_queries() {
    let counter = StatsCounter::new();

    counter.record_query(1_000_000);
    counter.record_query(2_000_000);
    counter.record_query(3_000_000);

    assert_eq!(counter.total_queries(), 3);
    assert_eq!(counter.avg_query_time_ms(), 2.0);
}

#[test]
fn test_background_queries() {
    let counter = StatsCounter::new();

    counter.record_bg_query(5_000_000);
    counter.record_bg_query(15_000_000);

    assert_eq!(counter.bg_total_queries(), 2);
    assert_eq!(counter.bg_avg_query_time_ms(), 10.0);
}

#[test]
fn test_mixed_queries() {
    let counter = StatsCounter::new();

    counter.record_query(1_000_000);
    counter.record_query(2_000_000);
    counter.record_bg_query(10_000_000);
    counter.record_bg_query(20_000_000);

    assert_eq!(counter.total_queries(), 2);
    assert_eq!(counter.bg_total_queries(), 2);
    assert_eq!(counter.avg_query_time_ms(), 1.5);
    assert_eq!(counter.bg_avg_query_time_ms(), 15.0);
}

#[test]
fn test_concurrent_updates() {
    use std::sync::Arc;
    use std::thread;

    let counter = Arc::new(StatsCounter::new());
    let mut handles = Vec::new();

    for _ in 0..10 {
        let counter_clone = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                counter_clone.record_query((i + 1) * 1_000_000);
                counter_clone.record_bg_query((i + 1) * 5_000_000);
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(counter.total_queries(), 1000);
    assert_eq!(counter.bg_total_queries(), 1000);

    let expected_avg = (1..=100).sum::<u32>() as f64 / 100.0;
    let actual_avg = counter.avg_query_time_ms();
    assert!((actual_avg - expected_avg).abs() < 0.01);

    let expected_bg_avg = (1..=100).sum::<u32>() as f64 * 5.0 / 100.0;
    let actual_bg_avg = counter.bg_avg_query_time_ms();
    assert!((actual_bg_avg - expected_bg_avg).abs() < 0.01);
}

#[test]
fn test_large_values() {
    let counter = StatsCounter::new();

    counter.record_query(u64::MAX);

    assert_eq!(counter.total_queries(), 1);
    assert!(counter.avg_query_time_ms() > 0.0);
}
