// * [FR-04] [AUDIT-1] Circuit Breaker & Playwright Handoff
// * Tracks domain failures and hands off to external renderer when threshold exceeded

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use xxhash_rust::xxh64::xxh64;

// * Circuit breaker threshold - failures before handoff
const FAILURE_THRESHOLD: u32 = 3;

// * Redis key prefixes
const FAILURES_PREFIX: &str = "failures";
const DOMAIN_CONFIG_PREFIX: &str = "domain_config";
const SLOW_RENDER_QUEUE: &str = "queue:slow_render_tasks";

// * TTL for failure counter (reset after 1 hour of no failures)
const FAILURE_TTL_SECS: u64 = 3600;

#[derive(Debug, Error)]
pub enum CircuitBreakerError {
    #[error("Redis connection error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("Circuit breaker tripped - domain requires full browser")]
    CircuitTripped,
}

// * Circuit state for a domain
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,
    Open,
}

// * Result of checking circuit state
#[derive(Debug)]
pub struct CircuitCheckResult {
    pub state: CircuitState,
    pub failure_count: u32,
    pub requires_full_browser: bool,
}

// * CircuitBreaker manages failure tracking and Playwright handoff
pub struct CircuitBreaker {
    redis: Option<ConnectionManager>,
}

impl CircuitBreaker {
    // * Creates a new CircuitBreaker with optional Redis connection
    pub async fn new(redis_url: Option<&str>) -> Result<Self, CircuitBreakerError> {
        let redis = if let Some(url) = redis_url {
            let client = redis::Client::open(url)?;
            Some(ConnectionManager::new(client).await?)
        } else {
            None
        };

        Ok(Self { redis })
    }

    // * Computes domain hash for Redis keys
    fn domain_hash(domain: &str) -> u64 {
        xxh64(domain.as_bytes(), 0)
    }

    // * Returns the Redis key for failure count
    fn failure_key(domain: &str) -> String {
        format!("{}:{}:count", FAILURES_PREFIX, Self::domain_hash(domain))
    }

    // * Returns the Redis key for domain config
    fn config_key(domain: &str) -> String {
        format!(
            "{}:{}:requires_full_browser",
            DOMAIN_CONFIG_PREFIX,
            Self::domain_hash(domain)
        )
    }

    // * Checks the current circuit state for a domain
    pub async fn check(&self, domain: &str) -> Result<CircuitCheckResult, CircuitBreakerError> {
        if let Some(mut redis) = self.redis.clone() {
            // * Check if domain requires full browser
            let requires_full_browser: bool = redis
                .get(&Self::config_key(domain))
                .await
                .unwrap_or(false);

            if requires_full_browser {
                return Ok(CircuitCheckResult {
                    state: CircuitState::Open,
                    failure_count: FAILURE_THRESHOLD + 1,
                    requires_full_browser: true,
                });
            }

            // * Get current failure count
            let failure_count: u32 = redis
                .get(&Self::failure_key(domain))
                .await
                .unwrap_or(0);

            let state = if failure_count > FAILURE_THRESHOLD {
                CircuitState::Open
            } else {
                CircuitState::Closed
            };

            Ok(CircuitCheckResult {
                state,
                failure_count,
                requires_full_browser: false,
            })
        } else {
            // * Without Redis, always return closed circuit
            Ok(CircuitCheckResult {
                state: CircuitState::Closed,
                failure_count: 0,
                requires_full_browser: false,
            })
        }
    }

    // * Records a failure for a domain (crash or timeout)
    pub async fn record_failure(&self, domain: &str) -> Result<u32, CircuitBreakerError> {
        if let Some(mut redis) = self.redis.clone() {
            let key = Self::failure_key(domain);

            // * Increment failure count
            let count: u32 = redis.incr(&key, 1).await?;

            // * Set TTL on first failure
            if count == 1 {
                redis.expire::<_, ()>(&key, FAILURE_TTL_SECS as i64).await?;
            }

            debug!("Domain '{}' failure count: {}", domain, count);

            // * Check if threshold exceeded
            if count > FAILURE_THRESHOLD {
                self.trip_circuit(domain).await?;
            }

            Ok(count)
        } else {
            Ok(0)
        }
    }

    // * Trips the circuit breaker for a domain
    async fn trip_circuit(&self, domain: &str) -> Result<(), CircuitBreakerError> {
        if let Some(mut redis) = self.redis.clone() {
            // * Mark domain as requiring full browser
            redis
                .set::<_, _, ()>(&Self::config_key(domain), true)
                .await?;

            warn!(
                "Circuit breaker TRIPPED for domain '{}' - marking as requires_full_browser",
                domain
            );
        }
        Ok(())
    }

    // * Hands off a URL to the slow render queue
    pub async fn handoff_to_queue(
        &self,
        normalized_url: &str,
    ) -> Result<(), CircuitBreakerError> {
        if let Some(mut redis) = self.redis.clone() {
            redis
                .rpush::<_, _, ()>(SLOW_RENDER_QUEUE, normalized_url)
                .await?;

            info!(
                "URL '{}' handed off to slow render queue",
                normalized_url
            );
        }
        Ok(())
    }

    // * Checks if domain requires full browser and should bypass local rendering
    pub async fn should_bypass_local(&self, domain: &str) -> Result<bool, CircuitBreakerError> {
        let result = self.check(domain).await?;
        Ok(result.requires_full_browser)
    }

    // * Handles a failure - increments counter and hands off if threshold exceeded
    pub async fn handle_failure(
        &self,
        domain: &str,
        normalized_url: &str,
    ) -> Result<bool, CircuitBreakerError> {
        let count = self.record_failure(domain).await?;

        if count > FAILURE_THRESHOLD {
            self.handoff_to_queue(normalized_url).await?;
            return Ok(true); // * Indicates handoff occurred
        }

        Ok(false)
    }

    // * Resets the circuit breaker for a domain (for testing or manual recovery)
    pub async fn reset(&self, domain: &str) -> Result<(), CircuitBreakerError> {
        if let Some(mut redis) = self.redis.clone() {
            redis.del::<_, ()>(&Self::failure_key(domain)).await?;
            redis.del::<_, ()>(&Self::config_key(domain)).await?;
            info!("Circuit breaker RESET for domain '{}'", domain);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_without_redis() {
        let breaker = CircuitBreaker::new(None).await.unwrap();

        // * Without Redis, should always return closed
        let result = breaker.check("example.com").await.unwrap();
        assert_eq!(result.state, CircuitState::Closed);
        assert_eq!(result.failure_count, 0);
        assert!(!result.requires_full_browser);
    }

    #[tokio::test]
    async fn test_should_bypass_local_without_redis() {
        let breaker = CircuitBreaker::new(None).await.unwrap();

        // * Without Redis, should not bypass
        assert!(!breaker.should_bypass_local("example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_handle_failure_without_redis() {
        let breaker = CircuitBreaker::new(None).await.unwrap();

        // * Without Redis, should return false (no handoff)
        let handed_off = breaker
            .handle_failure("example.com", "https://example.com/page")
            .await
            .unwrap();
        assert!(!handed_off);
    }

    #[test]
    fn test_domain_hash_consistency() {
        let hash1 = CircuitBreaker::domain_hash("example.com");
        let hash2 = CircuitBreaker::domain_hash("example.com");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_key_formats() {
        let failure_key = CircuitBreaker::failure_key("example.com");
        assert!(failure_key.starts_with("failures:"));
        assert!(failure_key.ends_with(":count"));

        let config_key = CircuitBreaker::config_key("example.com");
        assert!(config_key.starts_with("domain_config:"));
        assert!(config_key.ends_with(":requires_full_browser"));
    }
}
