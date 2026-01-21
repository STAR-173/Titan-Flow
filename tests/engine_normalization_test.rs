use titan_flow::engine::normalization::normalize_url;

// * Test Suite for URL Normalization [EDD-3.2]

#[test]
fn test_basic_normalization() {
    let base = "https://example.com";
    let href = "page";
    assert_eq!(normalize_url(href, base).unwrap(), "https://example.com/page");
}

#[test]
fn test_strip_fragment() {
    let base = "https://example.com";
    let href = "page#section1";
    assert_eq!(normalize_url(href, base).unwrap(), "https://example.com/page");
}

#[test]
fn test_lowercase_host() {
    let base = "https://EXAMPLE.com";
    let href = "/page";
    assert_eq!(normalize_url(href, base).unwrap(), "https://example.com/page");
}

#[test]
fn test_tracking_param_removal() {
    let base = "https://example.com";
    // * Complex URL with mixed tracking and real params
    let href = "/product?id=123&utm_source=google&ref=landing&gclid=xyz&sort=asc";
    
    let normalized = normalize_url(href, base).unwrap();
    // * Expect: id=123 and sort=asc ONLY.
    assert!(normalized.contains("id=123"));
    assert!(normalized.contains("sort=asc"));
    assert!(!normalized.contains("utm_source"));
    assert!(!normalized.contains("gclid"));
}

#[test]
fn test_query_sorting() {
    let base = "https://example.com";
    // * Input order: b, a, c
    let href = "/search?b=2&a=1&c=3";
    
    let normalized = normalize_url(href, base).unwrap();
    // * Expect sorted order: a=1&b=2&c=3
    assert_eq!(normalized, "https://example.com/search?a=1&b=2&c=3");
}

#[test]
fn test_invalid_base() {
    let base = "not_a_url";
    let href = "page";
    assert_eq!(normalize_url(href, base), None);
}
