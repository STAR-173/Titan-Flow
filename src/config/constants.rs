// * [EDD-7] Configuration Constants
// * Central location for all configurable thresholds and timeouts

// * Page fetch timeout in milliseconds
pub const PAGE_TIMEOUT_MS: u64 = 60_000;

// * Maximum tokens per chunk for text processing
pub const CHUNK_TOKEN_THRESHOLD: usize = 2048;

// * Overlap rate between consecutive chunks (10%)
pub const OVERLAP_RATE: f64 = 0.1;

// * Minimum word count threshold for content validation
pub const MIN_WORD_THRESHOLD: usize = 1;

// * Minimum score for image relevance filtering
pub const IMAGE_SCORE_THRESHOLD: i32 = 2;

// * Maximum screenshot height in pixels before truncation
pub const SCREENSHOT_HEIGHT_THRESHOLD: u32 = 10_000;
