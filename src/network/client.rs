use crate::network::errors::NetworkError;
use crate::network::identity::IdentityProfile;
use reqwest::{Client, Proxy};
use reqwest::header::HeaderMap;
use regex::Regex;
use std::time::Duration;

// * The Primary HTTP Engine for the Fast Path.
pub struct FastClient {
    inner: Client,
    ban_title_regex: Regex,
    // * Test-only flag to simulate failures [Unit Testing]
    #[cfg(test)]
    simulate_fail_code: Option<u16>,
}

impl FastClient {
    // * Initializes the client. 
    // * Optional `proxy_url` (scheme://user:pass@host:port).
    pub fn new(proxy_url: Option<&str>) -> Result<Self, NetworkError> {
        // * Build Identity Headers [EDD-1.2]
        // * We manually apply headers since we switched to reqwest to fix Windows build.
        let profile = IdentityProfile::generate_chrome_120();
        let mut headers = HeaderMap::new();
        profile.apply_to_headers(&mut headers);

        let mut builder = Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .timeout(Duration::from_secs(30));

        // * Proxy Injection
        if let Some(url) = proxy_url {
            let proxy = Proxy::all(url)
                .map_err(|e| NetworkError::Reqwest(e))?;
            builder = builder.proxy(proxy);
        }

        let client = builder.build()?;

        let ban_regex = Regex::new(r"(?i)(Just a moment|Attention Required|Security Check|Access Denied|Cloudflare|Captcha)")
            .expect("! CRITICAL: Failed to compile Soft Ban Regex");

        Ok(Self {
            inner: client,
            ban_title_regex: ban_regex,
            #[cfg(test)]
            simulate_fail_code: None,
        })
    }

    #[cfg(test)]
    pub fn with_simulation_mode(mut self, code: u16) -> Self {
        self.simulate_fail_code = Some(code);
        self
    }

    // * Fetches a URL and validates the response against Soft Ban rules.
    pub async fn fetch(&self, url: &str) -> Result<String, NetworkError> {
        // * SIMULATION HOOK FOR TESTS
        #[cfg(test)]
        if let Some(code) = self.simulate_fail_code {
            if code == 403 { return Err(NetworkError::HardBan(403)); }
            if code == 200 { return Err(NetworkError::SoftBan("Simulated SoftBan".into())); }
        }

        let resp = self.inner.get(url).send().await?;
        let status = resp.status();

        if status.as_u16() == 403 || status.as_u16() == 429 {
            return Err(NetworkError::HardBan(status.as_u16()));
        }

        if !status.is_success() {
            return Err(NetworkError::Reqwest(resp.error_for_status().unwrap_err()));
        }

        let body = resp.text().await?;
        
        if body.len() < 500 {
            return Err(NetworkError::EmptyResponse(body.len()));
        }

        self.detect_soft_ban(&body)?;

        Ok(body)
    }

    fn detect_soft_ban(&self, body: &str) -> Result<(), NetworkError> {
        if let Some(cap) = self.ban_title_regex.find(body) {
            return Err(NetworkError::SoftBan(format!("Title Trigger: {}", cap.as_str())));
        }

        let signatures = ["captcha-delivery", "cf-turnstile", "datadome", "challenge-platform"];
        for sig in signatures {
            if body.contains(sig) {
                return Err(NetworkError::SoftBan(format!("Body Trigger: {}", sig)));
            }
        }

        Ok(())
    }
}