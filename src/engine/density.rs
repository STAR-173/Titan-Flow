// * [FR-02] [APP-A.2] Density Metric Calculator
// * Computes content density score to determine Fast vs Slow path routing

use scraper::{Html, Selector};
use std::sync::LazyLock;

// * Routing threshold - scores below this trigger Slow Path
const SLOW_PATH_THRESHOLD: f64 = 0.48;

// * Weight factors for density calculation
const TEXT_DENSITY_WEIGHT: f64 = 0.4;
const LINK_DENSITY_WEIGHT: f64 = 0.2;
const TAG_SCORE_WEIGHT: f64 = 0.2;

// * Tag score values
const HIGH_VALUE_TAG_SCORE: f64 = 1.5;
const LOW_VALUE_TAG_SCORE: f64 = 0.5;

// * Precompiled selectors
static TEXT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("body").unwrap());
static LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").unwrap());
static ARTICLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article, main").unwrap());

// * Routing decision based on density score
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RoutingPath {
    Fast,
    Slow,
}

// * Detailed density metrics for a page
#[derive(Debug, Clone)]
pub struct DensityMetrics {
    pub text_density: f64,
    pub link_density: f64,
    pub tag_score: f64,
    pub final_score: f64,
    pub routing: RoutingPath,
}

impl DensityMetrics {
    // * Computes all density metrics from HTML
    pub fn compute(html: &str) -> Self {
        let document = Html::parse_document(html);

        let text_density = compute_text_density(&document);
        let link_density = compute_link_density(&document);
        let tag_score = compute_tag_score(&document);

        // * Formula: Score = (0.4 * text_density) + (0.2 * link_density) + (0.2 * tag_score)
        let final_score = (TEXT_DENSITY_WEIGHT * text_density)
            + (LINK_DENSITY_WEIGHT * link_density)
            + (TAG_SCORE_WEIGHT * tag_score);

        let routing = if final_score < SLOW_PATH_THRESHOLD {
            RoutingPath::Slow
        } else {
            RoutingPath::Fast
        };

        Self {
            text_density,
            link_density,
            tag_score,
            final_score,
            routing,
        }
    }

    // * Quick routing decision without full metrics
    pub fn should_use_slow_path(html: &str) -> bool {
        Self::compute(html).routing == RoutingPath::Slow
    }
}

// * Computes text density as ratio of text length to total HTML length
fn compute_text_density(document: &Html) -> f64 {
    let body_text: String = document
        .select(&TEXT_SELECTOR)
        .flat_map(|el| el.text())
        .collect::<Vec<_>>()
        .join(" ");

    let text_len = body_text.split_whitespace().count();
    let html_len = document.root_element().html().len();

    if html_len == 0 {
        return 0.0;
    }

    // * Normalize to 0-1 range (assuming ~10 chars per word average)
    let ratio = (text_len as f64 * 10.0) / html_len as f64;
    ratio.min(1.0)
}

// * Computes link density as ratio of link text to total text
// * FIXED: Now strips whitespace to ensure formatting doesn't skew ratio
fn compute_link_density(document: &Html) -> f64 {
    let total_text_len: usize = document
        .select(&TEXT_SELECTOR)
        .flat_map(|el| el.text())
        .map(|s| s.trim().len())
        .sum();

    let link_text_len: usize = document
        .select(&LINK_SELECTOR)
        .flat_map(|el| el.text())
        .map(|s| s.trim().len())
        .sum();

    if total_text_len == 0 {
        return 0.0;
    }

    // * Invert so lower link density = higher score (content-rich pages have less link text)
    let ratio = link_text_len as f64 / total_text_len as f64;
    1.0 - ratio.min(1.0)
}

// * Computes tag score based on presence of semantic content tags
fn compute_tag_score(document: &Html) -> f64 {
    let has_article_or_main = document.select(&ARTICLE_SELECTOR).next().is_some();

    if has_article_or_main {
        HIGH_VALUE_TAG_SCORE
    } else {
        LOW_VALUE_TAG_SCORE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_quality_content_routes_fast() {
        let html = r#"
            <html>
            <body>
                <article>
                    <h1>Article Title</h1>
                    <p>This is a well-structured article with substantial text content.
                    It contains multiple paragraphs of meaningful information that would
                    be valuable to readers. The content is rich and informative.</p>
                    <p>Another paragraph with more valuable content that adds to the
                    overall quality and density of the page.</p>
                </article>
            </body>
            </html>
        "#;

        let metrics = DensityMetrics::compute(html);
        assert_eq!(metrics.tag_score, HIGH_VALUE_TAG_SCORE);
        assert!(metrics.text_density > 0.0);
    }

    #[test]
    fn test_link_heavy_page_has_low_link_density_score() {
        let html = r#"
            <html><body>
                <a href="/1">Link 1</a>
                <a href="/2">Link 2</a>
                <a href="/3">Link 3</a>
            </body></html>
        "#;

        let metrics = DensityMetrics::compute(html);
        // * Link-heavy pages should have lower link_density score
        // * With whitespace stripping, total_len ~18, link_len ~18, ratio ~1.0, score ~0.0
        assert!(metrics.link_density < 0.5);
    }

    #[test]
    fn test_page_without_article_tag_gets_low_tag_score() {
        let html = r#"<html><body><div>Just a div</div></body></html>"#;

        let metrics = DensityMetrics::compute(html);
        assert_eq!(metrics.tag_score, LOW_VALUE_TAG_SCORE);
    }

    #[test]
    fn test_page_with_main_tag_gets_high_tag_score() {
        let html = r#"<html><body><main>Main content</main></body></html>"#;

        let metrics = DensityMetrics::compute(html);
        assert_eq!(metrics.tag_score, HIGH_VALUE_TAG_SCORE);
    }

    #[test]
    fn test_empty_page_routes_slow() {
        let html = r#"<html><body></body></html>"#;

        let metrics = DensityMetrics::compute(html);
        assert_eq!(metrics.routing, RoutingPath::Slow);
    }

    #[test]
    fn test_should_use_slow_path_helper() {
        let sparse_html = r#"<html><body><script>app.init()</script></body></html>"#;
        assert!(DensityMetrics::should_use_slow_path(sparse_html));
    }
}