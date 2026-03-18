use std::time::{Duration, Instant};

/// Rate limiter for API providers.
pub enum RateLimiter {
    Fixed(FixedRateLimiter),
    Adaptive(AdaptiveRateLimiter),
}

/// Fixed-interval rate limiter.
pub struct FixedRateLimiter {
    min_interval: Duration,
    last_call: Option<Instant>,
}

/// Adaptive rate limiter that backs off on 429s and recovers after sustained success.
pub struct AdaptiveRateLimiter {
    base_interval: Duration,
    current_interval: Duration,
    max_interval: Duration,
    last_call: Option<Instant>,
    success_count: u32,
}

impl RateLimiter {
    pub fn fixed(min_interval: Duration) -> Self {
        Self::Fixed(FixedRateLimiter {
            min_interval,
            last_call: None,
        })
    }

    pub fn adaptive(base_interval: Duration, max_interval: Duration) -> Self {
        Self::Adaptive(AdaptiveRateLimiter {
            base_interval,
            current_interval: base_interval,
            max_interval,
            last_call: None,
            success_count: 0,
        })
    }

    pub async fn wait(&mut self) {
        match self {
            Self::Fixed(f) => f.wait().await,
            Self::Adaptive(a) => a.wait().await,
        }
    }

    pub fn on_success(&mut self) {
        if let Self::Adaptive(a) = self {
            a.on_success();
        }
    }

    pub fn on_rate_limited(&mut self) {
        if let Self::Adaptive(a) = self {
            a.on_rate_limited();
        }
    }
}

impl FixedRateLimiter {
    async fn wait(&mut self) {
        if let Some(last) = self.last_call {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                tokio::time::sleep(self.min_interval - elapsed).await;
            }
        }
        self.last_call = Some(Instant::now());
    }
}

impl AdaptiveRateLimiter {
    async fn wait(&mut self) {
        if let Some(last) = self.last_call {
            let elapsed = last.elapsed();
            if elapsed < self.current_interval {
                tokio::time::sleep(self.current_interval - elapsed).await;
            }
        }
        self.last_call = Some(Instant::now());
    }

    fn on_success(&mut self) {
        self.success_count += 1;
        if self.success_count >= 5 {
            self.success_count = 0;
            let reduced = self
                .current_interval
                .saturating_sub(Duration::from_millis(100));
            self.current_interval = reduced.max(self.base_interval);
        }
    }

    fn on_rate_limited(&mut self) {
        self.success_count = 0;
        self.current_interval = (self.current_interval * 2).min(self.max_interval);
    }
}

/// Holds one rate limiter per provider.
pub struct RateLimiters {
    openlibrary: RateLimiter,
    googlebooks: RateLimiter,
    hardcover: RateLimiter,
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiters {
    pub fn new() -> Self {
        Self {
            openlibrary: RateLimiter::fixed(Duration::from_secs(1)),
            googlebooks: RateLimiter::fixed(Duration::from_millis(1500)),
            hardcover: RateLimiter::adaptive(Duration::from_millis(1200), Duration::from_secs(15)),
        }
    }

    pub fn get_mut(&mut self, provider: &str) -> &mut RateLimiter {
        match provider {
            "openlibrary" => &mut self.openlibrary,
            "googlebooks" => &mut self.googlebooks,
            "hardcover" => &mut self.hardcover,
            _ => &mut self.openlibrary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixed_first_call_no_wait() {
        let mut limiter = RateLimiter::fixed(Duration::from_secs(1));
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn fixed_second_call_waits() {
        let mut limiter = RateLimiter::fixed(Duration::from_millis(100));
        limiter.wait().await;
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() >= Duration::from_millis(80));
    }

    #[test]
    fn adaptive_on_rate_limited_doubles_interval() {
        let mut limiter =
            RateLimiter::adaptive(Duration::from_millis(100), Duration::from_millis(1000));
        if let RateLimiter::Adaptive(ref a) = limiter {
            assert_eq!(a.current_interval, Duration::from_millis(100));
        }
        limiter.on_rate_limited();
        if let RateLimiter::Adaptive(ref a) = limiter {
            assert_eq!(a.current_interval, Duration::from_millis(200));
        }
    }

    #[test]
    fn adaptive_on_rate_limited_caps_at_max() {
        let mut limiter =
            RateLimiter::adaptive(Duration::from_millis(100), Duration::from_millis(300));
        limiter.on_rate_limited(); // 200
        limiter.on_rate_limited(); // 300 (capped)
        limiter.on_rate_limited(); // still 300
        if let RateLimiter::Adaptive(ref a) = limiter {
            assert_eq!(a.current_interval, Duration::from_millis(300));
        }
    }

    #[test]
    fn adaptive_recovers_after_5_successes() {
        let mut limiter =
            RateLimiter::adaptive(Duration::from_millis(100), Duration::from_millis(1000));
        limiter.on_rate_limited(); // 200ms
        for _ in 0..5 {
            limiter.on_success();
        }
        if let RateLimiter::Adaptive(ref a) = limiter {
            assert_eq!(a.current_interval, Duration::from_millis(100));
        }
    }

    #[test]
    fn adaptive_does_not_go_below_base() {
        let mut limiter =
            RateLimiter::adaptive(Duration::from_millis(100), Duration::from_millis(1000));
        // Already at base, 5 successes should keep at base
        for _ in 0..10 {
            limiter.on_success();
        }
        if let RateLimiter::Adaptive(ref a) = limiter {
            assert_eq!(a.current_interval, Duration::from_millis(100));
        }
    }

    #[test]
    fn rate_limiters_returns_correct_provider() {
        let mut limiters = RateLimiters::new();
        // Just verify we can get mutable references without panicking
        let _ = limiters.get_mut("openlibrary");
        let _ = limiters.get_mut("googlebooks");
        let _ = limiters.get_mut("hardcover");
        let _ = limiters.get_mut("unknown");
    }
}
