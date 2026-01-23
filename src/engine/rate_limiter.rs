// * [FR-03] [APP-A.3] [AUDIT-1] Rate Limiter & Robotstxt Parser
// * Handles per-domain rate limiting, robots.txt compliance, and blacklist management

use governor::{Quota, RateLimiter as GovernorLimiter};
use nonzero_ext::nonzero;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use robotstxt::DefaultMatcher;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use xxhash_rust::xxh64::xxh64;

// * TTL constants for Redis keys
const BACKOFF_429_TTL_SECS: u64 = 3600;
const BLACKLIST_TTL_SECS: u64 = 86400;
const DEFAULT_CRAWL_DELAY_MS: u64 = 1000;
const SLOW_PATH_MULTIPLIER: u64 = 2;

// * Redis key prefixes
const RATELIMIT_PREFIX: &str = "ratelimit";
const BLACKLIST_PREFIX: &str = "blacklist";

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Redis connection error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("Domain is blacklisted until TTL expires")]
    DomainBlacklisted,

    #[error("Rate limit exceeded, retry after delay")]
    RateLimitExceeded,

    #[error("Failed to parse robots.txt: {0}")]
    RobotstxtParseError(String),
}

// * Represents the crawl delay configuration for a domain
#[derive(Debug, Clone)]
pub struct CrawlDelayConfig {
    pub standard_delay_ms: u64,
    pub slow_path_delay_ms: u64,
}

impl Default for CrawlDelayConfig {
    fn default() -> Self {
        Self {
            standard_delay_ms: DEFAULT_CRAWL_DELAY_MS,
            slow_path_delay_ms: DEFAULT_CRAWL_DELAY_MS * SLOW_PATH_MULTIPLIER,
        }
    }
}

// * RobotstxtParser extracts Crawl-Delay from robots.txt content
pub struct RobotstxtParser {
    user_agent: String,
}

impl RobotstxtParser {
    pub fn new(user_agent: &str) -> Self {
        Self {
            user_agent: user_agent.to_string(),
        }
    }

    // * Parses robots.txt content and extracts Crawl-Delay for the configured user-agent
    pub fn parse_crawl_delay(&self, robots_txt: &str) -> CrawlDelayConfig {
        // * Parse Crawl-Delay directive manually since robotstxt crate focuses on Allow/Disallow
        let crawl_delay_ms = self.extract_crawl_delay(robots_txt);

        CrawlDelayConfig {
            standard_delay_ms: crawl_delay_ms,
            slow_path_delay_ms: crawl_delay_ms * SLOW_PATH_MULTIPLIER,
        }
    }

    // * Checks if a path is allowed for crawling
    pub fn is_allowed(&self, robots_txt: &str, path: &str) -> bool {
        let matcher = DefaultMatcher::default();
        matcher.one_agent_allowed_by_robots(robots_txt, &self.user_agent, path)
    }

    // * Extracts Crawl-Delay value from robots.txt content
    fn extract_crawl_delay(&self, robots_txt: &str) -> u64 {
        let mut in_matching_agent_block = false;
        let mut found_delay: Option<u64> = None;

        for line in robots_txt.lines() {
            let line = line.trim();

            // * Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let lowercase_line = line.to_lowercase();

            // * Check for User-agent directive
            if lowercase_line.starts_with("user-agent:") {
                let agent = line[11..].trim();
                // * Match wildcard or specific user-agent
                in_matching_agent_block =
                    agent == "*" || self.user_agent.to_lowercase().contains(&agent.to_lowercase());
            }

            // * Extract Crawl-Delay if in matching block
            if in_matching_agent_block && lowercase_line.starts_with("crawl-delay:") {
                if let Some(delay_str) = line.split(':').nth(1) {
                    if let Ok(delay) = delay_str.trim().parse::<f64>() {
                        // * Convert seconds to milliseconds
                        found_delay = Some((delay * 1000.0) as u64);
                    }
                }
            }
        }

        found_delay.unwrap_or(DEFAULT_CRAWL_DELAY_MS)
    }
}

// * Computes a hash of the domain for Redis key generation
fn compute_domain_hash(domain: &str) -> u64 {
    xxh64(domain.as_bytes(), 0)
}

// * DomainRateLimiter manages rate limiting for a single domain
pub struct DomainRateLimiter {
    domain: String,
    domain_hash: u64,
    config: CrawlDelayConfig,
    local_limiter: GovernorLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
    >,
}

impl DomainRateLimiter {
    pub fn new(domain: &str, config: CrawlDelayConfig) -> Self {
        let domain_hash = compute_domain_hash(domain);

        // * Create local governor rate limiter based on crawl delay
        let requests_per_second = if config.standard_delay_ms > 0 {
            std::cmp::max(1, 1000 / config.standard_delay_ms as u32)
        } else {
            1
        };

        let quota = Quota::per_second(NonZeroU32::new(requests_per_second).unwrap_or(nonzero!(1u32)));
        let local_limiter = GovernorLimiter::direct(quota);

        Self {
            domain: domain.to_string(),
            domain_hash,
            config,
            local_limiter,
        }
    }

    // * Returns the Redis key for this domain's rate limit bucket
    pub fn redis_key(&self) -> String {
        format!("{}:{}:bucket", RATELIMIT_PREFIX, self.domain_hash)
    }

    // * Returns the Redis key for this domain's blacklist entry
    pub fn blacklist_key(&self) -> String {
        format!("{}:{}", BLACKLIST_PREFIX, self.domain_hash)
    }

    // * Checks local rate limit (non-blocking)
    pub fn check_local(&self) -> bool {
        self.local_limiter.check().is_ok()
    }

    // * Waits for local rate limit to allow a request
    pub async fn wait_local(&self) {
        self.local_limiter.until_ready().await;
    }

    pub fn get_config(&self) -> &CrawlDelayConfig {
        &self.config
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }
}

// * RateLimitManager coordinates rate limiting across multiple domains with Redis backend
pub struct RateLimitManager {
    redis: Option<ConnectionManager>,
    domain_limiters: Arc<RwLock<HashMap<String, Arc<DomainRateLimiter>>>>,
    robots_parser: RobotstxtParser,
}

impl RateLimitManager {
    // * Creates a new RateLimitManager with optional Redis connection
    pub async fn new(redis_url: Option<&str>, user_agent: &str) -> Result<Self, RateLimitError> {
        let redis = if let Some(url) = redis_url {
            let client = redis::Client::open(url)?;
            Some(ConnectionManager::new(client).await?)
        } else {
            None
        };

        Ok(Self {
            redis,
            domain_limiters: Arc::new(RwLock::new(HashMap::new())),
            robots_parser: RobotstxtParser::new(user_agent),
        })
    }

    // * Registers a domain with its robots.txt content
    pub async fn register_domain(
        &self,
        domain: &str,
        robots_txt: Option<&str>,
    ) -> Arc<DomainRateLimiter> {
        let config = robots_txt
            .map(|txt| self.robots_parser.parse_crawl_delay(txt))
            .unwrap_or_default();

        let limiter = Arc::new(DomainRateLimiter::new(domain, config));

        let mut limiters = self.domain_limiters.write().await;
        limiters.insert(domain.to_string(), Arc::clone(&limiter));

        debug!(
            "Registered domain '{}' with delay {}ms (slow: {}ms)",
            domain,
            limiter.get_config().standard_delay_ms,
            limiter.get_config().slow_path_delay_ms
        );

        limiter
    }

    // * Gets or creates a rate limiter for a domain
    pub async fn get_limiter(&self, domain: &str) -> Arc<DomainRateLimiter> {
        let limiters = self.domain_limiters.read().await;
        if let Some(limiter) = limiters.get(domain) {
            return Arc::clone(limiter);
        }
        drop(limiters);

        // * Create default limiter if not registered
        self.register_domain(domain, None).await
    }

    // * Checks if a domain is blacklisted
    pub async fn is_blacklisted(&self, domain: &str) -> Result<bool, RateLimitError> {
        if let Some(mut redis) = self.redis.clone() {
            let key = format!("{}:{}", BLACKLIST_PREFIX, compute_domain_hash(domain));
            let exists: bool = redis.exists(&key).await?;
            return Ok(exists);
        }
        Ok(false)
    }

    // * Records an HTTP 429 response - sets 1 hour backoff
    pub async fn record_429(&self, domain: &str) -> Result<(), RateLimitError> {
        if let Some(mut redis) = self.redis.clone() {
            let key = format!("{}:{}:bucket", RATELIMIT_PREFIX, compute_domain_hash(domain));
            redis
                .set_ex::<_, _, ()>(&key, "backoff", BACKOFF_429_TTL_SECS)
                .await?;
            warn!(
                "Domain '{}' received 429 - backoff for {} seconds",
                domain, BACKOFF_429_TTL_SECS
            );
        }
        Ok(())
    }

    // * Records a Tier 2 proxy failure - blacklists domain for 24 hours
    pub async fn record_tier2_failure(&self, domain: &str) -> Result<(), RateLimitError> {
        if let Some(mut redis) = self.redis.clone() {
            let key = format!("{}:{}", BLACKLIST_PREFIX, compute_domain_hash(domain));
            redis
                .set_ex::<_, _, ()>(&key, "blacklisted", BLACKLIST_TTL_SECS)
                .await?;
            warn!(
                "Domain '{}' blacklisted for {} seconds after Tier 2 failure",
                domain, BLACKLIST_TTL_SECS
            );
        }
        Ok(())
    }

    // * Checks if crawling is allowed for a URL based on robots.txt
    pub fn is_crawl_allowed(&self, robots_txt: &str, path: &str) -> bool {
        self.robots_parser.is_allowed(robots_txt, path)
    }

    // * Acquires permission to make a request (checks blacklist and rate limit)
    pub async fn acquire(&self, domain: &str, is_slow_path: bool) -> Result<(), RateLimitError> {
        // * Check blacklist first
        if self.is_blacklisted(domain).await? {
            return Err(RateLimitError::DomainBlacklisted);
        }

        let limiter = self.get_limiter(domain).await;

        // * Wait for local rate limit
        limiter.wait_local().await;

        // * Apply additional delay for slow path
        if is_slow_path {
            let extra_delay = limiter.get_config().slow_path_delay_ms
                - limiter.get_config().standard_delay_ms;
            if extra_delay > 0 {
                tokio::time::sleep(Duration::from_millis(extra_delay)).await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_robotstxt_parser_extracts_crawl_delay() {
        let parser = RobotstxtParser::new("Mozilla/5.0 Chrome/120");
        let robots_txt = r#"
User-agent: *
Crawl-delay: 2
Disallow: /private/
"#;
        let config = parser.parse_crawl_delay(robots_txt);
        assert_eq!(config.standard_delay_ms, 2000);
        assert_eq!(config.slow_path_delay_ms, 4000);
    }

    #[test]
    fn test_robotstxt_parser_default_delay() {
        let parser = RobotstxtParser::new("Mozilla/5.0 Chrome/120");
        let robots_txt = r#"
User-agent: *
Disallow: /admin/
"#;
        let config = parser.parse_crawl_delay(robots_txt);
        assert_eq!(config.standard_delay_ms, DEFAULT_CRAWL_DELAY_MS);
    }

    #[test]
    fn test_robotstxt_is_allowed() {
        let parser = RobotstxtParser::new("Mozilla/5.0 Chrome/120");
        let robots_txt = r#"
User-agent: *
Disallow: /private/
Allow: /public/
"#;
        assert!(parser.is_allowed(robots_txt, "/public/page.html"));
        assert!(!parser.is_allowed(robots_txt, "/private/secret.html"));
    }

    #[test]
    fn test_domain_hash_consistency() {
        let hash1 = compute_domain_hash("example.com");
        let hash2 = compute_domain_hash("example.com");
        assert_eq!(hash1, hash2);

        let hash3 = compute_domain_hash("other.com");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_domain_rate_limiter_keys() {
        let config = CrawlDelayConfig::default();
        let limiter = DomainRateLimiter::new("example.com", config);

        assert!(limiter.redis_key().starts_with("ratelimit:"));
        assert!(limiter.blacklist_key().starts_with("blacklist:"));
    }

    #[tokio::test]
    async fn test_rate_limit_manager_without_redis() {
        let manager = RateLimitManager::new(None, "TestBot/1.0").await.unwrap();

        // * Should not be blacklisted without Redis
        assert!(!manager.is_blacklisted("example.com").await.unwrap());

        // * Should be able to acquire
        assert!(manager.acquire("example.com", false).await.is_ok());
    }

    #[tokio::test]
    async fn test_register_domain_with_robotstxt() {
        let manager = RateLimitManager::new(None, "TestBot/1.0").await.unwrap();
        let robots_txt = "User-agent: *\nCrawl-delay: 5";

        let limiter = manager.register_domain("example.com", Some(robots_txt)).await;
        assert_eq!(limiter.get_config().standard_delay_ms, 5000);
        assert_eq!(limiter.get_config().slow_path_delay_ms, 10000);
    }
}
