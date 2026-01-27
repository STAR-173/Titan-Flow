// * Milestone 4: The Refinery (Extraction Pipeline)
// * Goal: Extract text, tables, media metadata, and entities from HTML content.
// * This module provides a unified pipeline for processing crawled web content.

pub mod chunker;
pub mod content_cleaner;
pub mod metadata;
pub mod regex_extractor;
pub mod tables;

// * Re-exports for convenient access
pub use chunker::{chunk_text, chunk_text_with_window, ChunkerConfig, SlidingWindowChunker, TextChunk};
pub use content_cleaner::{extract_content, extract_text, CleanedContent, CleanerConfig, ContentCleaner};
pub use metadata::{MetadataExtractor, PageMetadata};
pub use regex_extractor::{EntityType, ExtractorConfig, ExtractionResult, RegexExtractor};
pub use tables::{ExtractedTable, TableScorer};

use serde::{Deserialize, Serialize};

/// Unified result from the refinery pipeline
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefineryResult {
    /// Cleaned and extracted main content
    pub content: CleanedContent,
    /// Page metadata (title, author, dates, etc.)
    pub metadata: PageMetadata,
    /// Extracted data tables
    pub tables: Vec<ExtractedTable>,
    /// Extracted entities (emails, URLs, dates, etc.)
    pub entities: ExtractionResult,
    /// Text chunks for embedding/processing
    pub chunks: Vec<TextChunk>,
    /// Processing statistics
    pub stats: RefineryStats,
}

impl RefineryResult {
    /// Converts result to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Converts result to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Processing statistics from the refinery
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefineryStats {
    pub word_count: usize,
    pub paragraph_count: usize,
    pub table_count: usize,
    pub entity_count: usize,
    pub chunk_count: usize,
    pub quality_score: f32,
    pub has_main_content: bool,
}

/// Configuration for the refinery pipeline
#[derive(Debug, Clone)]
pub struct RefineryConfig {
    /// Content cleaner configuration
    pub cleaner: CleanerConfig,
    /// Chunker configuration
    pub chunker: ChunkerConfig,
    /// Entity extractor configuration
    pub extractor: ExtractorConfig,
    /// Whether to extract tables
    pub extract_tables: bool,
    /// Whether to extract entities
    pub extract_entities: bool,
    /// Whether to generate chunks
    pub generate_chunks: bool,
}

impl Default for RefineryConfig {
    fn default() -> Self {
        Self {
            cleaner: CleanerConfig::default(),
            chunker: ChunkerConfig::default(),
            extractor: ExtractorConfig::default(),
            extract_tables: true,
            extract_entities: true,
            generate_chunks: true,
        }
    }
}

/// The main refinery pipeline for processing HTML content
///
/// # Example
/// ```ignore
/// use titan_flow::refinery::Refinery;
///
/// let refinery = Refinery::new();
/// let result = refinery.process(html_content);
///
/// println!("Title: {:?}", result.metadata.title);
/// println!("Word count: {}", result.stats.word_count);
/// println!("Tables found: {}", result.tables.len());
/// ```
pub struct Refinery {
    config: RefineryConfig,
    cleaner: ContentCleaner,
    chunker: SlidingWindowChunker,
    extractor: RegexExtractor,
}

impl Refinery {
    /// Creates a new refinery with default configuration
    pub fn new() -> Self {
        let config = RefineryConfig::default();
        Self {
            cleaner: ContentCleaner::with_config(config.cleaner.clone()),
            chunker: SlidingWindowChunker::with_config(config.chunker.clone()),
            extractor: RegexExtractor::with_config(config.extractor.clone()),
            config,
        }
    }

    /// Creates a new refinery with custom configuration
    pub fn with_config(config: RefineryConfig) -> Self {
        Self {
            cleaner: ContentCleaner::with_config(config.cleaner.clone()),
            chunker: SlidingWindowChunker::with_config(config.chunker.clone()),
            extractor: RegexExtractor::with_config(config.extractor.clone()),
            config,
        }
    }

    /// Processes HTML content through the full refinery pipeline
    ///
    /// # Pipeline Steps:
    /// 1. Extract and clean main content (remove boilerplate)
    /// 2. Extract page metadata (JSON-LD, meta tags, fallbacks)
    /// 3. Extract data tables (heuristic scoring)
    /// 4. Extract entities (regex patterns)
    /// 5. Generate text chunks (sliding window)
    pub fn process(&self, html: &str) -> RefineryResult {
        let mut result = RefineryResult::default();

        // * Step 1: Clean and extract main content
        result.content = self.cleaner.clean(html);

        // * Step 2: Extract metadata
        result.metadata = MetadataExtractor::extract(html);

        // * Step 3: Extract data tables
        if self.config.extract_tables {
            result.tables = TableScorer::extract_all_tables(html);
        }

        // * Step 4: Extract entities from cleaned text
        if self.config.extract_entities {
            result.entities = self.extractor.extract(&result.content.text);
        }

        // * Step 5: Generate chunks from cleaned text
        if self.config.generate_chunks && !result.content.text.is_empty() {
            result.chunks = self.chunker.chunk(&result.content.text);
        }

        // * Calculate statistics
        result.stats = RefineryStats {
            word_count: result.content.word_count,
            paragraph_count: result.content.paragraphs.len(),
            table_count: result.tables.len(),
            entity_count: result.entities.total_count,
            chunk_count: result.chunks.len(),
            quality_score: result.content.quality_score,
            has_main_content: result.content.found_main_content,
        };

        result
    }

    /// Processes only content extraction (skips tables, entities, chunks)
    pub fn process_content_only(&self, html: &str) -> CleanedContent {
        self.cleaner.clean(html)
    }

    /// Processes only metadata extraction
    pub fn process_metadata_only(&self, html: &str) -> PageMetadata {
        MetadataExtractor::extract(html)
    }

    /// Returns the current configuration
    pub fn config(&self) -> &RefineryConfig {
        &self.config
    }
}

impl Default for Refinery {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to process HTML with default settings
pub fn process_html(html: &str) -> RefineryResult {
    Refinery::new().process(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_html() -> &'static str {
        r#"
        <html lang="en">
        <head>
            <title>Sample Article - Test Site</title>
            <meta name="description" content="A sample article for testing">
            <meta name="author" content="Test Author">
            <meta property="og:title" content="Sample Article">
            <meta property="article:published_time" content="2024-01-15">
            <script type="application/ld+json">
            {
                "@type": "NewsArticle",
                "headline": "Sample Article Headline",
                "datePublished": "2024-01-15T10:00:00Z",
                "author": {"@type": "Person", "name": "John Doe"},
                "wordCount": 150
            }
            </script>
        </head>
        <body>
            <nav><a href="/">Home</a></nav>
            <article>
                <h1>Sample Article Headline</h1>
                <p>This is the first paragraph of our sample article with enough content to be meaningful. Contact us at test@example.com for more information about this topic.</p>
                <p>Published on 2024-01-15, this article discusses important topics that readers will find interesting and informative.</p>
                <h2>Data Section</h2>
                <table>
                    <thead><tr><th>Name</th><th>Value</th><th>Status</th></tr></thead>
                    <tbody>
                        <tr><td>Item A</td><td>100</td><td>Active</td></tr>
                        <tr><td>Item B</td><td>200</td><td>Pending</td></tr>
                    </tbody>
                </table>
                <p>Visit https://example.com for more resources and documentation about this subject matter.</p>
            </article>
            <footer>Copyright 2024</footer>
        </body>
        </html>
        "#
    }

    #[test]
    fn test_full_pipeline() {
        let refinery = Refinery::new();
        let result = refinery.process(sample_html());

        // * Check content extraction
        assert!(result.content.found_main_content);
        assert!(!result.content.text.is_empty());
        assert!(result.content.paragraphs.len() >= 2);

        // * Check metadata extraction
        assert!(result.metadata.title.is_some());
        assert!(result.metadata.date_published.is_some());
        assert!(result.metadata.author.is_some());

        // * Check table extraction
        assert!(!result.tables.is_empty());

        // * Check entity extraction
        assert!(result.entities.entities.contains_key("email"));
        assert!(result.entities.entities.contains_key("url"));

        // * Check chunks
        assert!(!result.chunks.is_empty());

        // * Check stats
        assert!(result.stats.word_count > 0);
        assert!(result.stats.quality_score > 0.0);
    }

    #[test]
    fn test_convenience_function() {
        let result = process_html(sample_html());

        assert!(result.content.found_main_content);
        assert!(result.metadata.title.is_some());
    }

    #[test]
    fn test_content_only_processing() {
        let refinery = Refinery::new();
        let content = refinery.process_content_only(sample_html());

        assert!(content.found_main_content);
        assert!(!content.text.is_empty());
    }

    #[test]
    fn test_metadata_only_processing() {
        let refinery = Refinery::new();
        let metadata = refinery.process_metadata_only(sample_html());

        assert!(metadata.title.is_some());
        assert!(metadata.author.is_some());
    }

    #[test]
    fn test_custom_config() {
        let config = RefineryConfig {
            extract_tables: false,
            extract_entities: false,
            generate_chunks: false,
            ..Default::default()
        };

        let refinery = Refinery::with_config(config);
        let result = refinery.process(sample_html());

        // * Tables, entities, and chunks should be empty
        assert!(result.tables.is_empty());
        assert!(result.entities.entities.is_empty());
        assert!(result.chunks.is_empty());

        // * But content and metadata should still work
        assert!(!result.content.text.is_empty());
        assert!(result.metadata.title.is_some());
    }

    #[test]
    fn test_result_serialization() {
        let result = process_html(sample_html());
        let json = result.to_json();

        assert!(json.contains("content"));
        assert!(json.contains("metadata"));
        assert!(json.contains("stats"));
    }

    #[test]
    fn test_empty_html() {
        let result = process_html("<html><body></body></html>");

        assert!(result.content.text.is_empty() || result.content.word_count == 0);
        assert!(result.chunks.is_empty());
    }

    #[test]
    fn test_stats_accuracy() {
        let result = process_html(sample_html());

        assert_eq!(result.stats.word_count, result.content.word_count);
        assert_eq!(result.stats.paragraph_count, result.content.paragraphs.len());
        assert_eq!(result.stats.table_count, result.tables.len());
        assert_eq!(result.stats.entity_count, result.entities.total_count);
        assert_eq!(result.stats.chunk_count, result.chunks.len());
    }
}
