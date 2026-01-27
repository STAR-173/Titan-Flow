// * Milestone 4 - Task 4.5: Newsroom Metadata Extraction [FR-06.b]
// * Extraction chain: JSON-LD -> Meta Tags -> Fallback heuristics
// * Ported from crawl4ai content extraction patterns

use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// * Precompiled selectors for metadata extraction
static SELECTOR_JSON_LD: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(r#"script[type="application/ld+json"]"#).unwrap());
static SELECTOR_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").unwrap());
static SELECTOR_H1: LazyLock<Selector> = LazyLock::new(|| Selector::parse("h1").unwrap());
static SELECTOR_META: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("meta").unwrap());
static SELECTOR_TIME: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("time[datetime]").unwrap());
static SELECTOR_ARTICLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article").unwrap());

// * Regex patterns for date extraction from text
static DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{4}[-/]\d{2}[-/]\d{2})|(\w+\s+\d{1,2},?\s+\d{4})").unwrap()
});

/// Represents extracted metadata from a web page
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PageMetadata {
    // * Core fields
    pub title: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub authors: Vec<String>,

    // * Dates
    pub date_published: Option<String>,
    pub date_modified: Option<String>,

    // * Source info
    pub site_name: Option<String>,
    pub publisher: Option<String>,
    pub canonical_url: Option<String>,

    // * Social/SEO
    pub og_image: Option<String>,
    pub og_type: Option<String>,
    pub keywords: Vec<String>,

    // * Article-specific
    pub section: Option<String>,
    pub language: Option<String>,
    pub word_count: Option<usize>,
    pub reading_time_minutes: Option<u32>,

    // * Extraction metadata
    pub extraction_method: String,
}

impl PageMetadata {
    /// Converts metadata to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Converts metadata to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Checks if essential metadata was extracted
    pub fn has_essential_fields(&self) -> bool {
        self.title.is_some() || self.description.is_some()
    }
}

/// JSON-LD schema types we care about
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonLdType {
    Single(String),
    Array(Vec<String>),
}

/// Partial JSON-LD structure for article/news content
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct JsonLdArticle {
    #[serde(rename = "@type")]
    schema_type: Option<JsonLdType>,
    #[serde(alias = "headline")]
    name: Option<String>,
    description: Option<String>,
    #[serde(alias = "datePublished")]
    date_published: Option<String>,
    #[serde(alias = "dateModified")]
    date_modified: Option<String>,
    author: Option<JsonLdAuthor>,
    publisher: Option<JsonLdPublisher>,
    image: Option<JsonLdImage>,
    #[serde(alias = "articleSection")]
    article_section: Option<String>,
    #[serde(alias = "wordCount")]
    word_count: Option<usize>,
    #[serde(alias = "inLanguage")]
    in_language: Option<String>,
    #[serde(alias = "mainEntityOfPage")]
    main_entity_of_page: Option<JsonLdMainEntity>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonLdAuthor {
    Single(JsonLdPerson),
    Multiple(Vec<JsonLdPerson>),
    Name(String),
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct JsonLdPerson {
    name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct JsonLdPublisher {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonLdImage {
    Url(String),
    Object { url: Option<String> },
    Array(Vec<JsonLdImage>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonLdMainEntity {
    Url(String),
    Object {
        #[serde(rename = "@id")]
        id: Option<String>,
    },
}

/// Extracts metadata from HTML content using a prioritized extraction chain
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Main extraction method using prioritized chain:
    /// 1. JSON-LD (highest priority - structured data)
    /// 2. Open Graph meta tags
    /// 3. Standard meta tags
    /// 4. Fallback heuristics
    pub fn extract(html: &str) -> PageMetadata {
        let document = Html::parse_document(html);
        let mut metadata = PageMetadata::default();

        // * Step 1: Try JSON-LD extraction (best structured data)
        if Self::extract_json_ld(&document, &mut metadata) {
            metadata.extraction_method = "json_ld".to_string();
        }

        // * Step 2: Fill gaps with Open Graph tags
        Self::extract_open_graph(&document, &mut metadata);

        // * Step 3: Fill remaining gaps with standard meta tags
        Self::extract_meta_tags(&document, &mut metadata);

        // * Step 4: Fallback heuristics for missing essential fields
        Self::extract_fallbacks(&document, &mut metadata);

        // * Calculate reading time if we have word count
        if let Some(wc) = metadata.word_count {
            // * Average reading speed: 200-250 words per minute
            metadata.reading_time_minutes = Some((wc / 200).max(1) as u32);
        }

        if metadata.extraction_method.is_empty() {
            metadata.extraction_method = "meta_tags".to_string();
        }

        metadata
    }

    /// Extracts metadata from JSON-LD scripts
    fn extract_json_ld(document: &Html, metadata: &mut PageMetadata) -> bool {
        let mut found_article = false;

        for script in document.select(&SELECTOR_JSON_LD) {
            let json_text = script.text().collect::<String>();

            // * Try to parse as article/news schema
            if let Ok(article) = serde_json::from_str::<JsonLdArticle>(&json_text) {
                // * Check if this is an article type
                let is_article = match &article.schema_type {
                    Some(JsonLdType::Single(t)) => {
                        t.contains("Article") || t.contains("NewsArticle") || t.contains("BlogPosting")
                    }
                    Some(JsonLdType::Array(types)) => types.iter().any(|t| {
                        t.contains("Article") || t.contains("NewsArticle") || t.contains("BlogPosting")
                    }),
                    None => false,
                };

                if is_article || article.date_published.is_some() {
                    found_article = true;

                    // * Extract title/headline
                    if metadata.title.is_none() {
                        metadata.title = article.name;
                    }

                    // * Extract description
                    if metadata.description.is_none() {
                        metadata.description = article.description;
                    }

                    // * Extract dates
                    if metadata.date_published.is_none() {
                        metadata.date_published = article.date_published;
                    }
                    if metadata.date_modified.is_none() {
                        metadata.date_modified = article.date_modified;
                    }

                    // * Extract author(s)
                    if let Some(author) = article.author {
                        match author {
                            JsonLdAuthor::Single(person) => {
                                if let Some(name) = person.name {
                                    metadata.author = Some(name.clone());
                                    metadata.authors.push(name);
                                }
                            }
                            JsonLdAuthor::Multiple(people) => {
                                for person in people {
                                    if let Some(name) = person.name {
                                        if metadata.author.is_none() {
                                            metadata.author = Some(name.clone());
                                        }
                                        metadata.authors.push(name);
                                    }
                                }
                            }
                            JsonLdAuthor::Name(name) => {
                                metadata.author = Some(name.clone());
                                metadata.authors.push(name);
                            }
                        }
                    }

                    // * Extract publisher
                    if let Some(publisher) = article.publisher {
                        metadata.publisher = publisher.name;
                    }

                    // * Extract image
                    if let Some(image) = article.image {
                        metadata.og_image = Self::extract_image_url(image);
                    }

                    // * Extract article-specific fields
                    metadata.section = article.article_section;
                    metadata.word_count = article.word_count;
                    metadata.language = article.in_language;

                    // * Extract canonical URL
                    if let Some(main_entity) = article.main_entity_of_page {
                        metadata.canonical_url = match main_entity {
                            JsonLdMainEntity::Url(url) => Some(url),
                            JsonLdMainEntity::Object { id } => id,
                        };
                    }
                }
            }
        }

        found_article
    }

    /// Helper to extract URL from various JSON-LD image formats
    fn extract_image_url(image: JsonLdImage) -> Option<String> {
        match image {
            JsonLdImage::Url(url) => Some(url),
            JsonLdImage::Object { url } => url,
            JsonLdImage::Array(images) => images.into_iter().next().and_then(Self::extract_image_url),
        }
    }

    /// Extracts Open Graph meta tags
    fn extract_open_graph(document: &Html, metadata: &mut PageMetadata) {
        for meta in document.select(&SELECTOR_META) {
            let property = meta.value().attr("property").unwrap_or("");
            let content = meta.value().attr("content").unwrap_or("");

            if content.is_empty() {
                continue;
            }

            match property {
                "og:title" => {
                    if metadata.title.is_none() {
                        metadata.title = Some(content.to_string());
                    }
                }
                "og:description" => {
                    if metadata.description.is_none() {
                        metadata.description = Some(content.to_string());
                    }
                }
                "og:image" => {
                    if metadata.og_image.is_none() {
                        metadata.og_image = Some(content.to_string());
                    }
                }
                "og:site_name" => {
                    if metadata.site_name.is_none() {
                        metadata.site_name = Some(content.to_string());
                    }
                }
                "og:type" => {
                    metadata.og_type = Some(content.to_string());
                }
                "og:url" => {
                    if metadata.canonical_url.is_none() {
                        metadata.canonical_url = Some(content.to_string());
                    }
                }
                "article:published_time" => {
                    if metadata.date_published.is_none() {
                        metadata.date_published = Some(content.to_string());
                    }
                }
                "article:modified_time" => {
                    if metadata.date_modified.is_none() {
                        metadata.date_modified = Some(content.to_string());
                    }
                }
                "article:author" => {
                    if metadata.author.is_none() {
                        metadata.author = Some(content.to_string());
                    }
                }
                "article:section" => {
                    if metadata.section.is_none() {
                        metadata.section = Some(content.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    /// Extracts standard meta tags
    fn extract_meta_tags(document: &Html, metadata: &mut PageMetadata) {
        for meta in document.select(&SELECTOR_META) {
            let name = meta
                .value()
                .attr("name")
                .or_else(|| meta.value().attr("property"))
                .unwrap_or("");
            let content = meta.value().attr("content").unwrap_or("");

            if content.is_empty() {
                continue;
            }

            match name.to_lowercase().as_str() {
                "description" => {
                    if metadata.description.is_none() {
                        metadata.description = Some(content.to_string());
                    }
                }
                "author" => {
                    if metadata.author.is_none() {
                        metadata.author = Some(content.to_string());
                        if !metadata.authors.contains(&content.to_string()) {
                            metadata.authors.push(content.to_string());
                        }
                    }
                }
                "keywords" => {
                    let keywords: Vec<String> = content
                        .split(',')
                        .map(|k| k.trim().to_string())
                        .filter(|k| !k.is_empty())
                        .collect();
                    metadata.keywords.extend(keywords);
                }
                "date" | "publish-date" | "pubdate" => {
                    if metadata.date_published.is_none() {
                        metadata.date_published = Some(content.to_string());
                    }
                }
                "last-modified" | "revised" => {
                    if metadata.date_modified.is_none() {
                        metadata.date_modified = Some(content.to_string());
                    }
                }
                "language" | "content-language" => {
                    if metadata.language.is_none() {
                        metadata.language = Some(content.to_string());
                    }
                }
                _ => {}
            }
        }

        // * Extract canonical link
        if metadata.canonical_url.is_none() {
            let canonical_selector = Selector::parse(r#"link[rel="canonical"]"#).unwrap();
            if let Some(link) = document.select(&canonical_selector).next() {
                if let Some(href) = link.value().attr("href") {
                    metadata.canonical_url = Some(href.to_string());
                }
            }
        }

        // * Extract language from html tag
        if metadata.language.is_none() {
            let html_selector = Selector::parse("html").unwrap();
            if let Some(html) = document.select(&html_selector).next() {
                if let Some(lang) = html.value().attr("lang") {
                    metadata.language = Some(lang.to_string());
                }
            }
        }
    }

    /// Fallback extraction using heuristics
    fn extract_fallbacks(document: &Html, metadata: &mut PageMetadata) {
        // * Title fallback: <title> tag or first <h1>
        if metadata.title.is_none() {
            if let Some(title) = document.select(&SELECTOR_TITLE).next() {
                let text: String = title.text().collect();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    metadata.title = Some(trimmed.to_string());
                }
            }
        }

        if metadata.title.is_none() {
            if let Some(h1) = document.select(&SELECTOR_H1).next() {
                let text: String = h1.text().collect();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    metadata.title = Some(trimmed.to_string());
                }
            }
        }

        // * Date fallback: <time> elements with datetime attribute
        if metadata.date_published.is_none() {
            if let Some(time) = document.select(&SELECTOR_TIME).next() {
                if let Some(datetime) = time.value().attr("datetime") {
                    metadata.date_published = Some(datetime.to_string());
                }
            }
        }

        // * Date fallback: search for dates in article text
        if metadata.date_published.is_none() {
            if let Some(article) = document.select(&SELECTOR_ARTICLE).next() {
                let text: String = article.text().collect();
                if let Some(captures) = DATE_PATTERN.captures(&text) {
                    if let Some(date_match) = captures.get(0) {
                        metadata.date_published = Some(date_match.as_str().to_string());
                    }
                }
            }
        }

        // * Word count fallback: count words in article/body
        if metadata.word_count.is_none() {
            let body_selector = Selector::parse("body").unwrap();
            if let Some(body) = document.select(&body_selector).next() {
                let text: String = body.text().collect();
                let word_count = text.split_whitespace().count();
                if word_count > 0 {
                    metadata.word_count = Some(word_count);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_ld_extraction() {
        let html = r#"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "NewsArticle",
                    "headline": "Breaking News Story",
                    "description": "A detailed description of the news event",
                    "datePublished": "2024-01-15T10:30:00Z",
                    "dateModified": "2024-01-15T12:00:00Z",
                    "author": {"@type": "Person", "name": "John Doe"},
                    "publisher": {"@type": "Organization", "name": "News Corp"},
                    "articleSection": "Politics",
                    "wordCount": 1500
                }
                </script>
            </head>
            <body><article>Content here</article></body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);

        assert_eq!(metadata.title.as_deref(), Some("Breaking News Story"));
        assert_eq!(
            metadata.description.as_deref(),
            Some("A detailed description of the news event")
        );
        assert_eq!(
            metadata.date_published.as_deref(),
            Some("2024-01-15T10:30:00Z")
        );
        assert_eq!(metadata.author.as_deref(), Some("John Doe"));
        assert_eq!(metadata.publisher.as_deref(), Some("News Corp"));
        assert_eq!(metadata.section.as_deref(), Some("Politics"));
        assert_eq!(metadata.word_count, Some(1500));
        assert_eq!(metadata.extraction_method, "json_ld");
    }

    #[test]
    fn test_open_graph_extraction() {
        let html = r#"
            <html>
            <head>
                <meta property="og:title" content="OG Title"/>
                <meta property="og:description" content="OG Description"/>
                <meta property="og:image" content="https://example.com/image.jpg"/>
                <meta property="og:site_name" content="Example Site"/>
                <meta property="article:published_time" content="2024-02-20"/>
                <meta property="article:author" content="Jane Smith"/>
            </head>
            <body></body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);

        assert_eq!(metadata.title.as_deref(), Some("OG Title"));
        assert_eq!(metadata.description.as_deref(), Some("OG Description"));
        assert_eq!(
            metadata.og_image.as_deref(),
            Some("https://example.com/image.jpg")
        );
        assert_eq!(metadata.site_name.as_deref(), Some("Example Site"));
        assert_eq!(metadata.date_published.as_deref(), Some("2024-02-20"));
        assert_eq!(metadata.author.as_deref(), Some("Jane Smith"));
    }

    #[test]
    fn test_meta_tags_extraction() {
        let html = r#"
            <html lang="en">
            <head>
                <meta name="description" content="Standard meta description"/>
                <meta name="author" content="Test Author"/>
                <meta name="keywords" content="rust, web, crawler"/>
                <link rel="canonical" href="https://example.com/article"/>
            </head>
            <body></body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);

        assert_eq!(
            metadata.description.as_deref(),
            Some("Standard meta description")
        );
        assert_eq!(metadata.author.as_deref(), Some("Test Author"));
        assert_eq!(
            metadata.canonical_url.as_deref(),
            Some("https://example.com/article")
        );
        assert_eq!(metadata.language.as_deref(), Some("en"));
        assert!(metadata.keywords.contains(&"rust".to_string()));
        assert!(metadata.keywords.contains(&"web".to_string()));
    }

    #[test]
    fn test_fallback_extraction() {
        let html = r#"
            <html>
            <head>
                <title>Fallback Title | Site Name</title>
            </head>
            <body>
                <article>
                    <time datetime="2024-03-01">March 1, 2024</time>
                    <p>Article content goes here with multiple words.</p>
                </article>
            </body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);

        assert_eq!(
            metadata.title.as_deref(),
            Some("Fallback Title | Site Name")
        );
        assert_eq!(metadata.date_published.as_deref(), Some("2024-03-01"));
        assert!(metadata.word_count.is_some());
    }

    #[test]
    fn test_multiple_authors() {
        let html = r#"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "Article",
                    "headline": "Multi-author Article",
                    "author": [
                        {"@type": "Person", "name": "Author One"},
                        {"@type": "Person", "name": "Author Two"}
                    ]
                }
                </script>
            </head>
            <body></body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);

        assert_eq!(metadata.authors.len(), 2);
        assert!(metadata.authors.contains(&"Author One".to_string()));
        assert!(metadata.authors.contains(&"Author Two".to_string()));
    }

    #[test]
    fn test_reading_time_calculation() {
        let html = r#"
            <html>
            <head>
                <script type="application/ld+json">
                {
                    "@type": "Article",
                    "headline": "Long Article",
                    "wordCount": 2000
                }
                </script>
            </head>
            <body></body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);

        assert_eq!(metadata.word_count, Some(2000));
        assert_eq!(metadata.reading_time_minutes, Some(10)); // * 2000/200 = 10
    }

    #[test]
    fn test_to_json() {
        let html = r#"
            <html>
            <head>
                <meta property="og:title" content="Test Article"/>
            </head>
            <body></body>
            </html>
        "#;

        let metadata = MetadataExtractor::extract(html);
        let json = metadata.to_json();

        assert!(json.contains("Test Article"));
        assert!(json.contains("title"));
    }

    #[test]
    fn test_has_essential_fields() {
        let html_with = r#"<html><head><title>Has Title</title></head></html>"#;
        let html_without = r#"<html><head></head></html>"#;

        let meta_with = MetadataExtractor::extract(html_with);
        let meta_without = MetadataExtractor::extract(html_without);

        assert!(meta_with.has_essential_fields());
        assert!(!meta_without.has_essential_fields());
    }
}
