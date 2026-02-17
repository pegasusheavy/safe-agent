use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::error::{Result, SafeAgentError};

/// Sliding-window rate limiter for tool calls.
///
/// Tracks tool call timestamps in memory and enforces per-minute
/// and per-hour limits to prevent runaway tool loops.
pub struct RateLimiter {
    per_minute: u32,
    per_hour: u32,
    /// Recent timestamps of tool calls, oldest first.
    calls: Mutex<VecDeque<Instant>>,
}

/// Rate limit status.
#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    /// Calls in the last minute.
    pub calls_last_minute: u32,
    /// Calls in the last hour.
    pub calls_last_hour: u32,
    /// Per-minute limit (0 = unlimited).
    pub limit_per_minute: u32,
    /// Per-hour limit (0 = unlimited).
    pub limit_per_hour: u32,
    /// Whether rate limited.
    pub is_limited: bool,
}

impl RateLimiter {
    pub fn new(per_minute: u32, per_hour: u32) -> Self {
        Self {
            per_minute,
            per_hour,
            calls: Mutex::new(VecDeque::new()),
        }
    }

    /// Record a tool call and check if the rate limit is exceeded.
    /// Returns Ok(()) if within limits, or Err with a rate-limit message.
    pub fn check_and_record(&self) -> Result<()> {
        let mut calls = self.calls.lock().unwrap();
        let now = Instant::now();

        // Prune entries older than 1 hour
        let one_hour_ago = now - Duration::from_secs(3600);
        while calls.front().is_some_and(|t| *t < one_hour_ago) {
            calls.pop_front();
        }

        // Count calls in the last minute
        let one_minute_ago = now - Duration::from_secs(60);
        let calls_last_minute = calls.iter().filter(|t| **t >= one_minute_ago).count() as u32;

        // Check per-minute limit
        if self.per_minute > 0 && calls_last_minute >= self.per_minute {
            warn!(
                calls = calls_last_minute,
                limit = self.per_minute,
                "rate limit exceeded (per minute)"
            );
            return Err(SafeAgentError::RateLimited(format!(
                "tool call rate limit exceeded: {calls_last_minute}/{} per minute",
                self.per_minute
            )));
        }

        // Check per-hour limit
        let calls_last_hour = calls.len() as u32;
        if self.per_hour > 0 && calls_last_hour >= self.per_hour {
            warn!(
                calls = calls_last_hour,
                limit = self.per_hour,
                "rate limit exceeded (per hour)"
            );
            return Err(SafeAgentError::RateLimited(format!(
                "tool call rate limit exceeded: {calls_last_hour}/{} per hour",
                self.per_hour
            )));
        }

        // Record this call
        calls.push_back(now);
        Ok(())
    }

    /// Check limits without recording a call.
    pub fn status(&self) -> RateLimitStatus {
        let calls = self.calls.lock().unwrap();
        let now = Instant::now();

        let one_minute_ago = now - Duration::from_secs(60);
        let calls_last_minute = calls.iter().filter(|t| **t >= one_minute_ago).count() as u32;
        let calls_last_hour = calls.len() as u32;

        let minute_limited = self.per_minute > 0 && calls_last_minute >= self.per_minute;
        let hour_limited = self.per_hour > 0 && calls_last_hour >= self.per_hour;

        RateLimitStatus {
            calls_last_minute,
            calls_last_hour,
            limit_per_minute: self.per_minute,
            limit_per_hour: self.per_hour,
            is_limited: minute_limited || hour_limited,
        }
    }

    /// Reset all tracked calls (useful for testing).
    #[cfg(test)]
    pub fn reset(&self) {
        let mut calls = self.calls.lock().unwrap();
        calls.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_within_limits() {
        let limiter = RateLimiter::new(10, 100);
        for _ in 0..5 {
            assert!(limiter.check_and_record().is_ok());
        }
        let status = limiter.status();
        assert_eq!(status.calls_last_minute, 5);
        assert!(!status.is_limited);
    }

    #[test]
    fn test_minute_limit_exceeded() {
        let limiter = RateLimiter::new(3, 100);
        for _ in 0..3 {
            assert!(limiter.check_and_record().is_ok());
        }
        let result = limiter.check_and_record();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("rate limit"));
        assert!(err_msg.contains("per minute"));
    }

    #[test]
    fn test_hour_limit_exceeded() {
        let limiter = RateLimiter::new(0, 5); // no per-minute limit
        for _ in 0..5 {
            assert!(limiter.check_and_record().is_ok());
        }
        let result = limiter.check_and_record();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("per hour"));
    }

    #[test]
    fn test_unlimited() {
        let limiter = RateLimiter::new(0, 0);
        for _ in 0..100 {
            assert!(limiter.check_and_record().is_ok());
        }
        let status = limiter.status();
        assert!(!status.is_limited);
    }

    #[test]
    fn test_status_reflects_limits() {
        let limiter = RateLimiter::new(5, 50);
        let status = limiter.status();
        assert_eq!(status.limit_per_minute, 5);
        assert_eq!(status.limit_per_hour, 50);
        assert_eq!(status.calls_last_minute, 0);
        assert_eq!(status.calls_last_hour, 0);
        assert!(!status.is_limited);
    }

    #[test]
    fn test_reset() {
        let limiter = RateLimiter::new(5, 50);
        for _ in 0..4 {
            limiter.check_and_record().unwrap();
        }
        assert_eq!(limiter.status().calls_last_minute, 4);
        limiter.reset();
        assert_eq!(limiter.status().calls_last_minute, 0);
    }
}
