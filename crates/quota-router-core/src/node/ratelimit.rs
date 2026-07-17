use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use super::provider::RouterNodeId;

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }
}

pub struct RateLimiter {
    consumer_buckets: Mutex<BTreeMap<[u8; 32], TokenBucket>>,
    peer_buckets: Mutex<BTreeMap<RouterNodeId, TokenBucket>>,
    _max_sustained: u32,
    _max_burst: u32,
}

impl RateLimiter {
    pub fn new(max_sustained: u32, max_burst: u32) -> Self {
        Self {
            consumer_buckets: Mutex::new(BTreeMap::new()),
            peer_buckets: Mutex::new(BTreeMap::new()),
            _max_sustained: max_sustained,
            _max_burst: max_burst,
        }
    }

    pub fn check_consumer(&self, consumer_id: &[u8; 32]) -> bool {
        let mut buckets = self.consumer_buckets.lock().unwrap();
        let bucket = buckets.entry(*consumer_id).or_insert_with(|| {
            TokenBucket::new(self._max_burst as f64, self._max_sustained as f64)
        });
        bucket.try_consume(1.0)
    }

    pub fn check_peer(&self, peer_id: &RouterNodeId) -> bool {
        let mut buckets = self.peer_buckets.lock().unwrap();
        let bucket = buckets.entry(*peer_id).or_insert_with(|| {
            TokenBucket::new(self._max_burst as f64, self._max_sustained as f64)
        });
        bucket.try_consume(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_within_limit() {
        let rl = RateLimiter::new(100, 200);
        for _ in 0..100 {
            assert!(rl.check_consumer(&[1u8; 32]));
        }
    }

    #[test]
    fn rate_limiter_blocks_over_burst() {
        let rl = RateLimiter::new(10, 10);
        for _ in 0..10 {
            assert!(rl.check_consumer(&[1u8; 32]));
        }
        assert!(!rl.check_consumer(&[1u8; 32]));
    }

    #[test]
    fn rate_limiter_per_consumer_isolation() {
        let rl = RateLimiter::new(5, 5);
        for _ in 0..5 {
            assert!(rl.check_consumer(&[1u8; 32]));
        }
        assert!(!rl.check_consumer(&[1u8; 32]));
        // Different consumer still allowed
        assert!(rl.check_consumer(&[2u8; 32]));
    }

    #[test]
    fn rate_limiter_peer_isolation() {
        let rl = RateLimiter::new(3, 3);
        let p1 = RouterNodeId([1u8; 32]);
        let p2 = RouterNodeId([2u8; 32]);
        for _ in 0..3 {
            assert!(rl.check_peer(&p1));
        }
        assert!(!rl.check_peer(&p1));
        assert!(rl.check_peer(&p2));
    }

    #[test]
    fn rate_limiter_token_refill() {
        let rl = RateLimiter::new(100, 2);
        let consumer = [1u8; 32];
        assert!(rl.check_consumer(&consumer));
        assert!(rl.check_consumer(&consumer));
        assert!(!rl.check_consumer(&consumer));
        // Wait for refill (refill_rate = 100/s, so ~10ms per token)
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(rl.check_consumer(&consumer));
    }

    #[test]
    fn rate_limiter_default_config() {
        let rl = RateLimiter::new(0, 500);
        let consumer = [1u8; 32];
        for _ in 0..500 {
            assert!(rl.check_consumer(&consumer));
        }
        assert!(!rl.check_consumer(&consumer));
    }
}
