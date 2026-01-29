// * Milestone 5: Persistence & AI Enrichment
// * Goal: Save data, deduplicate, and run async AI enrichment
// * This module provides storage, deduplication, link scoring, and AI processing

pub mod ai_worker;
pub mod dedup;
pub mod link_scorer;
pub mod schema;

// * Re-exports for convenient access
pub use ai_worker::{
    compute_embedding, compute_sentiment, AIEnrichmentWorker, EnrichmentError,
    EnrichmentPipelineBuilder, InMemoryRecordStore, RecordProvider, RecordUpdater,
    WorkerConfig, WorkerHandle, WorkerStats,
};
pub use dedup::{
    BloomFilter, DedupCheckResult, DedupManager, DedupResult, DedupStats, LSHIndex,
    MinHashSignature,
};
pub use link_scorer::{
    score_link, score_links, LinkScorer, PriorityLinkQueue, ScoreBreakdown, ScoredLink,
    ScorerConfig,
};
pub use schema::{
    EnrichmentBatch, EnrichmentFilter, MediaReference, MediaType, MultimodalRecord,
    MultimodalRecordBuilder, SchemaError, EMBEDDING_DIM, SENTIMENT_MAX, SENTIMENT_MIN,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // * Verify all major types are accessible
        let _record = MultimodalRecord::default();
        let _scorer = LinkScorer::new();
        let _index = LSHIndex::new();
        let _worker = AIEnrichmentWorker::new();
        let _manager = DedupManager::new();
    }

    #[test]
    fn test_integration_record_to_dedup() {
        let record = MultimodalRecord::new(
            "https://example.com/page".to_string(),
            12345,
            "Sample content for integration test".to_string(),
        );

        let mut manager = DedupManager::new();
        let result = manager.check_and_index(
            &record.url,
            record.content_hash,
            &record.text_content,
            &record.id,
        );

        assert!(result.is_unique());
    }

    #[test]
    fn test_integration_link_scoring() {
        let links = vec![
            ("https://example.com/docs/api".to_string(), "API Documentation".to_string()),
            ("https://example.com/ad/banner".to_string(), "Advertisement".to_string()),
            ("https://example.com/blog/post".to_string(), "Blog Post".to_string()),
        ];

        let scored = score_links(&links);

        // * Docs should be highest priority
        assert!(scored[0].url.contains("docs"), "Docs should be first");
        // * Ad should be lowest
        assert!(scored.last().unwrap().url.contains("ad"), "Ad should be last");
    }

    #[tokio::test]
    async fn test_integration_full_pipeline() {
        // * Create record
        let record = MultimodalRecord::builder(
            "https://example.com/article".to_string(),
            99999,
            "This is a great article about machine learning".to_string(),
        )
        .title("ML Article")
        .word_count(100)
        .build();

        // * Check deduplication
        let mut dedup = DedupManager::new();
        let dedup_result = dedup.check_and_index(
            &record.url,
            record.content_hash,
            &record.text_content,
            &record.id,
        );
        assert!(dedup_result.is_unique());

        // * Score the link
        let scored = score_link(&record.url, record.title.as_deref().unwrap_or(""));
        assert!(scored.score > 0.0);

        // * Verify needs enrichment
        assert!(record.needs_enrichment());
    }

    #[test]
    fn test_constants_exported() {
        assert_eq!(EMBEDDING_DIM, 768);
        assert!((SENTIMENT_MIN - (-1.0)).abs() < f32::EPSILON);
        assert!((SENTIMENT_MAX - 1.0).abs() < f32::EPSILON);
    }
}
