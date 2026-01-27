// * Milestone 4 - Task 4.2: Regex Entity Extraction [EDD-5.2]
// * Fast extraction of PII and common entities without LLMs.
// * Ported from crawl4ai/extraction_strategy.py (RegexExtractionStrategy)

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

// * Precompiled regex patterns for performance
// * Patterns ported from RegexExtractionStrategy.DEFAULT_PATTERNS

static PATTERN_EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").expect("Invalid email regex")
});

static PATTERN_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://[^\s"'<>\]\)]+[^\s"'<>\]\)\.,;:!?]"#).expect("Invalid URL regex")
});

static PATTERN_UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}")
        .expect("Invalid UUID regex")
});

static PATTERN_DATE_ISO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}").expect("Invalid ISO date regex"));

static PATTERN_PHONE_US: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+1[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}")
        .expect("Invalid US phone regex")
});

static PATTERN_PHONE_INTL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\+[1-9]\d{1,14}").expect("Invalid international phone regex")
});

static PATTERN_IPV4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b")
        .expect("Invalid IPv4 regex")
});

static PATTERN_CREDIT_CARD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b")
        .expect("Invalid credit card regex")
});

static PATTERN_SSN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("Invalid SSN regex")
});

static PATTERN_HASHTAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#[a-zA-Z][a-zA-Z0-9_]*").expect("Invalid hashtag regex"));

static PATTERN_MENTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@[a-zA-Z][a-zA-Z0-9_]*").expect("Invalid mention regex"));

static PATTERN_CURRENCY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[$\u20ac\u00a3]\s?\d{1,3}(?:,\d{3})*(?:\.\d{2})?|\d{1,3}(?:,\d{3})*(?:\.\d{2})?\s?(?:USD|EUR|GBP)")
        .expect("Invalid currency regex")
});

/// Represents the type of entity extracted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Email,
    Url,
    Uuid,
    DateIso,
    PhoneUs,
    PhoneIntl,
    Ipv4,
    CreditCard,
    Ssn,
    Hashtag,
    Mention,
    Currency,
}

impl EntityType {
    /// Returns the string representation for JSON output
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Email => "email",
            EntityType::Url => "url",
            EntityType::Uuid => "uuid",
            EntityType::DateIso => "date_iso",
            EntityType::PhoneUs => "phone_us",
            EntityType::PhoneIntl => "phone_intl",
            EntityType::Ipv4 => "ipv4",
            EntityType::CreditCard => "credit_card",
            EntityType::Ssn => "ssn",
            EntityType::Hashtag => "hashtag",
            EntityType::Mention => "mention",
            EntityType::Currency => "currency",
        }
    }
}

/// Configuration for which entity types to extract
#[derive(Debug, Clone)]
pub struct ExtractorConfig {
    pub extract_emails: bool,
    pub extract_urls: bool,
    pub extract_uuids: bool,
    pub extract_dates: bool,
    pub extract_phones: bool,
    pub extract_ips: bool,
    pub extract_pii: bool,
    pub extract_social: bool,
    pub extract_currency: bool,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            extract_emails: true,
            extract_urls: true,
            extract_uuids: true,
            extract_dates: true,
            extract_phones: true,
            extract_ips: false,
            extract_pii: false, // ! Disabled by default for privacy
            extract_social: true,
            extract_currency: true,
        }
    }
}

/// Result of entity extraction containing all found entities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractionResult {
    pub entities: HashMap<String, Vec<String>>,
    pub total_count: usize,
}

impl ExtractionResult {
    /// Converts the extraction result to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Converts the extraction result to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Extracts entities from text using regex patterns
pub struct RegexExtractor {
    config: ExtractorConfig,
}

impl RegexExtractor {
    /// Creates a new extractor with default configuration
    pub fn new() -> Self {
        Self {
            config: ExtractorConfig::default(),
        }
    }

    /// Creates a new extractor with custom configuration
    pub fn with_config(config: ExtractorConfig) -> Self {
        Self { config }
    }

    /// Extracts all unique matches for a given pattern
    fn extract_pattern(pattern: &Regex, text: &str) -> Vec<String> {
        pattern
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Extracts all configured entity types from the given text
    pub fn extract(&self, text: &str) -> ExtractionResult {
        let mut entities: HashMap<String, Vec<String>> = HashMap::new();
        let mut total_count = 0;

        // * Core entities (always extracted based on config)
        if self.config.extract_emails {
            let matches = Self::extract_pattern(&PATTERN_EMAIL, text);
            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(EntityType::Email.as_str().to_string(), matches);
            }
        }

        if self.config.extract_urls {
            let matches = Self::extract_pattern(&PATTERN_URL, text);
            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(EntityType::Url.as_str().to_string(), matches);
            }
        }

        if self.config.extract_uuids {
            let matches = Self::extract_pattern(&PATTERN_UUID, text);
            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(EntityType::Uuid.as_str().to_string(), matches);
            }
        }

        if self.config.extract_dates {
            let matches = Self::extract_pattern(&PATTERN_DATE_ISO, text);
            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(EntityType::DateIso.as_str().to_string(), matches);
            }
        }

        // * Phone numbers
        if self.config.extract_phones {
            let us_phones = Self::extract_pattern(&PATTERN_PHONE_US, text);
            if !us_phones.is_empty() {
                total_count += us_phones.len();
                entities.insert(EntityType::PhoneUs.as_str().to_string(), us_phones);
            }

            let intl_phones = Self::extract_pattern(&PATTERN_PHONE_INTL, text);
            if !intl_phones.is_empty() {
                total_count += intl_phones.len();
                entities.insert(EntityType::PhoneIntl.as_str().to_string(), intl_phones);
            }
        }

        // * IP addresses
        if self.config.extract_ips {
            let matches = Self::extract_pattern(&PATTERN_IPV4, text);
            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(EntityType::Ipv4.as_str().to_string(), matches);
            }
        }

        // ! PII extraction (disabled by default)
        if self.config.extract_pii {
            let cc_matches = Self::extract_pattern(&PATTERN_CREDIT_CARD, text);
            if !cc_matches.is_empty() {
                total_count += cc_matches.len();
                entities.insert(EntityType::CreditCard.as_str().to_string(), cc_matches);
            }

            let ssn_matches = Self::extract_pattern(&PATTERN_SSN, text);
            if !ssn_matches.is_empty() {
                total_count += ssn_matches.len();
                entities.insert(EntityType::Ssn.as_str().to_string(), ssn_matches);
            }
        }

        // * Social media entities
        if self.config.extract_social {
            let hashtags = Self::extract_pattern(&PATTERN_HASHTAG, text);
            if !hashtags.is_empty() {
                total_count += hashtags.len();
                entities.insert(EntityType::Hashtag.as_str().to_string(), hashtags);
            }

            let mentions = Self::extract_pattern(&PATTERN_MENTION, text);
            if !mentions.is_empty() {
                total_count += mentions.len();
                entities.insert(EntityType::Mention.as_str().to_string(), mentions);
            }
        }

        // * Currency values
        if self.config.extract_currency {
            let matches = Self::extract_pattern(&PATTERN_CURRENCY, text);
            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(EntityType::Currency.as_str().to_string(), matches);
            }
        }

        ExtractionResult {
            entities,
            total_count,
        }
    }

    /// Extracts only the specified entity types
    pub fn extract_specific(&self, text: &str, types: &[EntityType]) -> ExtractionResult {
        let mut entities: HashMap<String, Vec<String>> = HashMap::new();
        let mut total_count = 0;

        for entity_type in types {
            let matches = match entity_type {
                EntityType::Email => Self::extract_pattern(&PATTERN_EMAIL, text),
                EntityType::Url => Self::extract_pattern(&PATTERN_URL, text),
                EntityType::Uuid => Self::extract_pattern(&PATTERN_UUID, text),
                EntityType::DateIso => Self::extract_pattern(&PATTERN_DATE_ISO, text),
                EntityType::PhoneUs => Self::extract_pattern(&PATTERN_PHONE_US, text),
                EntityType::PhoneIntl => Self::extract_pattern(&PATTERN_PHONE_INTL, text),
                EntityType::Ipv4 => Self::extract_pattern(&PATTERN_IPV4, text),
                EntityType::CreditCard => Self::extract_pattern(&PATTERN_CREDIT_CARD, text),
                EntityType::Ssn => Self::extract_pattern(&PATTERN_SSN, text),
                EntityType::Hashtag => Self::extract_pattern(&PATTERN_HASHTAG, text),
                EntityType::Mention => Self::extract_pattern(&PATTERN_MENTION, text),
                EntityType::Currency => Self::extract_pattern(&PATTERN_CURRENCY, text),
            };

            if !matches.is_empty() {
                total_count += matches.len();
                entities.insert(entity_type.as_str().to_string(), matches);
            }
        }

        ExtractionResult {
            entities,
            total_count,
        }
    }
}

impl Default for RegexExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_emails() {
        let extractor = RegexExtractor::new();
        let text = "Contact us at support@example.com or sales@company.org for help.";

        let result = extractor.extract(text);
        let emails = result.entities.get("email").unwrap();

        assert_eq!(emails.len(), 2);
        assert!(emails.contains(&"support@example.com".to_string()));
        assert!(emails.contains(&"sales@company.org".to_string()));
    }

    #[test]
    fn test_extract_urls() {
        let extractor = RegexExtractor::new();
        let text = "Visit https://example.com and http://test.org/page?id=1 for more info.";

        let result = extractor.extract(text);
        let urls = result.entities.get("url").unwrap();

        assert_eq!(urls.len(), 2);
        assert!(urls.iter().any(|u| u.contains("example.com")));
        assert!(urls.iter().any(|u| u.contains("test.org")));
    }

    #[test]
    fn test_extract_uuids() {
        let extractor = RegexExtractor::new();
        let text = "Record ID: 550e8400-e29b-41d4-a716-446655440000 and another: 6ba7b810-9dad-11d1-80b4-00c04fd430c8";

        let result = extractor.extract(text);
        let uuids = result.entities.get("uuid").unwrap();

        assert_eq!(uuids.len(), 2);
    }

    #[test]
    fn test_extract_iso_dates() {
        let extractor = RegexExtractor::new();
        let text = "Published on 2024-01-15 and updated 2024-03-20.";

        let result = extractor.extract(text);
        let dates = result.entities.get("date_iso").unwrap();

        assert_eq!(dates.len(), 2);
        assert!(dates.contains(&"2024-01-15".to_string()));
        assert!(dates.contains(&"2024-03-20".to_string()));
    }

    #[test]
    fn test_extract_us_phones() {
        let extractor = RegexExtractor::new();
        let text = "Call us at (555) 123-4567 or 555-987-6543 or +1-800-555-0199";

        let result = extractor.extract(text);
        let phones = result.entities.get("phone_us").unwrap();

        assert!(phones.len() >= 2);
    }

    #[test]
    fn test_extract_hashtags_and_mentions() {
        let extractor = RegexExtractor::new();
        let text = "Check out #TitanFlow and @rustlang for updates. #WebCrawler is trending!";

        let result = extractor.extract(text);

        let hashtags = result.entities.get("hashtag").unwrap();
        assert_eq!(hashtags.len(), 2);
        assert!(hashtags.contains(&"#TitanFlow".to_string()));
        assert!(hashtags.contains(&"#WebCrawler".to_string()));

        let mentions = result.entities.get("mention").unwrap();
        assert!(mentions.contains(&"@rustlang".to_string()));
    }

    #[test]
    fn test_extract_currency() {
        let extractor = RegexExtractor::new();
        let text = "Price: $1,234.56 or €99.99 or 500 USD";

        let result = extractor.extract(text);
        let currency = result.entities.get("currency").unwrap();

        assert!(!currency.is_empty());
    }

    #[test]
    fn test_extract_specific_types() {
        let extractor = RegexExtractor::new();
        let text = "Email: test@example.com, Date: 2024-01-15, UUID: 550e8400-e29b-41d4-a716-446655440000";

        let result = extractor.extract_specific(text, &[EntityType::Email, EntityType::DateIso]);

        assert!(result.entities.contains_key("email"));
        assert!(result.entities.contains_key("date_iso"));
        assert!(!result.entities.contains_key("uuid"));
    }

    #[test]
    fn test_to_json() {
        let extractor = RegexExtractor::new();
        let text = "Contact: admin@test.com on 2024-01-01";

        let result = extractor.extract(text);
        let json = result.to_json();

        assert!(json.contains("email"));
        assert!(json.contains("admin@test.com"));
        assert!(json.contains("date_iso"));
    }

    #[test]
    fn test_pii_disabled_by_default() {
        let extractor = RegexExtractor::new();
        let text = "SSN: 123-45-6789, Card: 4111111111111111";

        let result = extractor.extract(text);

        // * PII should not be extracted with default config
        assert!(!result.entities.contains_key("ssn"));
        assert!(!result.entities.contains_key("credit_card"));
    }

    #[test]
    fn test_pii_when_enabled() {
        let config = ExtractorConfig {
            extract_pii: true,
            ..Default::default()
        };
        let extractor = RegexExtractor::with_config(config);
        let text = "SSN: 123-45-6789, Card: 4111111111111111";

        let result = extractor.extract(text);

        assert!(result.entities.contains_key("ssn"));
        assert!(result.entities.contains_key("credit_card"));
    }
}
