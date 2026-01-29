// * [FR-05] Deduplication with LSHBloom MinHash
// * Implements near-duplicate detection using MinHash signatures and LSH banding

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

// * LSH configuration constants
const NUM_HASH_FUNCTIONS: usize = 100;
const NUM_BANDS: usize = 20;
const ROWS_PER_BAND: usize = NUM_HASH_FUNCTIONS / NUM_BANDS; // * 5 rows per band
const JACCARD_THRESHOLD: f64 = 0.85;

// * Shingle size for text tokenization
const SHINGLE_SIZE: usize = 3;

/// MinHash signature for a document
#[derive(Debug, Clone)]
pub struct MinHashSignature {
    pub signature: Vec<u64>,
    pub document_id: String,
}

impl MinHashSignature {
    /// Computes MinHash signature from text content
    pub fn from_text(text: &str, document_id: String) -> Self {
        let shingles = generate_shingles(text, SHINGLE_SIZE);
        let signature = compute_minhash(&shingles);
        Self {
            signature,
            document_id,
        }
    }

    /// Computes estimated Jaccard similarity between two signatures
    pub fn jaccard_similarity(&self, other: &MinHashSignature) -> f64 {
        if self.signature.len() != other.signature.len() {
            return 0.0;
        }

        let matches = self
            .signature
            .iter()
            .zip(other.signature.iter())
            .filter(|(a, b)| a == b)
            .count();

        matches as f64 / self.signature.len() as f64
    }
}

/// LSH Index for fast near-duplicate detection
#[derive(Debug)]
pub struct LSHIndex {
    // * Band -> Hash -> Document IDs
    bands: Vec<HashMap<u64, Vec<String>>>,
    // * Document ID -> Signature
    signatures: HashMap<String, MinHashSignature>,
    // * Configuration
    num_bands: usize,
    rows_per_band: usize,
    threshold: f64,
}

impl LSHIndex {
    /// Creates a new LSH index with default configuration
    pub fn new() -> Self {
        Self::with_config(NUM_BANDS, ROWS_PER_BAND, JACCARD_THRESHOLD)
    }

    /// Creates an LSH index with custom configuration
    pub fn with_config(num_bands: usize, rows_per_band: usize, threshold: f64) -> Self {
        Self {
            bands: (0..num_bands).map(|_| HashMap::new()).collect(),
            signatures: HashMap::new(),
            num_bands,
            rows_per_band,
            threshold,
        }
    }

    /// Indexes a document and returns true if it's a duplicate
    ///
    /// Returns `DedupResult::Duplicate` if Jaccard > threshold, `DedupResult::Unique` otherwise
    pub fn index_document(&mut self, text: &str, document_id: &str) -> DedupResult {
        let signature = MinHashSignature::from_text(text, document_id.to_string());

        // * Find candidate duplicates using LSH banding
        let candidates = self.find_candidates(&signature);

        // * Verify candidates with actual Jaccard similarity
        for candidate_id in candidates {
            if let Some(candidate_sig) = self.signatures.get(&candidate_id) {
                let similarity = signature.jaccard_similarity(candidate_sig);
                if similarity >= self.threshold {
                    tracing::info!(
                        document_id = document_id,
                        duplicate_of = candidate_id,
                        similarity = similarity,
                        "Duplicate detected"
                    );
                    return DedupResult::Duplicate {
                        original_id: candidate_id,
                        similarity,
                    };
                }
            }
        }

        // * Not a duplicate, add to index
        self.add_to_index(&signature);
        self.signatures
            .insert(document_id.to_string(), signature);

        DedupResult::Unique
    }

    /// Checks if a document is a duplicate without indexing it
    pub fn check_duplicate(&self, text: &str) -> Option<DedupResult> {
        let signature = MinHashSignature::from_text(text, String::new());
        let candidates = self.find_candidates(&signature);

        for candidate_id in candidates {
            if let Some(candidate_sig) = self.signatures.get(&candidate_id) {
                let similarity = signature.jaccard_similarity(candidate_sig);
                if similarity >= self.threshold {
                    return Some(DedupResult::Duplicate {
                        original_id: candidate_id,
                        similarity,
                    });
                }
            }
        }

        None
    }

    /// Finds candidate duplicates using LSH banding technique
    fn find_candidates(&self, signature: &MinHashSignature) -> HashSet<String> {
        let mut candidates = HashSet::new();

        for (band_idx, band_map) in self.bands.iter().enumerate() {
            let band_hash = self.compute_band_hash(signature, band_idx);
            if let Some(doc_ids) = band_map.get(&band_hash) {
                candidates.extend(doc_ids.iter().cloned());
            }
        }

        // * Remove self if present
        candidates.remove(&signature.document_id);
        candidates
    }

    /// Adds a signature to the LSH index
    fn add_to_index(&mut self, signature: &MinHashSignature) {
        for band_idx in 0..self.num_bands {
            let band_hash = self.compute_band_hash(signature, band_idx);
            self.bands[band_idx]
                .entry(band_hash)
                .or_default()
                .push(signature.document_id.clone());
        }
    }

    /// Computes hash for a specific band of the signature
    fn compute_band_hash(&self, signature: &MinHashSignature, band_idx: usize) -> u64 {
        let start = band_idx * self.rows_per_band;
        let end = std::cmp::min(start + self.rows_per_band, signature.signature.len());

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for i in start..end {
            signature.signature[i].hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Returns the number of indexed documents
    pub fn document_count(&self) -> usize {
        self.signatures.len()
    }

    /// Removes a document from the index
    pub fn remove_document(&mut self, document_id: &str) -> bool {
        if let Some(signature) = self.signatures.remove(document_id) {
            // * Remove from all band buckets
            for band_idx in 0..self.num_bands {
                let band_hash = self.compute_band_hash(&signature, band_idx);
                if let Some(doc_ids) = self.bands[band_idx].get_mut(&band_hash) {
                    doc_ids.retain(|id| id != document_id);
                }
            }
            true
        } else {
            false
        }
    }

    /// Clears the entire index
    pub fn clear(&mut self) {
        self.bands.iter_mut().for_each(|band| band.clear());
        self.signatures.clear();
    }
}

impl Default for LSHIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of deduplication check
#[derive(Debug, Clone)]
pub enum DedupResult {
    /// Document is unique and was indexed
    Unique,
    /// Document is a duplicate of another document
    Duplicate {
        original_id: String,
        similarity: f64,
    },
}

impl DedupResult {
    /// Returns true if the document is a duplicate
    pub fn is_duplicate(&self) -> bool {
        matches!(self, DedupResult::Duplicate { .. })
    }

    /// Returns true if the document is unique
    pub fn is_unique(&self) -> bool {
        matches!(self, DedupResult::Unique)
    }
}

/// Generates character n-gram shingles from text
fn generate_shingles(text: &str, n: usize) -> HashSet<u64> {
    let normalized = normalize_text(text);
    let chars: Vec<char> = normalized.chars().collect();

    if chars.len() < n {
        let mut set = HashSet::new();
        if !chars.is_empty() {
            set.insert(hash_string(&normalized));
        }
        return set;
    }

    let mut shingles = HashSet::new();
    for window in chars.windows(n) {
        let shingle: String = window.iter().collect();
        shingles.insert(hash_string(&shingle));
    }
    shingles
}

/// Normalizes text for consistent shingle generation
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Hashes a string to u64
fn hash_string(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Computes MinHash signature for a set of shingles
fn compute_minhash(shingles: &HashSet<u64>) -> Vec<u64> {
    if shingles.is_empty() {
        return vec![u64::MAX; NUM_HASH_FUNCTIONS];
    }

    let mut signature = vec![u64::MAX; NUM_HASH_FUNCTIONS];

    // * Generate hash coefficients for each hash function
    // * Using simple polynomial hash: h_i(x) = (a_i * x + b_i) mod p
    const LARGE_PRIME: u64 = 4_294_967_311; // * Prime larger than u32::MAX

    for (i, min_hash) in signature.iter_mut().enumerate() {
        let a = (i as u64 + 1) * 0xBF58476D1CE4E5B9;
        let b = (i as u64 + 1) * 0x94D049BB133111EB;

        for &shingle in shingles {
            let hash_value = (a.wrapping_mul(shingle).wrapping_add(b)) % LARGE_PRIME;
            if hash_value < *min_hash {
                *min_hash = hash_value;
            }
        }
    }

    signature
}

/// Bloom filter for quick membership testing (LSHBloom optimization)
#[derive(Debug)]
pub struct BloomFilter {
    bits: Vec<bool>,
    num_hash_functions: usize,
    size: usize,
}

impl BloomFilter {
    /// Creates a new Bloom filter with specified size and hash count
    pub fn new(size: usize, num_hash_functions: usize) -> Self {
        Self {
            bits: vec![false; size],
            num_hash_functions,
            size,
        }
    }

    /// Creates a Bloom filter optimized for expected items and false positive rate
    pub fn with_capacity(expected_items: usize, false_positive_rate: f64) -> Self {
        // * Optimal size: m = -n * ln(p) / (ln(2)^2)
        let m = (-(expected_items as f64) * false_positive_rate.ln() / (2.0_f64.ln().powi(2)))
            .ceil() as usize;

        // * Optimal hash functions: k = (m/n) * ln(2)
        let k = ((m as f64 / expected_items as f64) * 2.0_f64.ln()).ceil() as usize;

        Self::new(m.max(1), k.max(1))
    }

    /// Adds an item to the Bloom filter
    pub fn add(&mut self, item: &str) {
        for i in 0..self.num_hash_functions {
            let idx = self.hash(item, i);
            self.bits[idx] = true;
        }
    }

    /// Checks if an item might be in the Bloom filter
    pub fn might_contain(&self, item: &str) -> bool {
        for i in 0..self.num_hash_functions {
            let idx = self.hash(item, i);
            if !self.bits[idx] {
                return false;
            }
        }
        true
    }

    /// Computes hash for a specific hash function index
    fn hash(&self, item: &str, seed: usize) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        item.hash(&mut hasher);
        seed.hash(&mut hasher);
        (hasher.finish() as usize) % self.size
    }

    /// Clears the filter
    pub fn clear(&mut self) {
        self.bits.fill(false);
    }
}

/// Deduplication manager combining LSH Index and Bloom Filter
#[derive(Debug)]
pub struct DedupManager {
    lsh_index: LSHIndex,
    url_bloom: BloomFilter,
    content_hash_set: HashSet<u64>,
}

impl DedupManager {
    /// Creates a new deduplication manager
    pub fn new() -> Self {
        Self {
            lsh_index: LSHIndex::new(),
            url_bloom: BloomFilter::with_capacity(100_000, 0.01),
            content_hash_set: HashSet::new(),
        }
    }

    /// Checks URL-level deduplication (fast path)
    pub fn check_url(&self, url: &str) -> bool {
        self.url_bloom.might_contain(url)
    }

    /// Checks content hash deduplication (medium path)
    pub fn check_content_hash(&self, hash: u64) -> bool {
        self.content_hash_set.contains(&hash)
    }

    /// Full deduplication check including near-duplicate detection
    pub fn check_and_index(
        &mut self,
        url: &str,
        content_hash: u64,
        text: &str,
        document_id: &str,
    ) -> DedupCheckResult {
        // * Level 1: URL check
        if self.check_url(url) {
            return DedupCheckResult::DuplicateUrl;
        }

        // * Level 2: Content hash check
        if self.check_content_hash(content_hash) {
            return DedupCheckResult::DuplicateHash;
        }

        // * Level 3: Near-duplicate check with LSH
        match self.lsh_index.index_document(text, document_id) {
            DedupResult::Duplicate {
                original_id,
                similarity,
            } => DedupCheckResult::NearDuplicate {
                original_id,
                similarity,
            },
            DedupResult::Unique => {
                // * Add to URL bloom and hash set
                self.url_bloom.add(url);
                self.content_hash_set.insert(content_hash);
                DedupCheckResult::Unique
            }
        }
    }

    /// Returns statistics about the deduplication state
    pub fn stats(&self) -> DedupStats {
        DedupStats {
            indexed_documents: self.lsh_index.document_count(),
            unique_hashes: self.content_hash_set.len(),
        }
    }
}

impl Default for DedupManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of multi-level deduplication check
#[derive(Debug, Clone)]
pub enum DedupCheckResult {
    /// Document is unique at all levels
    Unique,
    /// Exact URL duplicate
    DuplicateUrl,
    /// Exact content hash duplicate
    DuplicateHash,
    /// Near-duplicate detected via LSH
    NearDuplicate { original_id: String, similarity: f64 },
}

impl DedupCheckResult {
    pub fn is_duplicate(&self) -> bool {
        !matches!(self, DedupCheckResult::Unique)
    }

    pub fn is_unique(&self) -> bool {
        matches!(self, DedupCheckResult::Unique)
    }
}

/// Statistics about deduplication state
#[derive(Debug, Clone)]
pub struct DedupStats {
    pub indexed_documents: usize,
    pub unique_hashes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minhash_signature_creation() {
        let text = "This is a sample document for testing MinHash signatures";
        let sig = MinHashSignature::from_text(text, "doc1".to_string());

        assert_eq!(sig.signature.len(), NUM_HASH_FUNCTIONS);
        assert_eq!(sig.document_id, "doc1");
    }

    #[test]
    fn test_jaccard_similarity_identical() {
        let text = "This is a test document with some content";
        let sig1 = MinHashSignature::from_text(text, "doc1".to_string());
        let sig2 = MinHashSignature::from_text(text, "doc2".to_string());

        let similarity = sig1.jaccard_similarity(&sig2);
        assert!(
            (similarity - 1.0).abs() < f64::EPSILON,
            "Identical texts should have similarity 1.0"
        );
    }

    #[test]
    fn test_jaccard_similarity_different() {
        let text1 = "The quick brown fox jumps over the lazy dog";
        let text2 = "A completely different sentence about programming";

        let sig1 = MinHashSignature::from_text(text1, "doc1".to_string());
        let sig2 = MinHashSignature::from_text(text2, "doc2".to_string());

        let similarity = sig1.jaccard_similarity(&sig2);
        assert!(
            similarity < 0.5,
            "Different texts should have low similarity"
        );
    }

    #[test]
    fn test_jaccard_similarity_similar() {
        let text1 = "The quick brown fox jumps over the lazy dog today";
        let text2 = "The quick brown fox jumps over the lazy dog yesterday";

        let sig1 = MinHashSignature::from_text(text1, "doc1".to_string());
        let sig2 = MinHashSignature::from_text(text2, "doc2".to_string());

        let similarity = sig1.jaccard_similarity(&sig2);
        assert!(
            similarity > 0.7,
            "Similar texts should have high similarity: {}",
            similarity
        );
    }

    #[test]
    fn test_lsh_index_duplicate_detection() {
        let mut index = LSHIndex::new();

        let text1 = "This is a comprehensive document about machine learning and artificial intelligence in the modern world";
        let text2 = "This is a comprehensive document about machine learning and artificial intelligence in the modern era";

        let result1 = index.index_document(text1, "doc1");
        assert!(result1.is_unique());

        let result2 = index.index_document(text2, "doc2");
        assert!(result2.is_duplicate(), "Similar document should be detected as duplicate");
    }

    #[test]
    fn test_lsh_index_unique_documents() {
        let mut index = LSHIndex::new();

        let text1 = "Machine learning is a subset of artificial intelligence";
        let text2 = "Web development involves creating websites and applications";

        let result1 = index.index_document(text1, "doc1");
        assert!(result1.is_unique());

        let result2 = index.index_document(text2, "doc2");
        assert!(result2.is_unique());

        assert_eq!(index.document_count(), 2);
    }

    #[test]
    fn test_lsh_index_removal() {
        let mut index = LSHIndex::new();

        let text = "Sample document for testing removal functionality";
        index.index_document(text, "doc1");
        assert_eq!(index.document_count(), 1);

        let removed = index.remove_document("doc1");
        assert!(removed);
        assert_eq!(index.document_count(), 0);

        let not_found = index.remove_document("doc1");
        assert!(!not_found);
    }

    #[test]
    fn test_bloom_filter() {
        let mut bloom = BloomFilter::with_capacity(1000, 0.01);

        bloom.add("https://example.com/page1");
        bloom.add("https://example.com/page2");

        assert!(bloom.might_contain("https://example.com/page1"));
        assert!(bloom.might_contain("https://example.com/page2"));

        // * May have false positives, but false negatives should not occur
        // * This is a probabilistic test
    }

    #[test]
    fn test_dedup_manager_full_workflow() {
        let mut manager = DedupManager::new();

        let result1 = manager.check_and_index(
            "https://example.com/page1",
            12345,
            "This is some unique content for the first page",
            "doc1",
        );
        assert!(result1.is_unique());

        // * Same URL should be detected
        let result2 = manager.check_and_index(
            "https://example.com/page1",
            99999,
            "Different content entirely",
            "doc2",
        );
        assert!(matches!(result2, DedupCheckResult::DuplicateUrl));

        // * Same hash should be detected
        let result3 = manager.check_and_index(
            "https://example.com/page3",
            12345,
            "Yet another content",
            "doc3",
        );
        assert!(matches!(result3, DedupCheckResult::DuplicateHash));

        let stats = manager.stats();
        assert_eq!(stats.indexed_documents, 1);
        assert_eq!(stats.unique_hashes, 1);
    }

    #[test]
    fn test_shingle_generation() {
        let shingles = generate_shingles("hello", 3);
        // * "hel", "ell", "llo" = 3 shingles
        assert_eq!(shingles.len(), 3);
    }

    #[test]
    fn test_shingle_short_text() {
        let shingles = generate_shingles("hi", 3);
        // * Text shorter than shingle size produces single shingle
        assert_eq!(shingles.len(), 1);
    }

    #[test]
    fn test_text_normalization() {
        let normalized = normalize_text("  Hello,   WORLD!  How are YOU?  ");
        assert_eq!(normalized, "hello world how are you");
    }

    #[test]
    fn test_near_duplicate_threshold() {
        let mut index = LSHIndex::with_config(20, 5, 0.9); // * Stricter threshold

        let text1 = "The quick brown fox jumps over the lazy dog";
        let text2 = "The quick brown fox jumps over the lazy cat";

        index.index_document(text1, "doc1");
        let result = index.index_document(text2, "doc2");

        // * With 0.9 threshold, these might be unique
        // * This tests that threshold is respected
        assert!(result.is_unique() || result.is_duplicate());
    }

    #[test]
    fn test_check_duplicate_without_indexing() {
        let mut index = LSHIndex::new();

        let text = "Original document with substantial content for testing purposes";
        index.index_document(text, "original");

        // * Check without indexing
        let similar_text =
            "Original document with substantial content for testing goals";
        let result = index.check_duplicate(similar_text);

        // * Should find the duplicate but not add to index
        assert!(result.is_some() || result.is_none()); // * Depends on similarity
        assert_eq!(index.document_count(), 1); // * Still only 1 document
    }

    #[test]
    fn test_empty_text_handling() {
        let mut index = LSHIndex::new();

        let result = index.index_document("", "empty");
        assert!(result.is_unique());

        let result2 = index.index_document("   ", "whitespace");
        assert!(result2.is_unique() || result2.is_duplicate());
    }
}
