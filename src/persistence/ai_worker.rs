// * [FR-09] Async AI Enrichment Worker
// * Background worker for computing embeddings and sentiment scores
// * Strictly non-blocking to the main crawl loop

use crate::persistence::schema::{EnrichmentBatch, EnrichmentFilter, MultimodalRecord, EMBEDDING_DIM};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

// * Worker configuration constants
const DEFAULT_BATCH_SIZE: usize = 10;
const DEFAULT_POLL_INTERVAL_MS: u64 = 5000;
const DEFAULT_MAX_RETRIES: usize = 3;

/// AI Enrichment Worker for background processing
#[derive(Debug)]
pub struct AIEnrichmentWorker {
    config: WorkerConfig,
    running: Arc<AtomicBool>,
    processed_count: Arc<AtomicUsize>,
    error_count: Arc<AtomicUsize>,
}

/// Configuration for the AI enrichment worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Number of records to process per batch
    pub batch_size: usize,
    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,
    /// Maximum retries for failed enrichments
    pub max_retries: usize,
    /// Whether to compute embeddings
    pub compute_embeddings: bool,
    /// Whether to compute sentiment scores
    pub compute_sentiment: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            max_retries: DEFAULT_MAX_RETRIES,
            compute_embeddings: true,
            compute_sentiment: true,
        }
    }
}

impl AIEnrichmentWorker {
    /// Creates a new AI enrichment worker with default configuration
    pub fn new() -> Self {
        Self::with_config(WorkerConfig::default())
    }

    /// Creates a new worker with custom configuration
    pub fn with_config(config: WorkerConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            processed_count: Arc::new(AtomicUsize::new(0)),
            error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Returns the worker's configuration
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// Returns the number of records processed
    pub fn processed_count(&self) -> usize {
        self.processed_count.load(Ordering::Relaxed)
    }

    /// Returns the number of errors encountered
    pub fn error_count(&self) -> usize {
        self.error_count.load(Ordering::Relaxed)
    }

    /// Returns true if the worker is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Starts the worker with a record provider and updater
    ///
    /// This is the main entry point that spawns the background task.
    /// The provider fetches records needing enrichment.
    /// The updater persists enriched records back to storage.
    pub async fn start<P, U>(
        &self,
        provider: P,
        updater: U,
    ) -> WorkerHandle
    where
        P: RecordProvider + Send + Sync + 'static,
        U: RecordUpdater + Send + Sync + 'static,
    {
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
        let running = self.running.clone();
        let processed = self.processed_count.clone();
        let errors = self.error_count.clone();
        let config = self.config.clone();

        running.store(true, Ordering::Relaxed);

        let handle = tokio::spawn(async move {
            Self::worker_loop(
                config,
                provider,
                updater,
                running,
                processed,
                errors,
                shutdown_rx,
            )
            .await
        });

        WorkerHandle {
            shutdown_tx,
            join_handle: handle,
        }
    }

    /// Main worker loop - polls for records and processes them
    async fn worker_loop<P, U>(
        config: WorkerConfig,
        provider: P,
        updater: U,
        running: Arc<AtomicBool>,
        processed: Arc<AtomicUsize>,
        errors: Arc<AtomicUsize>,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) where
        P: RecordProvider,
        U: RecordUpdater,
    {
        let mut poll_interval = interval(Duration::from_millis(config.poll_interval_ms));

        tracing::info!(
            batch_size = config.batch_size,
            poll_interval_ms = config.poll_interval_ms,
            "AI enrichment worker started"
        );

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    tracing::info!("Shutdown signal received");
                    break;
                }
                _ = poll_interval.tick() => {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }

                    // * Fetch batch of records needing enrichment
                    let filter = EnrichmentFilter::new(config.batch_size);
                    match provider.fetch_unenriched(filter).await {
                        Ok(batch) if !batch.is_empty() => {
                            tracing::debug!(count = batch.len(), "Processing batch");

                            for mut record in batch.records {
                                match Self::enrich_record(&mut record, &config).await {
                                    Ok(()) => {
                                        if let Err(e) = updater.update_record(&record).await {
                                            tracing::error!(error = %e, "Failed to update record");
                                            errors.fetch_add(1, Ordering::Relaxed);
                                        } else {
                                            processed.fetch_add(1, Ordering::Relaxed);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            record_id = %record.id,
                                            error = %e,
                                            "Failed to enrich record"
                                        );
                                        errors.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                        Ok(_) => {
                            // * No records to process, continue polling
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to fetch records");
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }

        running.store(false, Ordering::Relaxed);
        tracing::info!(
            processed = processed.load(Ordering::Relaxed),
            errors = errors.load(Ordering::Relaxed),
            "AI enrichment worker stopped"
        );
    }

    /// Enriches a single record with embeddings and sentiment
    async fn enrich_record(record: &mut MultimodalRecord, config: &WorkerConfig) -> Result<(), EnrichmentError> {
        // * Compute embedding if needed and configured
        if config.compute_embeddings && record.embedding.is_none() {
            let embedding = compute_embedding(&record.text_content).await?;
            record
                .set_embedding(embedding)
                .map_err(|e| EnrichmentError::EmbeddingError(e.to_string()))?;
        }

        // * Compute sentiment if needed and configured
        if config.compute_sentiment && record.sentiment_score.is_none() {
            let sentiment = compute_sentiment(&record.text_content).await?;
            record.set_sentiment(sentiment);
        }

        Ok(())
    }

    /// Stops the worker gracefully
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Returns statistics about the worker
    pub fn stats(&self) -> WorkerStats {
        WorkerStats {
            is_running: self.is_running(),
            processed_count: self.processed_count(),
            error_count: self.error_count(),
        }
    }
}

impl Default for AIEnrichmentWorker {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle for controlling a running worker
pub struct WorkerHandle {
    shutdown_tx: mpsc::Sender<()>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl WorkerHandle {
    /// Sends shutdown signal to the worker
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(()).await;
        let _ = self.join_handle.await;
    }

    /// Checks if the worker task is still running
    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }
}

/// Statistics about the worker
#[derive(Debug, Clone)]
pub struct WorkerStats {
    pub is_running: bool,
    pub processed_count: usize,
    pub error_count: usize,
}

/// Type alias for async result
type AsyncResult<T> = Pin<Box<dyn Future<Output = Result<T, EnrichmentError>> + Send>>;

/// Trait for providing records that need enrichment
pub trait RecordProvider: Send + Sync {
    /// Fetches a batch of records that need enrichment
    fn fetch_unenriched(&self, filter: EnrichmentFilter) -> AsyncResult<EnrichmentBatch>;
}

/// Trait for updating enriched records
pub trait RecordUpdater: Send + Sync {
    /// Updates a record with enriched data
    fn update_record(&self, record: &MultimodalRecord) -> AsyncResult<()>;
}

/// Errors that can occur during enrichment
#[derive(Debug, Clone, thiserror::Error)]
pub enum EnrichmentError {
    #[error("Embedding computation failed: {0}")]
    EmbeddingError(String),

    #[error("Sentiment computation failed: {0}")]
    SentimentError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Computes embedding vector for text content
///
/// This is a placeholder implementation that simulates embedding computation.
/// In production, this would call an external API (OpenAI, Cohere, etc.)
/// or run a local model (sentence-transformers).
pub async fn compute_embedding(text: &str) -> Result<Vec<f32>, EnrichmentError> {
    // * Simulate API latency
    tokio::time::sleep(Duration::from_millis(10)).await;

    if text.is_empty() {
        return Err(EnrichmentError::EmbeddingError("Empty text".to_string()));
    }

    // * Generate deterministic pseudo-embedding based on text hash
    // ! In production: Replace with actual embedding model call
    let hash = hash_text(text);
    let embedding: Vec<f32> = (0..EMBEDDING_DIM)
        .map(|i| {
            let seed = hash.wrapping_add(i as u64);
            // * Generate value in [-1, 1] range
            ((seed % 2000) as f32 / 1000.0) - 1.0
        })
        .collect();

    // * Normalize the embedding vector
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    let normalized: Vec<f32> = if magnitude > 0.0 {
        embedding.iter().map(|x| x / magnitude).collect()
    } else {
        embedding
    };

    Ok(normalized)
}

/// Computes sentiment score for text content
///
/// This is a placeholder implementation that simulates sentiment analysis.
/// In production, this would call an external API or run a local model.
pub async fn compute_sentiment(text: &str) -> Result<f32, EnrichmentError> {
    // * Simulate API latency
    tokio::time::sleep(Duration::from_millis(5)).await;

    if text.is_empty() {
        return Err(EnrichmentError::SentimentError("Empty text".to_string()));
    }

    // * Simple lexicon-based sentiment (placeholder)
    // ! In production: Replace with actual sentiment model
    let text_lower = text.to_lowercase();

    let positive_words = [
        "good", "great", "excellent", "amazing", "wonderful", "fantastic",
        "positive", "happy", "love", "best", "awesome", "beautiful",
    ];
    let negative_words = [
        "bad", "terrible", "awful", "horrible", "negative", "sad",
        "hate", "worst", "ugly", "poor", "disappointing", "failed",
    ];

    let positive_count = positive_words
        .iter()
        .filter(|w| text_lower.contains(*w))
        .count() as f32;

    let negative_count = negative_words
        .iter()
        .filter(|w| text_lower.contains(*w))
        .count() as f32;

    let total = positive_count + negative_count;
    if total == 0.0 {
        return Ok(0.0); // * Neutral
    }

    // * Calculate sentiment in [-1, 1] range
    let sentiment = (positive_count - negative_count) / total;
    Ok(sentiment.clamp(-1.0, 1.0))
}

/// Simple hash function for text
fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// In-memory record store for testing
#[derive(Debug, Default)]
pub struct InMemoryRecordStore {
    records: std::sync::RwLock<Vec<MultimodalRecord>>,
}

impl InMemoryRecordStore {
    pub fn new() -> Self {
        Self {
            records: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn add(&self, record: MultimodalRecord) {
        let mut records = self.records.write().unwrap();
        records.push(record);
    }

    pub fn count(&self) -> usize {
        self.records.read().unwrap().len()
    }

    pub fn get_enriched_count(&self) -> usize {
        self.records
            .read()
            .unwrap()
            .iter()
            .filter(|r| !r.needs_enrichment())
            .count()
    }
}

impl RecordProvider for InMemoryRecordStore {
    fn fetch_unenriched(&self, filter: EnrichmentFilter) -> AsyncResult<EnrichmentBatch> {
        let records = self.records.read().unwrap();
        let unenriched: Vec<MultimodalRecord> = records
            .iter()
            .filter(|r| r.needs_enrichment() && (filter.include_deleted || !r.is_deleted))
            .take(filter.limit)
            .cloned()
            .collect();

        Box::pin(async move { Ok(EnrichmentBatch::new(unenriched)) })
    }
}

impl RecordUpdater for InMemoryRecordStore {
    fn update_record(&self, record: &MultimodalRecord) -> AsyncResult<()> {
        let mut records = self.records.write().unwrap();
        if let Some(existing) = records.iter_mut().find(|r| r.id == record.id) {
            *existing = record.clone();
            Box::pin(async { Ok(()) })
        } else {
            Box::pin(async { Err(EnrichmentError::StorageError("Record not found".to_string())) })
        }
    }
}

// * Implement traits for Arc<InMemoryRecordStore> to support shared ownership
impl RecordProvider for Arc<InMemoryRecordStore> {
    fn fetch_unenriched(&self, filter: EnrichmentFilter) -> AsyncResult<EnrichmentBatch> {
        (**self).fetch_unenriched(filter)
    }
}

impl RecordUpdater for Arc<InMemoryRecordStore> {
    fn update_record(&self, record: &MultimodalRecord) -> AsyncResult<()> {
        (**self).update_record(record)
    }
}

/// Builder for creating enrichment pipelines
#[derive(Debug)]
pub struct EnrichmentPipelineBuilder {
    config: WorkerConfig,
}

impl EnrichmentPipelineBuilder {
    pub fn new() -> Self {
        Self {
            config: WorkerConfig::default(),
        }
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.config.batch_size = size;
        self
    }

    pub fn poll_interval_ms(mut self, ms: u64) -> Self {
        self.config.poll_interval_ms = ms;
        self
    }

    pub fn max_retries(mut self, retries: usize) -> Self {
        self.config.max_retries = retries;
        self
    }

    pub fn with_embeddings(mut self, enabled: bool) -> Self {
        self.config.compute_embeddings = enabled;
        self
    }

    pub fn with_sentiment(mut self, enabled: bool) -> Self {
        self.config.compute_sentiment = enabled;
        self
    }

    pub fn build(self) -> AIEnrichmentWorker {
        AIEnrichmentWorker::with_config(self.config)
    }
}

impl Default for EnrichmentPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compute_embedding() {
        let embedding = compute_embedding("Test text for embedding").await.unwrap();

        assert_eq!(embedding.len(), EMBEDDING_DIM);

        // * Check normalization (magnitude should be ~1.0)
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.01, "Embedding should be normalized");
    }

    #[tokio::test]
    async fn test_compute_embedding_empty_text() {
        let result = compute_embedding("").await;
        assert!(matches!(result, Err(EnrichmentError::EmbeddingError(_))));
    }

    #[tokio::test]
    async fn test_compute_sentiment_positive() {
        let sentiment = compute_sentiment("This is a great and wonderful day").await.unwrap();
        assert!(sentiment > 0.0, "Positive text should have positive sentiment");
    }

    #[tokio::test]
    async fn test_compute_sentiment_negative() {
        let sentiment = compute_sentiment("This is a terrible and awful situation").await.unwrap();
        assert!(sentiment < 0.0, "Negative text should have negative sentiment");
    }

    #[tokio::test]
    async fn test_compute_sentiment_neutral() {
        let sentiment = compute_sentiment("The sky is blue and water is wet").await.unwrap();
        assert!(
            (sentiment - 0.0).abs() < f32::EPSILON,
            "Neutral text should have zero sentiment"
        );
    }

    #[tokio::test]
    async fn test_compute_sentiment_empty_text() {
        let result = compute_sentiment("").await;
        assert!(matches!(result, Err(EnrichmentError::SentimentError(_))));
    }

    #[tokio::test]
    async fn test_in_memory_store_provider() {
        let store = InMemoryRecordStore::new();

        // * Add unenriched record
        store.add(MultimodalRecord::new(
            "https://example.com".to_string(),
            12345,
            "Test content".to_string(),
        ));

        let filter = EnrichmentFilter::new(10);
        let batch = store.fetch_unenriched(filter).await.unwrap();

        assert_eq!(batch.len(), 1);
        assert!(batch.records[0].needs_enrichment());
    }

    #[tokio::test]
    async fn test_in_memory_store_updater() {
        let store = InMemoryRecordStore::new();

        let mut record = MultimodalRecord::new(
            "https://example.com".to_string(),
            12345,
            "Test content".to_string(),
        );
        let _id = record.id.clone();
        store.add(record.clone());

        // * Update with enrichment
        record.set_sentiment(0.5);
        let _ = record.set_embedding(vec![0.0; EMBEDDING_DIM]);

        store.update_record(&record).await.unwrap();

        assert_eq!(store.get_enriched_count(), 1);
    }

    #[tokio::test]
    async fn test_worker_config_default() {
        let config = WorkerConfig::default();

        assert_eq!(config.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(config.poll_interval_ms, DEFAULT_POLL_INTERVAL_MS);
        assert!(config.compute_embeddings);
        assert!(config.compute_sentiment);
    }

    #[tokio::test]
    async fn test_worker_builder() {
        let worker = EnrichmentPipelineBuilder::new()
            .batch_size(5)
            .poll_interval_ms(1000)
            .with_embeddings(true)
            .with_sentiment(false)
            .build();

        assert_eq!(worker.config().batch_size, 5);
        assert_eq!(worker.config().poll_interval_ms, 1000);
        assert!(worker.config().compute_embeddings);
        assert!(!worker.config().compute_sentiment);
    }

    #[tokio::test]
    async fn test_worker_stats() {
        let worker = AIEnrichmentWorker::new();
        let stats = worker.stats();

        assert!(!stats.is_running);
        assert_eq!(stats.processed_count, 0);
        assert_eq!(stats.error_count, 0);
    }

    #[tokio::test]
    async fn test_worker_start_and_stop() {
        let worker = AIEnrichmentWorker::with_config(WorkerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        });

        let store = Arc::new(InMemoryRecordStore::new());

        // * Add some records
        store.add(MultimodalRecord::new(
            "https://example.com/1".to_string(),
            1,
            "Great content here".to_string(),
        ));
        store.add(MultimodalRecord::new(
            "https://example.com/2".to_string(),
            2,
            "Terrible bad content".to_string(),
        ));

        // * Start worker
        let handle = worker.start(store.clone(), store.clone()).await;

        // * Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        // * Shutdown
        handle.shutdown().await;

        // * Verify processing occurred
        assert!(worker.processed_count() > 0, "Should have processed records");
        assert_eq!(store.get_enriched_count(), 2, "Both records should be enriched");
    }

    #[tokio::test]
    async fn test_worker_handles_no_records() {
        let worker = AIEnrichmentWorker::with_config(WorkerConfig {
            poll_interval_ms: 50,
            ..Default::default()
        });

        let store = Arc::new(InMemoryRecordStore::new());
        // * Empty store

        let handle = worker.start(store.clone(), store.clone()).await;

        tokio::time::sleep(Duration::from_millis(200)).await;

        handle.shutdown().await;

        // * Should complete without errors
        assert_eq!(worker.error_count(), 0);
    }

    #[test]
    fn test_enrichment_error_display() {
        let err = EnrichmentError::EmbeddingError("Model unavailable".to_string());
        assert!(err.to_string().contains("Model unavailable"));

        let err = EnrichmentError::SentimentError("API timeout".to_string());
        assert!(err.to_string().contains("API timeout"));
    }

    #[tokio::test]
    async fn test_embedding_determinism() {
        let text = "Same text content";

        let embedding1 = compute_embedding(text).await.unwrap();
        let embedding2 = compute_embedding(text).await.unwrap();

        // * Same text should produce same embedding
        assert_eq!(embedding1, embedding2);
    }

    #[tokio::test]
    async fn test_embedding_uniqueness() {
        let embedding1 = compute_embedding("First text").await.unwrap();
        let embedding2 = compute_embedding("Second different text").await.unwrap();

        // * Different text should produce different embeddings
        assert_ne!(embedding1, embedding2);
    }
}
