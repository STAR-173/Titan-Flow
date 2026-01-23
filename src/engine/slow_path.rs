// * [FR-03] [EDD-3.3] Slow Path - Headless Browser Rendering
// * Uses ChromiumOxide for JavaScript-heavy pages that require full rendering

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::{
    EventRequestPaused, ResourceType,
};
use chromiumoxide::cdp::browser_protocol::fetch::EnableParams;
use chromiumoxide::page::Page;
use chromiumoxide::Handler;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::constants::PAGE_TIMEOUT_MS;

// * [APP-A.1] Stealth payload to mask WebDriver detection
const STEALTH_PAYLOAD: &str = r#"
(() => {
    // * Mask navigator.webdriver
    Object.defineProperty(navigator, 'webdriver', {
        get: () => undefined,
        configurable: true
    });

    // * Mask plugins (Chrome typically has plugins)
    Object.defineProperty(navigator, 'plugins', {
        get: () => {
            const plugins = [
                { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer' },
                { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai' },
                { name: 'Native Client', filename: 'internal-nacl-plugin' }
            ];
            plugins.item = (i) => plugins[i];
            plugins.namedItem = (name) => plugins.find(p => p.name === name);
            plugins.refresh = () => {};
            return plugins;
        },
        configurable: true
    });

    // * Mask languages
    Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en'],
        configurable: true
    });

    // * Set hardwareConcurrency to 4 (common value)
    Object.defineProperty(navigator, 'hardwareConcurrency', {
        get: () => 4,
        configurable: true
    });

    // * Mask permissions query
    const originalQuery = window.navigator.permissions.query;
    window.navigator.permissions.query = (parameters) => (
        parameters.name === 'notifications' ?
            Promise.resolve({ state: Notification.permission }) :
            originalQuery(parameters)
    );

    // * Remove automation indicators from window
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;
})();
"#;

// * Console capture script to collect JS errors and logs
const CONSOLE_CAPTURE_JS: &str = r#"
(() => {
    window.__titanConsoleLogs = [];
    const originalConsole = { ...console };

    ['log', 'warn', 'error', 'info', 'debug'].forEach(method => {
        console[method] = (...args) => {
            window.__titanConsoleLogs.push({
                type: method,
                timestamp: Date.now(),
                message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
            });
            originalConsole[method](...args);
        };
    });

    window.addEventListener('error', (event) => {
        window.__titanConsoleLogs.push({
            type: 'uncaught_error',
            timestamp: Date.now(),
            message: `${event.message} at ${event.filename}:${event.lineno}`
        });
    });
})();
"#;

// * Resource types to block for performance
const BLOCKED_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp", ".woff", ".woff2", ".ttf", ".mp4", ".webm", ".css"];

#[derive(Debug, Error)]
pub enum SlowPathError {
    #[error("Browser launch failed: {0}")]
    BrowserLaunch(String),

    #[error("Page navigation failed: {0}")]
    Navigation(String),

    #[error("Page timeout after {0}ms")]
    Timeout(u64),

    #[error("Script injection failed: {0}")]
    ScriptInjection(String),

    #[error("Content extraction failed: {0}")]
    ContentExtraction(String),

    #[error("Browser crashed")]
    BrowserCrash,
}

// * Result of slow path rendering
#[derive(Debug)]
pub struct SlowPathResult {
    pub html: String,
    pub console_logs: Vec<ConsoleLogEntry>,
    pub final_url: String,
}

#[derive(Debug, Clone)]
pub struct ConsoleLogEntry {
    pub log_type: String,
    pub timestamp: u64,
    pub message: String,
}

// * SlowPathRenderer manages headless browser instances
pub struct SlowPathRenderer {
    browser: Option<Browser>,
    handler: Option<tokio::task::JoinHandle<()>>,
}

impl SlowPathRenderer {
    // * Creates a new renderer (browser not launched until needed)
    pub fn new() -> Self {
        Self {
            browser: None,
            handler: None,
        }
    }

    // * Launches the browser if not already running
    pub async fn ensure_browser(&mut self) -> Result<&Browser, SlowPathError> {
        if self.browser.is_none() {
            let config = BrowserConfig::builder()
                .with_head()
                .no_sandbox()
                .viewport(None)
                .arg("--disable-blink-features=AutomationControlled")
                .arg("--disable-infobars")
                .arg("--disable-dev-shm-usage")
                .arg("--disable-gpu")
                .build()
                .map_err(|e| SlowPathError::BrowserLaunch(e.to_string()))?;

            let (browser, mut handler) = Browser::launch(config)
                .await
                .map_err(|e| SlowPathError::BrowserLaunch(e.to_string()))?;

            // * Spawn handler in background
            let handle = tokio::spawn(async move {
                while let Some(_event) = handler.next().await {
                    // * Process browser events
                }
            });

            self.browser = Some(browser);
            self.handler = Some(handle);
            info!("SlowPathRenderer browser launched");
        }

        Ok(self.browser.as_ref().unwrap())
    }

    // * Renders a page and returns the final HTML
    pub async fn render(&mut self, url: &str) -> Result<SlowPathResult, SlowPathError> {
        let browser = self.ensure_browser().await?;

        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| SlowPathError::Navigation(e.to_string()))?;

        // * Inject stealth script before navigation
        page.evaluate(STEALTH_PAYLOAD)
            .await
            .map_err(|e| SlowPathError::ScriptInjection(e.to_string()))?;

        // * Inject console capture
        page.evaluate(CONSOLE_CAPTURE_JS)
            .await
            .map_err(|e| SlowPathError::ScriptInjection(e.to_string()))?;

        // * Navigate with timeout
        let timeout = Duration::from_millis(PAGE_TIMEOUT_MS);
        let navigate_result = tokio::time::timeout(timeout, page.goto(url)).await;

        match navigate_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(SlowPathError::Navigation(e.to_string())),
            Err(_) => return Err(SlowPathError::Timeout(PAGE_TIMEOUT_MS)),
        }

        // * Wait for page to settle
        tokio::time::sleep(Duration::from_millis(500)).await;

        // * Get final URL after redirects
        let final_url = page
            .url()
            .await
            .map_err(|e| SlowPathError::ContentExtraction(e.to_string()))?
            .unwrap_or_else(|| url.to_string());

        // * Extract HTML content
        let html = page
            .content()
            .await
            .map_err(|e| SlowPathError::ContentExtraction(e.to_string()))?;

        // * Extract console logs
        let console_logs = self.extract_console_logs(&page).await;

        // * Close the page
        let _ = page.close().await;

        Ok(SlowPathResult {
            html,
            console_logs,
            final_url,
        })
    }

    // * Extracts captured console logs from the page
    async fn extract_console_logs(&self, page: &Page) -> Vec<ConsoleLogEntry> {
        let result = page
            .evaluate("JSON.stringify(window.__titanConsoleLogs || [])")
            .await;

        match result {
            Ok(value) => {
                if let Some(json_str) = value.into_value::<String>().ok() {
                    serde_json::from_str::<Vec<serde_json::Value>>(&json_str)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|v| {
                            Some(ConsoleLogEntry {
                                log_type: v.get("type")?.as_str()?.to_string(),
                                timestamp: v.get("timestamp")?.as_u64()?,
                                message: v.get("message")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
            Err(_) => Vec::new(),
        }
    }

    // * Closes the browser gracefully
    pub async fn shutdown(&mut self) {
        if let Some(browser) = self.browser.take() {
            let _ = browser.close().await;
        }
        if let Some(handler) = self.handler.take() {
            handler.abort();
        }
        info!("SlowPathRenderer shutdown complete");
    }
}

impl Default for SlowPathRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SlowPathRenderer {
    fn drop(&mut self) {
        // * Best effort cleanup - can't await in drop
        if let Some(handler) = self.handler.take() {
            handler.abort();
        }
    }
}

// * Checks if a URL should be blocked based on extension
pub fn should_block_resource(url: &str) -> bool {
    let lower_url = url.to_lowercase();
    BLOCKED_EXTENSIONS
        .iter()
        .any(|ext| lower_url.contains(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_block_resource() {
        assert!(should_block_resource("https://example.com/image.png"));
        assert!(should_block_resource("https://example.com/style.css"));
        assert!(should_block_resource("https://example.com/font.woff2"));
        assert!(should_block_resource("https://example.com/video.mp4"));
        assert!(!should_block_resource("https://example.com/page.html"));
        assert!(!should_block_resource("https://example.com/api/data"));
    }

    #[test]
    fn test_stealth_payload_contains_required_masks() {
        assert!(STEALTH_PAYLOAD.contains("webdriver"));
        assert!(STEALTH_PAYLOAD.contains("plugins"));
        assert!(STEALTH_PAYLOAD.contains("languages"));
        assert!(STEALTH_PAYLOAD.contains("hardwareConcurrency"));
    }

    #[test]
    fn test_console_capture_js_captures_methods() {
        assert!(CONSOLE_CAPTURE_JS.contains("log"));
        assert!(CONSOLE_CAPTURE_JS.contains("warn"));
        assert!(CONSOLE_CAPTURE_JS.contains("error"));
        assert!(CONSOLE_CAPTURE_JS.contains("__titanConsoleLogs"));
    }
}
