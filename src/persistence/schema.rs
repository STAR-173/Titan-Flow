// * [PRD-4] [EDD-6] LanceDB Schema for Multimodal Web Objects
// * Defines the core data structures for vector database persistence

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// * Embedding dimension for vector search (768-dim as per spec)
pub const EMBEDDING_DIM: usize = 768;

// * Sentiment score bounds
pub const SENTIMENT_MIN: f32 = -1.0;
pub const SENTIMENT_MAX: f32 = 1.0;

/// Primary record structure for LanceDB storage
///
/// # Fields
/// - `id`: Unique identifier (UUID v4 format)
/// - `url`: Normalized URL (from engine/normalization.rs)
/// - `content_hash`: xxHash64 fingerprint for deduplication
/// - `media_json`: Serialized media references (images, videos)
/// - `embedding`: 768-dimensional vector for semantic search
/// - `sentiment_score`: Sentiment analysis result (-1.0 to 1.0)
/// - `is_deleted`: Soft deletion flag
/// - `created_at`: Record creation timestamp
/// - `updated_at`: Last modification timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalRecord {
    // * Core identifiers
    pub id: String,
    pub url: String,
    pub content_hash: u64,

    // * Content fields
    pub title: Option<String>,
    pub text_content: String,
    pub media_json: String,

    // * AI enrichment fields (nullable until processed)
    pub embedding: Option<Vec<f32>>,
    pub sentiment_score: Option<f32>,

    // * Metadata
    pub word_count: u32,
    pub chunk_count: u32,
    pub quality_score: f32,

    // * Lifecycle
    pub is_deleted: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl MultimodalRecord {
    /// Creates a new record with generated UUID and timestamps
    pub fn new(url: String, content_hash: u64, text_content: String) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_uuid(),
            url,
            content_hash,
            title: None,
            text_content,
            media_json: "[]".to_string(),
            embedding: None,
            sentiment_score: None,
            word_count: 0,
            chunk_count: 0,
            quality_score: 0.0,
            is_deleted: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Creates a record builder for fluent construction
    pub fn builder(url: String, content_hash: u64, text_content: String) -> MultimodalRecordBuilder {
        MultimodalRecordBuilder::new(url, content_hash, text_content)
    }

    /// Checks if the record needs AI enrichment
    pub fn needs_enrichment(&self) -> bool {
        self.embedding.is_none() || self.sentiment_score.is_none()
    }

    /// Updates the record timestamps
    pub fn touch(&mut self) {
        self.updated_at = current_timestamp();
    }

    /// Soft deletes the record
    pub fn soft_delete(&mut self) {
        self.is_deleted = true;
        self.touch();
    }

    /// Sets the embedding vector with validation
    pub fn set_embedding(&mut self, embedding: Vec<f32>) -> Result<(), SchemaError> {
        if embedding.len() != EMBEDDING_DIM {
            return Err(SchemaError::InvalidEmbeddingDimension {
                expected: EMBEDDING_DIM,
                actual: embedding.len(),
            });
        }
        self.embedding = Some(embedding);
        self.touch();
        Ok(())
    }

    /// Sets the sentiment score with clamping
    pub fn set_sentiment(&mut self, score: f32) {
        self.sentiment_score = Some(score.clamp(SENTIMENT_MIN, SENTIMENT_MAX));
        self.touch();
    }

    /// Converts to JSON string for serialization
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl Default for MultimodalRecord {
    fn default() -> Self {
        let now = current_timestamp();
        Self {
            id: generate_uuid(),
            url: String::new(),
            content_hash: 0,
            title: None,
            text_content: String::new(),
            media_json: "[]".to_string(),
            embedding: None,
            sentiment_score: None,
            word_count: 0,
            chunk_count: 0,
            quality_score: 0.0,
            is_deleted: false,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Builder pattern for MultimodalRecord construction
#[derive(Debug, Clone)]
pub struct MultimodalRecordBuilder {
    record: MultimodalRecord,
}

impl MultimodalRecordBuilder {
    pub fn new(url: String, content_hash: u64, text_content: String) -> Self {
        Self {
            record: MultimodalRecord::new(url, content_hash, text_content),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.record.title = Some(title.into());
        self
    }

    pub fn media_json(mut self, json: impl Into<String>) -> Self {
        self.record.media_json = json.into();
        self
    }

    pub fn embedding(mut self, embedding: Vec<f32>) -> Self {
        // * Silent validation - invalid embeddings are ignored
        if embedding.len() == EMBEDDING_DIM {
            self.record.embedding = Some(embedding);
        }
        self
    }

    pub fn sentiment_score(mut self, score: f32) -> Self {
        self.record.sentiment_score = Some(score.clamp(SENTIMENT_MIN, SENTIMENT_MAX));
        self
    }

    pub fn word_count(mut self, count: u32) -> Self {
        self.record.word_count = count;
        self
    }

    pub fn chunk_count(mut self, count: u32) -> Self {
        self.record.chunk_count = count;
        self
    }

    pub fn quality_score(mut self, score: f32) -> Self {
        self.record.quality_score = score.clamp(0.0, 1.0);
        self
    }

    pub fn build(self) -> MultimodalRecord {
        self.record
    }
}

/// Media reference structure for storing in media_json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaReference {
    pub media_type: MediaType,
    pub url: String,
    pub alt_text: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size: Option<u64>,
    pub s3_path: Option<String>,
}

impl MediaReference {
    pub fn image(url: String) -> Self {
        Self {
            media_type: MediaType::Image,
            url,
            alt_text: None,
            width: None,
            height: None,
            file_size: None,
            s3_path: None,
        }
    }

    pub fn video(url: String) -> Self {
        Self {
            media_type: MediaType::Video,
            url,
            alt_text: None,
            width: None,
            height: None,
            file_size: None,
            s3_path: None,
        }
    }
}

/// Supported media types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Document,
}

/// Schema-related errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum SchemaError {
    #[error("Invalid embedding dimension: expected {expected}, got {actual}")]
    InvalidEmbeddingDimension { expected: usize, actual: usize },

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Invalid record: {0}")]
    InvalidRecord(String),
}

/// Generates a simple UUID v4 (time-based for uniqueness)
fn generate_uuid() -> String {
    let timestamp = current_timestamp();
    let random_part: u64 = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (timestamp >> 32) as u32,
        (timestamp >> 16) as u16 & 0xFFFF,
        (timestamp & 0x0FFF) as u16,
        (random_part >> 48) as u16 | 0x8000,
        random_part & 0xFFFFFFFFFFFF
    )
}

use std::hash::{BuildHasher, Hasher};

/// Returns current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Query filter for fetching records needing enrichment
#[derive(Debug, Clone, Default)]
pub struct EnrichmentFilter {
    pub limit: usize,
    pub include_deleted: bool,
}

impl EnrichmentFilter {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            include_deleted: false,
        }
    }
}

/// Batch of records for AI processing
#[derive(Debug, Clone)]
pub struct EnrichmentBatch {
    pub records: Vec<MultimodalRecord>,
    pub batch_id: String,
}

impl EnrichmentBatch {
    pub fn new(records: Vec<MultimodalRecord>) -> Self {
        Self {
            records,
            batch_id: generate_uuid(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_creation() {
        let record = MultimodalRecord::new(
            "https://example.com/page".to_string(),
            12345678,
            "Sample content".to_string(),
        );

        assert!(!record.id.is_empty());
        assert_eq!(record.url, "https://example.com/page");
        assert_eq!(record.content_hash, 12345678);
        assert_eq!(record.text_content, "Sample content");
        assert!(record.embedding.is_none());
        assert!(record.sentiment_score.is_none());
        assert!(!record.is_deleted);
    }

    #[test]
    fn test_builder_pattern() {
        let record = MultimodalRecord::builder(
            "https://example.com".to_string(),
            999,
            "Content".to_string(),
        )
        .title("Test Title")
        .word_count(150)
        .quality_score(0.85)
        .sentiment_score(0.5)
        .build();

        assert_eq!(record.title, Some("Test Title".to_string()));
        assert_eq!(record.word_count, 150);
        assert!((record.quality_score - 0.85).abs() < f32::EPSILON);
        assert_eq!(record.sentiment_score, Some(0.5));
    }

    #[test]
    fn test_embedding_validation() {
        let mut record = MultimodalRecord::default();

        // * Valid embedding
        let valid_embedding = vec![0.0_f32; EMBEDDING_DIM];
        assert!(record.set_embedding(valid_embedding).is_ok());
        assert!(record.embedding.is_some());

        // * Invalid embedding
        let invalid_embedding = vec![0.0_f32; 512];
        let result = record.set_embedding(invalid_embedding);
        assert!(matches!(
            result,
            Err(SchemaError::InvalidEmbeddingDimension { .. })
        ));
    }

    #[test]
    fn test_sentiment_clamping() {
        let mut record = MultimodalRecord::default();

        // * Normal value
        record.set_sentiment(0.5);
        assert_eq!(record.sentiment_score, Some(0.5));

        // * Above max
        record.set_sentiment(2.0);
        assert_eq!(record.sentiment_score, Some(SENTIMENT_MAX));

        // * Below min
        record.set_sentiment(-2.0);
        assert_eq!(record.sentiment_score, Some(SENTIMENT_MIN));
    }

    #[test]
    fn test_needs_enrichment() {
        let mut record = MultimodalRecord::default();
        assert!(record.needs_enrichment());

        record.set_sentiment(0.5);
        assert!(record.needs_enrichment()); // * Still needs embedding

        let _ = record.set_embedding(vec![0.0_f32; EMBEDDING_DIM]);
        assert!(!record.needs_enrichment()); // * Fully enriched
    }

    #[test]
    fn test_soft_delete() {
        let mut record = MultimodalRecord::default();
        let original_updated = record.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(1100));
        record.soft_delete();

        assert!(record.is_deleted);
        assert!(record.updated_at >= original_updated);
    }

    #[test]
    fn test_media_reference() {
        let image = MediaReference::image("https://example.com/img.jpg".to_string());
        assert_eq!(image.media_type, MediaType::Image);

        let video = MediaReference::video("https://example.com/vid.mp4".to_string());
        assert_eq!(video.media_type, MediaType::Video);
    }

    #[test]
    fn test_serialization() {
        let record = MultimodalRecord::builder(
            "https://example.com".to_string(),
            12345,
            "Test content".to_string(),
        )
        .title("Test")
        .build();

        let json = record.to_json();
        assert!(json.contains("example.com"));
        assert!(json.contains("Test content"));

        // * Deserialize back
        let parsed: MultimodalRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.url, record.url);
        assert_eq!(parsed.content_hash, record.content_hash);
    }

    #[test]
    fn test_enrichment_batch() {
        let records = vec![
            MultimodalRecord::default(),
            MultimodalRecord::default(),
        ];
        let batch = EnrichmentBatch::new(records);

        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
        assert!(!batch.batch_id.is_empty());
    }

    #[test]
    fn test_uuid_format() {
        let uuid = generate_uuid();

        // * Check UUID v4 format: 8-4-4-4-12
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);

        // * Check version indicator (should start with 4)
        assert!(parts[2].starts_with('4'));
    }

    #[test]
    fn test_quality_score_clamping() {
        let record = MultimodalRecord::builder(
            "https://example.com".to_string(),
            1,
            "content".to_string(),
        )
        .quality_score(1.5) // * Above max
        .build();

        assert!((record.quality_score - 1.0).abs() < f32::EPSILON);

        let record2 = MultimodalRecord::builder(
            "https://example.com".to_string(),
            1,
            "content".to_string(),
        )
        .quality_score(-0.5) // * Below min
        .build();

        assert!(record2.quality_score.abs() < f32::EPSILON);
    }
}
