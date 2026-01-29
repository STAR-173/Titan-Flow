// * [EDD-4] Link Intrinsic Scorer
// * Scores URLs based on intrinsic properties for crawl prioritization

use std::sync::LazyLock;
use regex::Regex;

// * Scoring constants from specification
const SCORE_MIN: f32 = 0.0;
const SCORE_MAX: f32 = 10.0;

// * Score adjustments
const SCORE_TITLE_LENGTH: f32 = 1.0;      // * +1.0 if title > 3 chars
const SCORE_NAV_KEYWORD: f32 = 1.5;       // * +1.5 for navigation-related keywords
const SCORE_AD_KEYWORD: f32 = -1.0;       // * -1.0 for ad-related keywords
const SCORE_DOCS_PATH: f32 = 2.0;         // * +2.0 for /docs/ path
const SCORE_API_PATH: f32 = 1.5;          // * +1.5 for /api/ path
const SCORE_BLOG_PATH: f32 = 1.0;         // * +1.0 for /blog/ path
const SCORE_ARTICLE_PATH: f32 = 1.0;      // * +1.0 for /article/ path
const SCORE_HTTPS: f32 = 0.5;             // * +0.5 for HTTPS
const SCORE_DEEP_PATH: f32 = -0.5;        // * -0.5 for each depth > 4
const SCORE_QUERY_PARAMS: f32 = -0.3;     // * -0.3 for query parameters
const SCORE_FRAGMENT: f32 = -0.5;         // * -0.5 for URL fragments

// * Title length threshold
const MIN_TITLE_LENGTH: usize = 3;

// * Maximum path depth before penalty
const MAX_PATH_DEPTH: usize = 4;

// * Precompiled regex patterns for keyword detection
static NAV_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(nav|menu|sidebar|header|footer|breadcrumb|navigation)\b").unwrap()
});

static AD_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(ad|ads|advert|banner|sponsor|promo|affiliate|tracking|click|impression)\b")
        .unwrap()
});

static SOCIAL_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(share|tweet|facebook|twitter|linkedin|pinterest|reddit)\b").unwrap()
});

static UTILITY_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(login|logout|signin|signout|register|signup|cart|checkout|account)\b")
        .unwrap()
});

static LEGAL_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(privacy|terms|cookie|gdpr|legal|disclaimer)\b").unwrap()
});

static HIGH_VALUE_KEYWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(documentation|guide|tutorial|learn|reference|manual|overview)\b").unwrap()
});

/// Scored link with URL and computed priority score
#[derive(Debug, Clone)]
pub struct ScoredLink {
    pub url: String,
    pub anchor_text: String,
    pub score: f32,
    pub breakdown: ScoreBreakdown,
}

impl ScoredLink {
    /// Creates a new scored link
    pub fn new(url: String, anchor_text: String) -> Self {
        let breakdown = ScoreBreakdown::default();
        Self {
            url,
            anchor_text,
            score: 5.0, // * Base score
            breakdown,
        }
    }
}

impl PartialEq for ScoredLink {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for ScoredLink {}

impl PartialOrd for ScoredLink {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredLink {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // * Higher scores should come first (reverse order)
        other
            .score
            .partial_cmp(&self.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Breakdown of score components for debugging
#[derive(Debug, Clone, Default)]
pub struct ScoreBreakdown {
    pub base_score: f32,
    pub title_bonus: f32,
    pub nav_bonus: f32,
    pub ad_penalty: f32,
    pub path_bonus: f32,
    pub https_bonus: f32,
    pub depth_penalty: f32,
    pub query_penalty: f32,
    pub fragment_penalty: f32,
    pub keyword_bonus: f32,
    pub final_score: f32,
}

/// Link scorer for prioritizing crawl queue
#[derive(Debug, Clone)]
pub struct LinkScorer {
    config: ScorerConfig,
}

/// Configuration for link scoring
#[derive(Debug, Clone)]
pub struct ScorerConfig {
    /// Base score for all links
    pub base_score: f32,
    /// Whether to apply HTTPS bonus
    pub reward_https: bool,
    /// Whether to penalize deep paths
    pub penalize_deep_paths: bool,
    /// Whether to penalize query parameters
    pub penalize_query_params: bool,
    /// Custom high-value path patterns
    pub high_value_paths: Vec<String>,
    /// Custom low-value path patterns
    pub low_value_paths: Vec<String>,
}

impl Default for ScorerConfig {
    fn default() -> Self {
        Self {
            base_score: 5.0,
            reward_https: true,
            penalize_deep_paths: true,
            penalize_query_params: true,
            high_value_paths: vec![
                "/docs/".to_string(),
                "/documentation/".to_string(),
                "/api/".to_string(),
                "/guide/".to_string(),
                "/tutorial/".to_string(),
                "/blog/".to_string(),
                "/article/".to_string(),
                "/learn/".to_string(),
            ],
            low_value_paths: vec![
                "/ads/".to_string(),
                "/ad/".to_string(),
                "/banner/".to_string(),
                "/tracking/".to_string(),
                "/click/".to_string(),
                "/redirect/".to_string(),
            ],
        }
    }
}

impl LinkScorer {
    /// Creates a new link scorer with default configuration
    pub fn new() -> Self {
        Self {
            config: ScorerConfig::default(),
        }
    }

    /// Creates a new link scorer with custom configuration
    pub fn with_config(config: ScorerConfig) -> Self {
        Self { config }
    }

    /// Scores a single link
    pub fn score(&self, url: &str, anchor_text: &str) -> ScoredLink {
        let mut breakdown = ScoreBreakdown {
            base_score: self.config.base_score,
            ..Default::default()
        };

        let mut score = self.config.base_score;

        // * Parse URL for analysis
        let parsed = url::Url::parse(url);

        // * Score based on anchor text
        score += self.score_anchor_text(anchor_text, &mut breakdown);

        // * Score based on URL structure
        if let Ok(parsed_url) = parsed {
            score += self.score_url_structure(&parsed_url, &mut breakdown);
        }

        // * Score based on keyword analysis
        score += self.score_keywords(url, anchor_text, &mut breakdown);

        // * Clamp to valid range
        breakdown.final_score = score.clamp(SCORE_MIN, SCORE_MAX);

        ScoredLink {
            url: url.to_string(),
            anchor_text: anchor_text.to_string(),
            score: breakdown.final_score,
            breakdown,
        }
    }

    /// Scores multiple links and returns them sorted by priority
    pub fn score_batch(&self, links: &[(String, String)]) -> Vec<ScoredLink> {
        let mut scored: Vec<ScoredLink> = links
            .iter()
            .map(|(url, text)| self.score(url, text))
            .collect();

        scored.sort();
        scored
    }

    /// Scores based on anchor text properties
    fn score_anchor_text(&self, anchor_text: &str, breakdown: &mut ScoreBreakdown) -> f32 {
        let mut score = 0.0;
        let text = anchor_text.trim();

        // * Title length bonus
        if text.len() > MIN_TITLE_LENGTH {
            score += SCORE_TITLE_LENGTH;
            breakdown.title_bonus = SCORE_TITLE_LENGTH;
        }

        // * Navigation keyword detection
        if NAV_KEYWORDS.is_match(text) {
            score += SCORE_NAV_KEYWORD;
            breakdown.nav_bonus = SCORE_NAV_KEYWORD;
        }

        // * Ad keyword penalty
        if AD_KEYWORDS.is_match(text) {
            score += SCORE_AD_KEYWORD;
            breakdown.ad_penalty = SCORE_AD_KEYWORD;
        }

        score
    }

    /// Scores based on URL structure
    fn score_url_structure(&self, url: &url::Url, breakdown: &mut ScoreBreakdown) -> f32 {
        let mut score = 0.0;

        // * HTTPS bonus
        if self.config.reward_https && url.scheme() == "https" {
            score += SCORE_HTTPS;
            breakdown.https_bonus = SCORE_HTTPS;
        }

        // * Path analysis
        let path = url.path().to_lowercase();

        // * High-value path bonus
        if path.contains("/docs/") || path.contains("/documentation/") {
            score += SCORE_DOCS_PATH;
            breakdown.path_bonus += SCORE_DOCS_PATH;
        } else if path.contains("/api/") {
            score += SCORE_API_PATH;
            breakdown.path_bonus += SCORE_API_PATH;
        } else if path.contains("/blog/") {
            score += SCORE_BLOG_PATH;
            breakdown.path_bonus += SCORE_BLOG_PATH;
        } else if path.contains("/article/") {
            score += SCORE_ARTICLE_PATH;
            breakdown.path_bonus += SCORE_ARTICLE_PATH;
        }

        // * Custom high-value paths
        for pattern in &self.config.high_value_paths {
            if path.contains(&pattern.to_lowercase()) && breakdown.path_bonus < SCORE_DOCS_PATH {
                score += 1.0;
                breakdown.path_bonus += 1.0;
                break;
            }
        }

        // * Custom low-value paths
        for pattern in &self.config.low_value_paths {
            if path.contains(&pattern.to_lowercase()) {
                score -= 1.0;
                breakdown.path_bonus -= 1.0;
                break;
            }
        }

        // * Path depth penalty
        if self.config.penalize_deep_paths {
            let depth = path.matches('/').count();
            if depth > MAX_PATH_DEPTH {
                let penalty = (depth - MAX_PATH_DEPTH) as f32 * SCORE_DEEP_PATH;
                score += penalty;
                breakdown.depth_penalty = penalty;
            }
        }

        // * Query parameter penalty
        if self.config.penalize_query_params && url.query().is_some() {
            score += SCORE_QUERY_PARAMS;
            breakdown.query_penalty = SCORE_QUERY_PARAMS;
        }

        // * Fragment penalty
        if url.fragment().is_some() {
            score += SCORE_FRAGMENT;
            breakdown.fragment_penalty = SCORE_FRAGMENT;
        }

        score
    }

    /// Scores based on keyword analysis
    fn score_keywords(&self, url: &str, anchor_text: &str, breakdown: &mut ScoreBreakdown) -> f32 {
        let mut score = 0.0;
        let combined = format!("{} {}", url, anchor_text);

        // * High-value keyword bonus
        if HIGH_VALUE_KEYWORDS.is_match(&combined) {
            score += 1.0;
            breakdown.keyword_bonus += 1.0;
        }

        // * Social media penalty (usually not content)
        if SOCIAL_KEYWORDS.is_match(&combined) {
            score -= 0.5;
            breakdown.keyword_bonus -= 0.5;
        }

        // * Utility page penalty (usually not crawl-worthy)
        if UTILITY_KEYWORDS.is_match(&combined) {
            score -= 0.5;
            breakdown.keyword_bonus -= 0.5;
        }

        // * Legal page slight penalty (lower priority)
        if LEGAL_KEYWORDS.is_match(&combined) {
            score -= 0.3;
            breakdown.keyword_bonus -= 0.3;
        }

        score
    }

    /// Returns the current configuration
    pub fn config(&self) -> &ScorerConfig {
        &self.config
    }
}

impl Default for LinkScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// Priority queue wrapper for scored links
#[derive(Debug)]
pub struct PriorityLinkQueue {
    links: Vec<ScoredLink>,
    scorer: LinkScorer,
    capacity: usize,
}

impl PriorityLinkQueue {
    /// Creates a new priority queue with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            links: Vec::with_capacity(capacity),
            scorer: LinkScorer::new(),
            capacity,
        }
    }

    /// Creates a queue with custom scorer
    pub fn with_scorer(capacity: usize, scorer: LinkScorer) -> Self {
        Self {
            links: Vec::with_capacity(capacity),
            scorer,
            capacity,
        }
    }

    /// Adds a link to the queue with scoring
    pub fn push(&mut self, url: &str, anchor_text: &str) -> bool {
        if self.links.len() >= self.capacity {
            return false;
        }

        let scored = self.scorer.score(url, anchor_text);
        self.links.push(scored);
        self.links.sort();
        true
    }

    /// Removes and returns the highest priority link
    pub fn pop(&mut self) -> Option<ScoredLink> {
        if self.links.is_empty() {
            None
        } else {
            Some(self.links.remove(0))
        }
    }

    /// Peeks at the highest priority link without removing
    pub fn peek(&self) -> Option<&ScoredLink> {
        self.links.first()
    }

    /// Returns the number of links in the queue
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// Returns true if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Returns true if the queue is at capacity
    pub fn is_full(&self) -> bool {
        self.links.len() >= self.capacity
    }

    /// Clears all links from the queue
    pub fn clear(&mut self) {
        self.links.clear();
    }

    /// Drains all links from the queue in priority order
    pub fn drain(&mut self) -> Vec<ScoredLink> {
        std::mem::take(&mut self.links)
    }
}

/// Convenience function to score a single link
pub fn score_link(url: &str, anchor_text: &str) -> ScoredLink {
    LinkScorer::new().score(url, anchor_text)
}

/// Convenience function to score and sort multiple links
pub fn score_links(links: &[(String, String)]) -> Vec<ScoredLink> {
    LinkScorer::new().score_batch(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scoring() {
        let scorer = LinkScorer::new();
        let scored = scorer.score("https://example.com/docs/guide", "Documentation Guide");

        assert!(scored.score > 5.0, "Docs link should have bonus: {}", scored.score);
        assert!(scored.breakdown.path_bonus > 0.0);
        assert!(scored.breakdown.title_bonus > 0.0);
    }

    #[test]
    fn test_title_length_bonus() {
        let scorer = LinkScorer::new();

        let long_title = scorer.score("https://example.com", "Click here to learn more");
        let short_title = scorer.score("https://example.com", "Go");

        assert!(
            long_title.breakdown.title_bonus > short_title.breakdown.title_bonus,
            "Longer title should get bonus"
        );
    }

    #[test]
    fn test_nav_keyword_bonus() {
        let scorer = LinkScorer::new();

        let nav_link = scorer.score("https://example.com/nav", "Navigation Menu");
        let regular_link = scorer.score("https://example.com/page", "Regular Page");

        assert!(
            nav_link.breakdown.nav_bonus > regular_link.breakdown.nav_bonus,
            "Nav keyword should get bonus"
        );
    }

    #[test]
    fn test_ad_keyword_penalty() {
        let scorer = LinkScorer::new();

        let ad_link = scorer.score("https://example.com/ad/banner", "Sponsored Ad");
        let regular_link = scorer.score("https://example.com/page", "Regular Content");

        assert!(
            ad_link.score < regular_link.score,
            "Ad link should have lower score"
        );
    }

    #[test]
    fn test_docs_path_bonus() {
        let scorer = LinkScorer::new();

        let docs_link = scorer.score("https://example.com/docs/api", "API Reference");
        let regular_link = scorer.score("https://example.com/about", "About Us");

        assert!(
            docs_link.breakdown.path_bonus > regular_link.breakdown.path_bonus,
            "Docs path should get bonus"
        );
    }

    #[test]
    fn test_https_bonus() {
        let scorer = LinkScorer::new();

        let https_link = scorer.score("https://example.com", "Secure Site");
        let http_link = scorer.score("http://example.com", "Insecure Site");

        assert!(
            https_link.breakdown.https_bonus > http_link.breakdown.https_bonus,
            "HTTPS should get bonus"
        );
    }

    #[test]
    fn test_deep_path_penalty() {
        let scorer = LinkScorer::new();

        let deep_path = scorer.score("https://example.com/a/b/c/d/e/f/g", "Deep Page");
        let shallow_path = scorer.score("https://example.com/page", "Shallow Page");

        assert!(
            deep_path.breakdown.depth_penalty < 0.0,
            "Deep path should have penalty"
        );
        assert!(
            deep_path.score < shallow_path.score,
            "Deep path should have lower score"
        );
    }

    #[test]
    fn test_query_param_penalty() {
        let scorer = LinkScorer::new();

        let query_link = scorer.score("https://example.com/page?id=123&ref=abc", "Query Page");
        let clean_link = scorer.score("https://example.com/page", "Clean Page");

        assert!(
            query_link.breakdown.query_penalty < 0.0,
            "Query params should have penalty"
        );
        assert!(query_link.score < clean_link.score);
    }

    #[test]
    fn test_fragment_penalty() {
        let scorer = LinkScorer::new();

        let fragment_link = scorer.score("https://example.com/page#section", "Fragment Link");
        let _clean_link = scorer.score("https://example.com/page", "Clean Link");

        assert!(
            fragment_link.breakdown.fragment_penalty < 0.0,
            "Fragment should have penalty"
        );
    }

    #[test]
    fn test_score_clamping() {
        let scorer = LinkScorer::new();

        // * Very bad link (many penalties)
        let bad_link = scorer.score(
            "http://example.com/ad/tracking/click?ref=spam&track=1#footer",
            "Ad",
        );
        assert!(bad_link.score >= SCORE_MIN, "Score should not go below minimum");

        // * Very good link (many bonuses)
        let good_link = scorer.score(
            "https://example.com/docs/api/guide",
            "Comprehensive API Documentation Guide and Tutorial",
        );
        assert!(good_link.score <= SCORE_MAX, "Score should not exceed maximum");
    }

    #[test]
    fn test_batch_scoring() {
        let scorer = LinkScorer::new();

        let links = vec![
            ("https://example.com/docs/guide".to_string(), "Documentation".to_string()),
            ("http://example.com/ad".to_string(), "Ad".to_string()),
            ("https://example.com/blog".to_string(), "Blog Post".to_string()),
        ];

        let scored = scorer.score_batch(&links);

        // * Should be sorted by score (highest first)
        assert!(scored[0].score >= scored[1].score);
        assert!(scored[1].score >= scored[2].score);
    }

    #[test]
    fn test_priority_queue() {
        let mut queue = PriorityLinkQueue::new(10);

        queue.push("http://example.com/ad", "Ad");
        queue.push("https://example.com/docs", "Documentation");
        queue.push("https://example.com/blog", "Blog");

        // * Highest priority should come out first
        let first = queue.pop().unwrap();
        assert!(
            first.url.contains("docs"),
            "Docs should have highest priority"
        );

        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_priority_queue_capacity() {
        let mut queue = PriorityLinkQueue::new(2);

        assert!(queue.push("https://example.com/1", "Link 1"));
        assert!(queue.push("https://example.com/2", "Link 2"));
        assert!(!queue.push("https://example.com/3", "Link 3")); // * Over capacity

        assert!(queue.is_full());
    }

    #[test]
    fn test_scored_link_ordering() {
        let high = ScoredLink {
            url: "high".to_string(),
            anchor_text: "".to_string(),
            score: 8.0,
            breakdown: ScoreBreakdown::default(),
        };

        let low = ScoredLink {
            url: "low".to_string(),
            anchor_text: "".to_string(),
            score: 3.0,
            breakdown: ScoreBreakdown::default(),
        };

        assert!(high < low, "Higher score should sort first (lower in ordering)");
    }

    #[test]
    fn test_convenience_functions() {
        let single = score_link("https://example.com/docs", "Docs");
        assert!(single.score > 0.0);

        let batch = score_links(&[
            ("https://example.com/a".to_string(), "A".to_string()),
            ("https://example.com/b".to_string(), "B".to_string()),
        ]);
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_custom_config() {
        let config = ScorerConfig {
            base_score: 3.0,
            reward_https: false,
            penalize_deep_paths: false,
            penalize_query_params: false,
            high_value_paths: vec!["/custom/".to_string()],
            low_value_paths: vec!["/bad/".to_string()],
        };

        let scorer = LinkScorer::with_config(config);

        let custom_high = scorer.score("http://example.com/custom/page", "Custom");
        let custom_low = scorer.score("http://example.com/bad/page", "Bad");

        assert!(
            custom_high.score > custom_low.score,
            "Custom paths should be respected"
        );
    }

    #[test]
    fn test_high_value_keywords() {
        let scorer = LinkScorer::new();

        let guide = scorer.score("https://example.com/learn", "Getting Started Tutorial");
        let random = scorer.score("https://example.com/random", "Random Page");

        assert!(
            guide.breakdown.keyword_bonus > random.breakdown.keyword_bonus,
            "High-value keywords should get bonus"
        );
    }

    #[test]
    fn test_social_keyword_penalty() {
        let scorer = LinkScorer::new();

        let social = scorer.score("https://example.com/share", "Share on Twitter");
        let regular = scorer.score("https://example.com/page", "Regular Content");

        assert!(
            social.breakdown.keyword_bonus < regular.breakdown.keyword_bonus,
            "Social keywords should get penalty"
        );
    }
}
