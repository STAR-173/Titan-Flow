// * Milestone 4 - Task 4.1 (README): DOM Tree Shaking & Boilerplate Removal
// * Removes navigation, footer, sidebar, ads, scripts, and extracts main content.
// * Ported from crawl4ai content filtering strategies

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// * Selectors for elements to remove (boilerplate)
// * These are prepared for future DOM manipulation when full tree shaking is implemented
#[allow(dead_code)]
static SELECTOR_SCRIPT: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script").unwrap());
#[allow(dead_code)]
static SELECTOR_STYLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("style").unwrap());
#[allow(dead_code)]
static SELECTOR_NAV: LazyLock<Selector> = LazyLock::new(|| Selector::parse("nav").unwrap());
#[allow(dead_code)]
static SELECTOR_FOOTER: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("footer").unwrap());
#[allow(dead_code)]
static SELECTOR_HEADER: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("header").unwrap());
#[allow(dead_code)]
static SELECTOR_ASIDE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("aside").unwrap());
#[allow(dead_code)]
static SELECTOR_NOSCRIPT: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("noscript").unwrap());
#[allow(dead_code)]
static SELECTOR_IFRAME: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("iframe").unwrap());
#[allow(dead_code)]
static SELECTOR_FORM: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("form").unwrap());
#[allow(dead_code)]
static SELECTOR_SVG: LazyLock<Selector> = LazyLock::new(|| Selector::parse("svg").unwrap());

// * Selectors for common ad/tracking patterns
#[allow(dead_code)]
static SELECTOR_ADS: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        r#"[class*="ad-"], [class*="ads-"], [class*="advert"],
           [id*="ad-"], [id*="ads-"], [id*="advert"],
           [class*="banner"], [class*="promo"], [class*="sponsor"],
           [class*="social-share"], [class*="share-buttons"],
           [class*="newsletter"], [class*="subscribe"],
           [class*="popup"], [class*="modal"], [class*="overlay"],
           [class*="cookie"], [class*="consent"],
           [data-ad], [data-advertisement]"#,
    )
    .unwrap()
});

// * Selectors for sidebar/navigation patterns
#[allow(dead_code)]
static SELECTOR_SIDEBAR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        r#"[class*="sidebar"], [class*="side-bar"], [id*="sidebar"],
           [class*="widget"], [class*="related-posts"], [class*="recommended"],
           [class*="comments"], [id*="comments"], [class*="comment-section"],
           [class*="breadcrumb"], [class*="pagination"]"#,
    )
    .unwrap()
});

// * Selectors for main content areas (priority order)
static SELECTOR_ARTICLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article").unwrap());
static SELECTOR_MAIN: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("main, [role='main']").unwrap());
static SELECTOR_CONTENT: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        r#"[class*="content"], [class*="article"], [class*="post-body"],
           [class*="entry-content"], [class*="post-content"],
           [id*="content"], [id*="article"], [id*="post"]"#,
    )
    .unwrap()
});

// * Paragraph and heading selectors for text extraction
static SELECTOR_PARAGRAPHS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("p").unwrap());
static SELECTOR_HEADINGS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1, h2, h3, h4, h5, h6").unwrap());
static SELECTOR_LIST_ITEMS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("li").unwrap());
static SELECTOR_BLOCKQUOTE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("blockquote").unwrap());
static SELECTOR_PRE: LazyLock<Selector> = LazyLock::new(|| Selector::parse("pre").unwrap());
static SELECTOR_CODE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("code").unwrap());

/// Configuration for content cleaning
#[derive(Debug, Clone)]
pub struct CleanerConfig {
    /// Remove navigation elements
    pub remove_nav: bool,
    /// Remove header elements
    pub remove_header: bool,
    /// Remove footer elements
    pub remove_footer: bool,
    /// Remove sidebar/aside elements
    pub remove_sidebar: bool,
    /// Remove ad-related elements
    pub remove_ads: bool,
    /// Remove forms
    pub remove_forms: bool,
    /// Remove iframes
    pub remove_iframes: bool,
    /// Minimum text length to consider a paragraph valid
    pub min_paragraph_length: usize,
    /// Minimum word count for extracted content
    pub min_word_count: usize,
}

impl Default for CleanerConfig {
    fn default() -> Self {
        Self {
            remove_nav: true,
            remove_header: true,
            remove_footer: true,
            remove_sidebar: true,
            remove_ads: true,
            remove_forms: true,
            remove_iframes: true,
            min_paragraph_length: 20,
            min_word_count: 25, // * Lowered from 50 to be more permissive
        }
    }
}

/// Result of content extraction
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanedContent {
    /// Main text content (cleaned and joined)
    pub text: String,
    /// Individual paragraphs
    pub paragraphs: Vec<String>,
    /// Extracted headings with their level
    pub headings: Vec<(u8, String)>,
    /// Code blocks found in the content
    pub code_blocks: Vec<String>,
    /// Blockquotes found in the content
    pub quotes: Vec<String>,
    /// Word count of extracted text
    pub word_count: usize,
    /// Whether main content area was found
    pub found_main_content: bool,
    /// Extraction quality score (0.0 - 1.0)
    pub quality_score: f32,
}

impl CleanedContent {
    /// Converts to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Extracts and cleans main content from HTML
pub struct ContentCleaner {
    config: CleanerConfig,
}

impl ContentCleaner {
    /// Creates a new cleaner with default configuration
    pub fn new() -> Self {
        Self {
            config: CleanerConfig::default(),
        }
    }

    /// Creates a new cleaner with custom configuration
    pub fn with_config(config: CleanerConfig) -> Self {
        Self { config }
    }

    /// Main extraction method - removes boilerplate and extracts content
    pub fn clean(&self, html: &str) -> CleanedContent {
        let document = Html::parse_document(html);
        let mut result = CleanedContent::default();

        // * Step 1: Find main content area
        let main_content = self.find_main_content(&document);
        result.found_main_content = main_content.is_some();

        // * Step 2: Extract content from main area or full document
        let content_html = if let Some(main) = main_content {
            main
        } else {
            // * Fallback: use body content
            let body_selector = Selector::parse("body").unwrap();
            if let Some(body) = document.select(&body_selector).next() {
                body.html()
            } else {
                html.to_string()
            }
        };

        // * Step 3: Parse content area and extract text
        let content_doc = Html::parse_fragment(&content_html);
        self.extract_content(&content_doc, &mut result);

        // * Step 4: Calculate quality score
        result.quality_score = self.calculate_quality(&result);

        result
    }

    /// Finds the main content area using priority selectors
    fn find_main_content(&self, document: &Html) -> Option<String> {
        // * Priority 1: <article> tag
        for article in document.select(&SELECTOR_ARTICLE) {
            let html = article.html();
            if self.is_valid_content_area(&html) {
                return Some(html);
            }
        }

        // * Priority 2: <main> tag or role="main"
        for main in document.select(&SELECTOR_MAIN) {
            let html = main.html();
            if self.is_valid_content_area(&html) {
                return Some(html);
            }
        }

        // * Priority 3: Common content class/id patterns
        for content in document.select(&SELECTOR_CONTENT) {
            let html = content.html();
            if self.is_valid_content_area(&html) {
                return Some(html);
            }
        }

        None
    }

    /// Validates if an HTML snippet is a valid content area
    fn is_valid_content_area(&self, html: &str) -> bool {
        let doc = Html::parse_fragment(html);
        let text: String = doc.root_element().text().collect();
        let word_count = text.split_whitespace().count();

        word_count >= self.config.min_word_count
    }

    /// Extracts paragraphs, headings, code, and quotes from content
    fn extract_content(&self, document: &Html, result: &mut CleanedContent) {
        let mut all_text = Vec::new();

        // * Extract headings
        for heading in document.select(&SELECTOR_HEADINGS) {
            let text: String = heading.text().collect();
            let text = text.trim();

            if !text.is_empty() && !self.is_boilerplate_text(text) {
                // * Determine heading level from tag name
                let tag = heading.value().name();
                let level = tag.chars().nth(1).and_then(|c| c.to_digit(10)).unwrap_or(1) as u8;

                result.headings.push((level, text.to_string()));
                all_text.push(text.to_string());
            }
        }

        // * Extract paragraphs
        for para in document.select(&SELECTOR_PARAGRAPHS) {
            let text: String = para.text().collect();
            let text = text.trim();

            if text.len() >= self.config.min_paragraph_length && !self.is_boilerplate_text(text) {
                result.paragraphs.push(text.to_string());
                all_text.push(text.to_string());
            }
        }

        // * Extract list items (often contain important content)
        for item in document.select(&SELECTOR_LIST_ITEMS) {
            let text: String = item.text().collect();
            let text = text.trim();

            if text.len() >= self.config.min_paragraph_length && !self.is_boilerplate_text(text) {
                all_text.push(format!("• {}", text));
            }
        }

        // * Extract blockquotes
        for quote in document.select(&SELECTOR_BLOCKQUOTE) {
            let text: String = quote.text().collect();
            let text = text.trim();

            if !text.is_empty() {
                result.quotes.push(text.to_string());
            }
        }

        // * Extract code blocks
        for pre in document.select(&SELECTOR_PRE) {
            let text: String = pre.text().collect();
            if !text.is_empty() {
                result.code_blocks.push(text);
            }
        }

        // * Also check standalone code elements
        for code in document.select(&SELECTOR_CODE) {
            // * Skip if parent is <pre> (already captured)
            let text: String = code.text().collect();
            if !text.is_empty() && text.len() > 20 {
                // * Only longer code snippets
                if !result.code_blocks.iter().any(|b| b.contains(&text)) {
                    result.code_blocks.push(text);
                }
            }
        }

        // * Build final text
        result.text = all_text.join("\n\n");
        result.word_count = result.text.split_whitespace().count();
    }

    /// Checks if text looks like boilerplate content
    fn is_boilerplate_text(&self, text: &str) -> bool {
        let lower = text.to_lowercase();

        // * Common boilerplate patterns
        let boilerplate_patterns = [
            "cookie",
            "privacy policy",
            "terms of service",
            "terms and conditions",
            "subscribe to",
            "sign up for",
            "follow us on",
            "share this",
            "related posts",
            "you may also like",
            "advertisement",
            "sponsored",
            "click here",
            "read more",
            "learn more",
            "©",
            "all rights reserved",
            "powered by",
        ];

        for pattern in &boilerplate_patterns {
            if lower.contains(pattern) {
                return true;
            }
        }

        // * Check for very short navigation-like text
        if text.len() < 15 && (lower.contains("menu") || lower.contains("home") || lower.contains("contact"))
        {
            return true;
        }

        false
    }

    /// Calculates extraction quality score
    fn calculate_quality(&self, result: &CleanedContent) -> f32 {
        let mut score = 0.0_f32;

        // * +0.3 for finding main content area
        if result.found_main_content {
            score += 0.3;
        }

        // * +0.2 for having headings
        if !result.headings.is_empty() {
            score += 0.2;
        }

        // * +0.2 for substantial paragraph count
        if result.paragraphs.len() >= 3 {
            score += 0.2;
        }

        // * +0.2 for good word count
        if result.word_count >= 200 {
            score += 0.2;
        } else if result.word_count >= 100 {
            score += 0.1;
        }

        // * +0.1 for code blocks or quotes (indicates rich content)
        if !result.code_blocks.is_empty() || !result.quotes.is_empty() {
            score += 0.1;
        }

        score.min(1.0)
    }

    /// Extracts just the text content (simplified API)
    pub fn extract_text(&self, html: &str) -> String {
        self.clean(html).text
    }
}

impl Default for ContentCleaner {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility function for quick content extraction with defaults
pub fn extract_content(html: &str) -> CleanedContent {
    ContentCleaner::new().clean(html)
}

/// Utility function for quick text extraction
pub fn extract_text(html: &str) -> String {
    ContentCleaner::new().extract_text(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_article_extraction() {
        let html = r#"
            <html>
            <body>
                <nav><a href="/">Home</a><a href="/about">About</a></nav>
                <article>
                    <h1>Main Article Title</h1>
                    <p>This is the first paragraph of the main article content with enough words to pass the minimum threshold.</p>
                    <p>This is the second paragraph with more interesting content about the topic being discussed in detail.</p>
                </article>
                <footer>Copyright 2024 Example Site. All rights reserved.</footer>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        assert!(result.found_main_content);
        assert!(!result.text.is_empty());
        assert!(result.paragraphs.len() >= 2);
        assert!(!result.headings.is_empty());
        assert!(result.headings[0].1.contains("Main Article Title"));
    }

    #[test]
    fn test_boilerplate_removal() {
        let html = r#"
            <html>
            <body>
                <article>
                    <p>This is legitimate article content that should be extracted and preserved properly.</p>
                    <p>Subscribe to our newsletter for more updates and special offers!</p>
                    <p>This is another legitimate paragraph with substantive information about the topic.</p>
                </article>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        // * Newsletter text should be filtered
        assert!(!result.text.to_lowercase().contains("subscribe"));
        // * Legitimate content should remain
        assert!(result.text.contains("legitimate article content"));
    }

    #[test]
    fn test_code_block_extraction() {
        let html = r#"
            <html>
            <body>
                <article>
                    <h2>Code Example</h2>
                    <p>Here is an example of how to use the function in your project for best results.</p>
                    <pre><code>fn main() {
    println!("Hello, world!");
}</code></pre>
                </article>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        assert!(!result.code_blocks.is_empty());
        assert!(result.code_blocks[0].contains("println!"));
    }

    #[test]
    fn test_blockquote_extraction() {
        let html = r#"
            <html>
            <body>
                <article>
                    <p>The author made an important statement about this complex topic that deserves attention.</p>
                    <blockquote>This is a notable quote that should be extracted separately from regular content.</blockquote>
                </article>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        assert!(!result.quotes.is_empty());
        assert!(result.quotes[0].contains("notable quote"));
    }

    #[test]
    fn test_heading_levels() {
        let html = r#"
            <html>
            <body>
                <article>
                    <h1>Main Title</h1>
                    <h2>Section One</h2>
                    <p>Content for section one with enough words to pass the minimum paragraph threshold.</p>
                    <h3>Subsection</h3>
                    <p>Content for subsection with additional detail and explanation of the topic.</p>
                </article>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        assert!(result.headings.len() >= 3);

        // * Check heading levels
        let h1_found = result.headings.iter().any(|(level, _)| *level == 1);
        let h2_found = result.headings.iter().any(|(level, _)| *level == 2);
        let h3_found = result.headings.iter().any(|(level, _)| *level == 3);

        assert!(h1_found);
        assert!(h2_found);
        assert!(h3_found);
    }

    #[test]
    fn test_quality_score_calculation() {
        // * High quality content
        let good_html = r#"
            <html>
            <body>
                <article>
                    <h1>Quality Article</h1>
                    <p>First paragraph with substantial content about the main topic being discussed here.</p>
                    <p>Second paragraph continuing the discussion with more details and information.</p>
                    <p>Third paragraph wrapping up the article with additional insights and conclusions.</p>
                    <blockquote>An important quote from an expert in the field.</blockquote>
                </article>
            </body>
            </html>
        "#;

        let good_result = extract_content(good_html);
        assert!(good_result.quality_score >= 0.5, "Quality score was {}", good_result.quality_score);

        // * Low quality content
        let poor_html = r#"
            <html>
            <body>
                <div>Short text</div>
            </body>
            </html>
        "#;

        let poor_result = extract_content(poor_html);
        assert!(poor_result.quality_score < 0.5);
    }

    #[test]
    fn test_fallback_without_article() {
        let html = r#"
            <html>
            <body>
                <div class="content">
                    <h1>Page Without Article Tag</h1>
                    <p>This content is inside a div with class content and should still be extracted properly.</p>
                    <p>Second paragraph with more information about the topic being discussed in detail.</p>
                </div>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        assert!(!result.text.is_empty());
        assert!(result.text.contains("Page Without Article"));
    }

    #[test]
    fn test_main_tag_priority() {
        let html = r#"
            <html>
            <body>
                <main>
                    <h1>Main Content Area</h1>
                    <p>This is the main content that should be extracted with high priority over other elements in the page structure.</p>
                    <p>Additional paragraph content to ensure we have enough words to meet the minimum threshold for content validation.</p>
                </main>
                <aside>
                    <p>This sidebar content should not be included in the main extraction output.</p>
                </aside>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        assert!(result.found_main_content);
        assert!(result.text.contains("Main Content Area"));
    }

    #[test]
    fn test_custom_config() {
        let config = CleanerConfig {
            min_paragraph_length: 10, // * Lower threshold
            min_word_count: 10,
            ..Default::default()
        };
        let cleaner = ContentCleaner::with_config(config);

        let html = r#"
            <html>
            <body>
                <article>
                    <p>Short para.</p>
                    <p>Another short one.</p>
                </article>
            </body>
            </html>
        "#;

        let result = cleaner.clean(html);
        assert!(!result.paragraphs.is_empty());
    }

    #[test]
    fn test_list_extraction() {
        let html = r#"
            <html>
            <body>
                <article>
                    <h2>Features</h2>
                    <ul>
                        <li>First feature with enough description to pass the threshold</li>
                        <li>Second feature with detailed explanation of functionality</li>
                        <li>Third feature describing another important capability</li>
                    </ul>
                </article>
            </body>
            </html>
        "#;

        let result = extract_content(html);

        // * List items should be captured with bullet markers
        assert!(result.text.contains("•"));
    }
}
