use url::Url;
use std::collections::{HashSet, BTreeMap};

// * Normalizes a URL to ensure a unique, deterministic representation.
// * This is critical for the deduplication engine and caching layer.
// *
// * Logic [EDD-3.2]:
// * 1. Join href with base_url.
// * 2. Strip Fragment (#).
// * 3. Lowercase Hostname.
// * 4. Remove Tracking Parameters (utm_*, gclid, etc.).
// * 5. Sort Query Parameters alphabetically.
pub fn normalize_url(href: &str, base_url: &str) -> Option<String> {
    // * Step 1: Parse Base and Join
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return None, // ? Should we log this error?
    };

    let mut url = match base.join(href) {
        Ok(u) => u,
        Err(_) => return None,
    };

    // * Step 2: Strip Fragment
    // * Fragments are client-side only and irrelevant for crawling uniqueness.
    url.set_fragment(None);

    // * Step 3: Lowercase Hostname
    // * DNS is case-insensitive, but string hashing is not.
    if let Some(host) = url.host_str() {
        let lower_host = host.to_lowercase();
        if url.set_host(Some(&lower_host)).is_err() {
            return None;
        }
    }

    // * Step 4 & 5: Filter and Sort Query Params
    // * We uses a BTreeMap to automatically sort keys alphabetically.
    let mut clean_pairs = BTreeMap::new();
    
    // * Tracking parameters to strip [EDD-3.2]
    // ! CRITICAL: Add new tracking params here as they are discovered.
    let drop_params: HashSet<&str> = [
        "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
        "gclid", "fbclid", "ref", "yclid", "_ga"
    ].into();

    for (k, v) in url.query_pairs() {
        let key_lower = k.to_lowercase();
        if !drop_params.contains(key_lower.as_str()) {
            // * Keep the original casing of the key/value, just sort by key
            clean_pairs.insert(k.into_owned(), v.into_owned());
        }
    }

    // * Reconstruct query string
    if clean_pairs.is_empty() {
        url.set_query(None);
    } else {
        let mut serializer = url.query_pairs_mut();
        serializer.clear();
        for (k, v) in clean_pairs {
            serializer.append_pair(&k, &v);
        }
    }

    // * Return normalized string
    Some(url.to_string())
}
