use rquest::header::{HeaderMap, HeaderValue};

// * IdentityProfile defines the browser fingerprinting characteristics.
pub struct IdentityProfile {
    pub chrome_version: &'static str,
    pub user_agent: String,
    pub sec_ch_ua: String,
    pub sec_ch_ua_platform: String,
}

impl IdentityProfile {
    // * Generates a specific Chrome 120 profile to match TLS fingerprints.
    pub fn generate_chrome_120() -> Self {
        let major_version = "120";
        let full_version = "120.0.6099.109";

        Self {
            chrome_version: major_version,
            user_agent: format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{} Safari/537.36",
                full_version
            ),
            sec_ch_ua: format!(
                r#""Chromium";v="{}", "Google Chrome";v="{}", "Not_A Brand";v="99""#,
                major_version, major_version
            ),
            sec_ch_ua_platform: r#""Windows""#.to_string(),
        }
    }

    // * Applies the configured profile to a mutable HeaderMap.
    pub fn apply_to_headers(&self, headers: &mut HeaderMap) {
        headers.insert(
            "User-Agent", 
            HeaderValue::from_str(&self.user_agent).expect("! CRITICAL: Invalid UA")
        );
        headers.insert(
            "sec-ch-ua", 
            HeaderValue::from_str(&self.sec_ch_ua).expect("! CRITICAL: Invalid sec-ch-ua")
        );
        headers.insert(
            "sec-ch-ua-platform", 
            HeaderValue::from_str(&self.sec_ch_ua_platform).expect("! CRITICAL: Invalid platform")
        );
        headers.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
        headers.insert("Upgrade-Insecure-Requests", HeaderValue::from_static("1"));
        headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
    }
}
