use titan_flow::network::identity::IdentityProfile;
use rquest::header::HeaderMap;

#[test]
fn test_generate_chrome_120_structure() {
    let profile = IdentityProfile::generate_chrome_120();
    assert_eq!(profile.chrome_version, "120");
    assert!(profile.user_agent.contains("Chrome/120.0.6099.109"));
}

#[test]
fn test_apply_to_headers_integrity() {
    let profile = IdentityProfile::generate_chrome_120();
    let mut headers = HeaderMap::new();
    profile.apply_to_headers(&mut headers);

    let ua = headers.get("User-Agent").unwrap().to_str().unwrap();
    assert_eq!(ua, profile.user_agent);
    
    // * Check rquest/reqwest specific header handling if needed, 
    // * but strictly we just check the map contents.
    assert_eq!(headers.get("sec-ch-ua-mobile").unwrap(), "?0");
}
