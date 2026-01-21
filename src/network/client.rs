use crate::network::errors::NetworkError;
use rquest::Impersonate;
use rquest::Client;
use regex::Regex;
use std::time::Duration;

// * The Primary HTTP Engine for the Fast Path.
pub struct FastClient {
    inner: Client,
    ban_title_regex: Regex,
}

impl FastClient {
    // * Initializes the client with Chrome 120 Identity.
    // * @param proxy_url - Optional proxy URL (e.g., "http://user:pass@ip:port")
    pub fn new(proxy_url: Option<&str>) -> Result<Self, NetworkError> {
        let mut builder = Client::builder()
            .impersonate(Impersonate::Chrome120)
            .enable_ech_grease(true)
            .permute_extensions(true)
            .cookie_store(true)
            .timeout(Duration::from_secs(30));

        // * Apply Proxy if provided
        if let Some(url) = proxy_url {
            builder = builder.proxy(rquest::Proxy::all(url)?);
        }

        let client = builder.build()?;

        let ban_regex = Regex::new(r"(?i)(Just a moment|Attention Required|Security Check|Access Denied|Cloudflare|Captcha)")
            .expect("! CRITICAL: Failed to compile Soft Ban Regex");

        Ok(Self {
            inner: client,
            ban_title_regex: ban_regex,
        })
    }

    // * Fetches a URL and validates the response against Soft Ban rules.
    pub async fn fetch(&self, url: &str) -> Result<String, NetworkError> {
        let resp = self.inner.get(url).send().await?;
        let status = resp.status();

        if status.as_u16() == 403 || status.as_u16() == 429 {
            return Err(NetworkError::HardBan(status.as_u16()));
        }

        if !status.is_success() {
            return Err(NetworkError::Rquest(resp.error_for_status().unwrap_err()));
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
