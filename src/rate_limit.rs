use crate::error::{LinkError, Result};
use std::time::{Duration, Instant};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_messages_per_second: u32,
    pub max_bytes_per_second: u64,
    pub burst_size: u32,
    pub window_duration: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_messages_per_second: 1000,
            max_bytes_per_second: 10 * 1024 * 1024,
            burst_size: 100,
            window_duration: Duration::from_secs(1),
        }
    }
}

impl RateLimitConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_messages(mut self, max: u32) -> Self {
        self.max_messages_per_second = max;
        self
    }

    pub fn with_max_bytes(mut self, max: u64) -> Self {
        self.max_bytes_per_second = max;
        self
    }

    pub fn with_burst_size(mut self, size: u32) -> Self {
        self.burst_size = size;
        self
    }

    pub fn with_window_duration(mut self, duration: Duration) -> Self {
        self.window_duration = duration;
        self
    }
}

struct MessageRecord {
    timestamp: Instant,
    size: u64,
}

pub struct RateLimiter {
    config: RateLimitConfig,
    message_history: VecDeque<MessageRecord>,
    byte_history: VecDeque<MessageRecord>,
    total_messages: u64,
    total_bytes: u64,
    total_rejected: u64,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            message_history: VecDeque::new(),
            byte_history: VecDeque::new(),
            total_messages: 0,
            total_bytes: 0,
            total_rejected: 0,
        }
    }

    pub fn check_and_record(&mut self, message_size: u64) -> Result<()> {
        let now = Instant::now();

        self.cleanup_old_records(now);

        let messages_in_window = self.count_messages_in_window(now);
        let bytes_in_window = self.count_bytes_in_window(now);

        if messages_in_window >= self.config.max_messages_per_second {
            self.total_rejected += 1;
            return Err(LinkError::RateLimitExceeded(
                format!("Message rate limit exceeded: {} msgs/sec", self.config.max_messages_per_second)
            ));
        }

        if bytes_in_window + message_size > self.config.max_bytes_per_second {
            self.total_rejected += 1;
            return Err(LinkError::RateLimitExceeded(
                format!("Byte rate limit exceeded: {} bytes/sec", self.config.max_bytes_per_second)
            ));
        }

        let burst_count = self.count_recent_burst(now);
        if burst_count >= self.config.burst_size {
            self.total_rejected += 1;
            return Err(LinkError::RateLimitExceeded(
                format!("Burst limit exceeded: {} msgs", self.config.burst_size)
            ));
        }

        self.record_message(now, message_size);

        Ok(())
    }

    pub fn check(&mut self, message_size: u64) -> bool {
        self.check_and_record(message_size).is_ok()
    }

    fn record_message(&mut self, timestamp: Instant, size: u64) {
        let record = MessageRecord {
            timestamp,
            size,
        };

        self.message_history.push_back(record.clone());
        self.byte_history.push_back(record);

        self.total_messages += 1;
        self.total_bytes += size;
    }

    fn cleanup_old_records(&mut self, now: Instant) {
        let cutoff = now - self.config.window_duration;

        while let Some(record) = self.message_history.front() {
            if record.timestamp < cutoff {
                self.message_history.pop_front();
            } else {
                break;
            }
        }

        while let Some(record) = self.byte_history.front() {
            if record.timestamp < cutoff {
                self.byte_history.pop_front();
            } else {
                break;
            }
        }
    }

    fn count_messages_in_window(&self, now: Instant) -> u32 {
        let cutoff = now - self.config.window_duration;
        self.message_history.iter()
            .filter(|r| r.timestamp >= cutoff)
            .count() as u32
    }

    fn count_bytes_in_window(&self, now: Instant) -> u64 {
        let cutoff = now - self.config.window_duration;
        self.byte_history.iter()
            .filter(|r| r.timestamp >= cutoff)
            .map(|r| r.size)
            .sum()
    }

    fn count_recent_burst(&self, now: Instant) -> u32 {
        let burst_window = Duration::from_millis(100);
        let cutoff = now - burst_window;

        self.message_history.iter()
            .filter(|r| r.timestamp >= cutoff)
            .count() as u32
    }

    pub fn reset(&mut self) {
        self.message_history.clear();
        self.byte_history.clear();
    }

    pub fn get_stats(&self) -> RateLimitStats {
        RateLimitStats {
            total_messages: self.total_messages,
            total_bytes: self.total_bytes,
            total_rejected: self.total_rejected,
            messages_in_window: self.message_history.len() as u32,
            bytes_in_window: self.byte_history.iter().map(|r| r.size).sum(),
        }
    }

    pub fn get_config(&self) -> &RateLimitConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: RateLimitConfig) {
        self.config = config;
    }
}

impl Clone for MessageRecord {
    fn clone(&self) -> Self {
        Self {
            timestamp: self.timestamp,
            size: self.size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub total_messages: u64,
    pub total_bytes: u64,
    pub total_rejected: u64,
    pub messages_in_window: u32,
    pub bytes_in_window: u64,
}

pub struct TokenBucketRateLimiter {
    capacity: u32,
    tokens: u32,
    refill_rate: u32,
    last_refill: Instant,
    total_messages: u64,
    total_rejected: u64,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: Instant::now(),
            total_messages: 0,
            total_rejected: 0,
        }
    }

    pub fn check_and_consume(&mut self) -> Result<()> {
        self.refill();

        if self.tokens == 0 {
            self.total_rejected += 1;
            return Err(LinkError::RateLimitExceeded(
                format!("Token bucket empty (capacity: {})", self.capacity)
            ));
        }

        self.tokens -= 1;
        self.total_messages += 1;

        Ok(())
    }

    pub fn check(&mut self) -> bool {
        self.check_and_consume().is_ok()
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let elapsed_secs = elapsed.as_secs_f64();

        let tokens_to_add = (elapsed_secs * self.refill_rate as f64) as u32;

        if tokens_to_add > 0 {
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
            self.last_refill = now;
        }
    }

    pub fn reset(&mut self) {
        self.tokens = self.capacity;
        self.last_refill = Instant::now();
    }

    pub fn get_available_tokens(&self) -> u32 {
        self.tokens
    }

    pub fn get_stats(&self) -> (u64, u64) {
        (self.total_messages, self.total_rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_rate_limiter_basic() {
        let config = RateLimitConfig::new()
            .with_max_messages(10)
            .with_max_bytes(1000);

        let mut limiter = RateLimiter::new(config);

        for _ in 0..10 {
            assert!(limiter.check_and_record(50).is_ok());
        }

        assert!(limiter.check_and_record(50).is_err());
    }

    #[test]
    fn test_rate_limiter_byte_limit() {
        let config = RateLimitConfig::new()
            .with_max_messages(100)
            .with_max_bytes(500);

        let mut limiter = RateLimiter::new(config);

        assert!(limiter.check_and_record(300).is_ok());
        assert!(limiter.check_and_record(300).is_err());
    }

    #[test]
    fn test_rate_limiter_burst() {
        let config = RateLimitConfig::new()
            .with_max_messages(1000)
            .with_burst_size(5);

        let mut limiter = RateLimiter::new(config);

        for _ in 0..5 {
            assert!(limiter.check_and_record(100).is_ok());
        }

        assert!(limiter.check_and_record(100).is_err());
    }

    #[test]
    fn test_rate_limiter_window() {
        let config = RateLimitConfig::new()
            .with_max_messages(5)
            .with_window_duration(Duration::from_millis(100));

        let mut limiter = RateLimiter::new(config);

        for _ in 0..5 {
            assert!(limiter.check_and_record(100).is_ok());
        }

        assert!(limiter.check_and_record(100).is_err());

        thread::sleep(Duration::from_millis(150));

        assert!(limiter.check_and_record(100).is_ok());
    }

    #[test]
    fn test_token_bucket() {
        let mut limiter = TokenBucketRateLimiter::new(5, 10);

        for _ in 0..5 {
            assert!(limiter.check_and_consume().is_ok());
        }

        assert!(limiter.check_and_consume().is_err());

        thread::sleep(Duration::from_millis(100));
        limiter.refill();

        assert!(limiter.check_and_consume().is_ok());
    }

    #[test]
    fn test_rate_limiter_stats() {
        let config = RateLimitConfig::new().with_max_messages(5);
        let mut limiter = RateLimiter::new(config);

        for _ in 0..3 {
            let _ = limiter.check_and_record(100);
        }

        for _ in 0..3 {
            let _ = limiter.check_and_record(100);
        }

        let stats = limiter.get_stats();
        assert_eq!(stats.total_messages, 5);
        assert_eq!(stats.total_rejected, 1);
    }
}
