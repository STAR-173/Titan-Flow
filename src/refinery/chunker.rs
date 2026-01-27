// * Milestone 4 - Task 4.4: Sliding Window Chunker [EDD-5.3]
// * Token-aware text chunking using Unicode word segmentation.
// * Ported from crawl4ai/utils.py (merge_chunks / chunk_documents)

use crate::config::constants::{CHUNK_TOKEN_THRESHOLD, OVERLAP_RATE};
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// Represents a text chunk with metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextChunk {
    pub content: String,
    pub word_count: usize,
    pub start_index: usize,
    pub end_index: usize,
    pub chunk_index: usize,
}

/// Configuration for the sliding window chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Maximum words per chunk (default: CHUNK_TOKEN_THRESHOLD = 2048)
    pub window_size: usize,
    /// Number of words to overlap between chunks (default: window_size * OVERLAP_RATE)
    pub overlap: usize,
    /// Minimum words for a valid chunk (avoids tiny trailing chunks)
    pub min_chunk_size: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        let window_size = CHUNK_TOKEN_THRESHOLD;
        let overlap = (window_size as f64 * OVERLAP_RATE) as usize;

        Self {
            window_size,
            overlap,
            min_chunk_size: 50, // * Avoid very small trailing chunks
        }
    }
}

impl ChunkerConfig {
    /// Creates a new config with specified window size
    /// Overlap is calculated as 10% of window size
    pub fn with_window_size(window_size: usize) -> Self {
        let overlap = (window_size as f64 * OVERLAP_RATE) as usize;
        Self {
            window_size,
            overlap,
            min_chunk_size: 50,
        }
    }

    /// Creates a new config with explicit window and overlap
    pub fn new(window_size: usize, overlap: usize, min_chunk_size: usize) -> Self {
        Self {
            window_size,
            overlap,
            min_chunk_size,
        }
    }
}

/// Sliding window text chunker with Unicode-aware word segmentation
pub struct SlidingWindowChunker {
    config: ChunkerConfig,
}

impl SlidingWindowChunker {
    /// Creates a new chunker with default configuration
    /// Default: window=2048 words, overlap=200 words (10%)
    pub fn new() -> Self {
        Self {
            config: ChunkerConfig::default(),
        }
    }

    /// Creates a new chunker with custom configuration
    pub fn with_config(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Chunks text into overlapping windows using Unicode word segmentation
    ///
    /// # Arguments
    /// * `text` - The input text to chunk
    ///
    /// # Returns
    /// Vector of text chunks with metadata
    pub fn chunk(&self, text: &str) -> Vec<TextChunk> {
        // * Use Unicode word segmentation (handles CJK, emoji, etc.)
        let words: Vec<&str> = text.unicode_words().collect();
        let total_words = words.len();

        // * If text fits in single chunk, return as-is
        if total_words <= self.config.window_size {
            return vec![TextChunk {
                content: text.to_string(),
                word_count: total_words,
                start_index: 0,
                end_index: total_words,
                chunk_index: 0,
            }];
        }

        let mut chunks: Vec<TextChunk> = Vec::new();
        let mut start = 0;
        let mut chunk_index = 0;

        while start < total_words {
            let end = (start + self.config.window_size).min(total_words);
            let chunk_words = &words[start..end];
            let word_count = chunk_words.len();
            let is_final_chunk = end >= total_words;

            // * Only merge small TRAILING chunks (not full-sized chunks in the middle)
            // * This prevents tiny fragments at the end while allowing normal chunking
            if is_final_chunk && word_count < self.config.min_chunk_size && !chunks.is_empty() {
                // * Merge with previous chunk if possible
                if let Some(last_chunk) = chunks.last_mut() {
                    let additional_content = chunk_words.join(" ");
                    last_chunk.content.push(' ');
                    last_chunk.content.push_str(&additional_content);
                    last_chunk.word_count += word_count;
                    last_chunk.end_index = end;
                }
                break;
            }

            chunks.push(TextChunk {
                content: chunk_words.join(" "),
                word_count,
                start_index: start,
                end_index: end,
                chunk_index,
            });

            // * Move to next window position with overlap
            if is_final_chunk {
                break;
            }

            // * Calculate step size (window - overlap)
            let step = self.config.window_size.saturating_sub(self.config.overlap);
            start += step.max(1); // * Ensure we always progress
            chunk_index += 1;
        }

        chunks
    }

    /// Chunks text and returns only the content strings (simplified API)
    pub fn chunk_simple(&self, text: &str) -> Vec<String> {
        self.chunk(text).into_iter().map(|c| c.content).collect()
    }

    /// Estimates the number of chunks for a given text without full processing
    pub fn estimate_chunk_count(&self, text: &str) -> usize {
        let word_count = text.unicode_words().count();

        if word_count <= self.config.window_size {
            return 1;
        }

        let step = self.config.window_size.saturating_sub(self.config.overlap);
        let remaining_after_first = word_count.saturating_sub(self.config.window_size);

        1 + (remaining_after_first + step - 1) / step
    }

    /// Returns the current configuration
    pub fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

impl Default for SlidingWindowChunker {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility function for quick chunking with default settings
pub fn chunk_text(text: &str) -> Vec<String> {
    SlidingWindowChunker::new().chunk_simple(text)
}

/// Utility function for quick chunking with custom window size
pub fn chunk_text_with_window(text: &str, window_size: usize) -> Vec<String> {
    let config = ChunkerConfig::with_window_size(window_size);
    SlidingWindowChunker::with_config(config).chunk_simple(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_text(word_count: usize) -> String {
        (0..word_count)
            .map(|i| format!("word{}", i))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_small_text_single_chunk() {
        let chunker = SlidingWindowChunker::new();
        let text = "This is a small piece of text that fits in one chunk.";

        let chunks = chunker.chunk(text);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, text);
        assert_eq!(chunks[0].chunk_index, 0);
    }

    #[test]
    fn test_exact_window_size() {
        let config = ChunkerConfig::new(10, 2, 3);
        let chunker = SlidingWindowChunker::with_config(config);
        let text = generate_text(10);

        let chunks = chunker.chunk(&text);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].word_count, 10);
    }

    #[test]
    fn test_sliding_window_overlap() {
        let config = ChunkerConfig::new(10, 3, 3);
        let chunker = SlidingWindowChunker::with_config(config);
        // * 20 words: chunk1 = 0-10, chunk2 = 7-17, chunk3 = 14-20
        let text = generate_text(20);

        let chunks = chunker.chunk(&text);

        assert!(chunks.len() >= 2);

        // * Verify overlap: end of chunk 0 should overlap with start of chunk 1
        assert!(chunks[0].end_index > chunks[1].start_index);
    }

    #[test]
    fn test_chunk_metadata() {
        let config = ChunkerConfig::new(5, 1, 2);
        let chunker = SlidingWindowChunker::with_config(config);
        let text = generate_text(12);

        let chunks = chunker.chunk(&text);

        // * Check indices are correct
        assert_eq!(chunks[0].start_index, 0);
        assert_eq!(chunks[0].end_index, 5);
        assert_eq!(chunks[0].chunk_index, 0);

        // * Second chunk should start at window - overlap = 5 - 1 = 4
        assert_eq!(chunks[1].start_index, 4);
        assert_eq!(chunks[1].chunk_index, 1);
    }

    #[test]
    fn test_unicode_segmentation() {
        let chunker = SlidingWindowChunker::new();

        // * Test with various Unicode text
        let text = "Hello 世界 こんにちは مرحبا Привет 🌍 emoji";
        let chunks = chunker.chunk(text);

        assert_eq!(chunks.len(), 1);
        // * Unicode words should be properly counted
        assert!(chunks[0].word_count >= 7);
    }

    #[test]
    fn test_chunk_simple() {
        let config = ChunkerConfig::new(5, 1, 2);
        let chunker = SlidingWindowChunker::with_config(config);
        let text = generate_text(10);

        let simple_chunks = chunker.chunk_simple(&text);
        let detailed_chunks = chunker.chunk(&text);

        assert_eq!(simple_chunks.len(), detailed_chunks.len());
        for (simple, detailed) in simple_chunks.iter().zip(detailed_chunks.iter()) {
            assert_eq!(simple, &detailed.content);
        }
    }

    #[test]
    fn test_estimate_chunk_count() {
        let config = ChunkerConfig::new(100, 20, 10);
        let chunker = SlidingWindowChunker::with_config(config);

        // * Small text
        let small = generate_text(50);
        assert_eq!(chunker.estimate_chunk_count(&small), 1);

        // * Exact window
        let exact = generate_text(100);
        assert_eq!(chunker.estimate_chunk_count(&exact), 1);

        // * Larger text
        let large = generate_text(250);
        let estimated = chunker.estimate_chunk_count(&large);
        let actual = chunker.chunk(&large).len();

        // * Estimate should be close to actual
        assert!((estimated as i32 - actual as i32).abs() <= 1);
    }

    #[test]
    fn test_default_config_values() {
        let config = ChunkerConfig::default();

        assert_eq!(config.window_size, CHUNK_TOKEN_THRESHOLD);
        assert_eq!(
            config.overlap,
            (CHUNK_TOKEN_THRESHOLD as f64 * OVERLAP_RATE) as usize
        );
    }

    #[test]
    fn test_utility_functions() {
        let text = generate_text(50);

        let default_chunks = chunk_text(&text);
        let custom_chunks = chunk_text_with_window(&text, 25);

        assert_eq!(default_chunks.len(), 1); // * 50 < 2048
        assert!(custom_chunks.len() >= 2); // * 50 > 25
    }

    #[test]
    fn test_min_chunk_size_merge() {
        let config = ChunkerConfig::new(10, 2, 5);
        let chunker = SlidingWindowChunker::with_config(config);
        // * 13 words: chunk1 = 10, remainder = 3 (< min_chunk_size)
        // * Should merge remainder into last chunk
        let text = generate_text(13);

        let chunks = chunker.chunk(&text);

        // * Last chunk should have merged the small remainder
        let last = chunks.last().unwrap();
        assert!(last.word_count >= 5);
    }

    #[test]
    fn test_empty_text() {
        let chunker = SlidingWindowChunker::new();
        let chunks = chunker.chunk("");

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.is_empty());
        assert_eq!(chunks[0].word_count, 0);
    }

    #[test]
    fn test_whitespace_only_text() {
        let chunker = SlidingWindowChunker::new();
        let chunks = chunker.chunk("   \n\t   ");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].word_count, 0);
    }
}
