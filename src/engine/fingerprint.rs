// * [FR-03.b] [EDD-3.2] Smart Caching via Head Fingerprinting
// * Computes content fingerprints to detect duplicate/unchanged pages

use regex::Regex;
use std::sync::LazyLock;
use xxhash_rust::xxh64::xxh64;

// * Precompiled regex patterns for metadata extraction
static TITLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>([^<]*)</title>").unwrap());

static META_DESC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta[^>]*name\s*=\s*["']description["'][^>]*content\s*=\s*["']([^"']*)["']"#)
        .unwrap()
});

static OG_UPDATED_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta[^>]*property\s*=\s*["']og:updated_time["'][^>]*content\s*=\s*["']([^"']*)["']"#)
        .unwrap()
});

static LAST_MODIFIED_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta[^>]*(?:name|http-equiv)\s*=\s*["']last-modified["'][^>]*content\s*=\s*["']([^"']*)["']"#)
        .unwrap()
});

// * Represents extracted head metadata for fingerprinting
#[derive(Debug, Default)]
pub struct HeadMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_updated_time: Option<String>,
    pub last_modified: Option<String>,
}

impl HeadMetadata {
    // * Extracts metadata from HTML head section
    pub fn extract(html: &str) -> Self {
        Self {
            title: TITLE_REGEX
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string()),
            description: META_DESC_REGEX
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string()),
            og_updated_time: OG_UPDATED_REGEX
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string()),
            last_modified: LAST_MODIFIED_REGEX
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string()),
        }
    }

    // * Converts metadata to a canonical string for hashing
    fn to_canonical_string(&self) -> String {
        format!(
            "t:{};d:{};ou:{};lm:{}",
            self.title.as_deref().unwrap_or(""),
            self.description.as_deref().unwrap_or(""),
            self.og_updated_time.as_deref().unwrap_or(""),
            self.last_modified.as_deref().unwrap_or("")
        )
    }
}

// * Computes a 64-bit fingerprint from HTML head metadata
// * Used for deduplication and change detection
pub fn compute_head_fingerprint(html: &str) -> u64 {
    let metadata = HeadMetadata::extract(html);
    let canonical = metadata.to_canonical_string();
    xxh64(canonical.as_bytes(), 0)
}

// * Checks if content has changed by comparing fingerprints
pub fn has_content_changed(new_fingerprint: u64, cached_fingerprint: u64) -> bool {
    new_fingerprint != cached_fingerprint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = r#"<html><head><title>Test Page</title></head></html>"#;
        let metadata = HeadMetadata::extract(html);
        assert_eq!(metadata.title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_extract_meta_description() {
        let html = r#"<html><head><meta name="description" content="A test description"></head></html>"#;
        let metadata = HeadMetadata::extract(html);
        assert_eq!(metadata.description, Some("A test description".to_string()));
    }

    #[test]
    fn test_extract_og_updated_time() {
        let html = r#"<meta property="og:updated_time" content="2024-01-15T10:00:00Z">"#;
        let metadata = HeadMetadata::extract(html);
        assert_eq!(
            metadata.og_updated_time,
            Some("2024-01-15T10:00:00Z".to_string())
        );
    }

    #[test]
    fn test_fingerprint_consistency() {
        let html = r#"<title>Test</title><meta name="description" content="Desc">"#;
        let fp1 = compute_head_fingerprint(html);
        let fp2 = compute_head_fingerprint(html);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprint_changes_with_content() {
        let html1 = r#"<title>Page V1</title>"#;
        let html2 = r#"<title>Page V2</title>"#;
        let fp1 = compute_head_fingerprint(html1);
        let fp2 = compute_head_fingerprint(html2);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn test_has_content_changed() {
        assert!(has_content_changed(123, 456));
        assert!(!has_content_changed(123, 123));
    }
}
