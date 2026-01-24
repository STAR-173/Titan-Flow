use crate::network::client::FastClient;
use crate::network::errors::NetworkError;
use std::sync::Arc;
use tokio::sync::RwLock;

// * The Ladder Levels
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProxyTier {
    Tier0Direct,      // * FIXED: CamelCase
    Tier1Datacenter,  // * FIXED: CamelCase
    Tier2Residential, // * FIXED: CamelCase
}

// * Manages the rotation and escalation of proxies.
pub struct ProxyManager {
    tier1_proxies: Vec<String>,
    tier2_proxies: Vec<String>,
    t1_idx: Arc<RwLock<usize>>,
    t2_idx: Arc<RwLock<usize>>,
}

impl ProxyManager {
    pub fn new(tier1: Vec<String>, tier2: Vec<String>) -> Self {
        Self {
            tier1_proxies: tier1,
            tier2_proxies: tier2,
            t1_idx: Arc::new(RwLock::new(0)),
            t2_idx: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn fetch_with_escalation(&self, url: &str) -> Result<String, NetworkError> {
        // --- TIER 0 ---
        match self.execute_tier(ProxyTier::Tier0Direct, url).await {
            Ok(body) => return Ok(body),
            Err(e) => {
                if !self.should_escalate(&e) { return Err(e); }
                tracing::warn!("Escalating to Tier 1 for {}", url);
            }
        }

        // --- TIER 1 ---
        match self.execute_tier(ProxyTier::Tier1Datacenter, url).await {
            Ok(body) => return Ok(body),
            Err(e) => {
                if !self.should_escalate(&e) { return Err(e); }
                tracing::warn!("Escalating to Tier 2 for {}", url);
            }
        }

        // --- TIER 2 ---
        self.execute_tier(ProxyTier::Tier2Residential, url).await
    }

    async fn execute_tier(&self, tier: ProxyTier, url: &str) -> Result<String, NetworkError> {
        let proxy_url = match tier {
            ProxyTier::Tier0Direct => None,
            ProxyTier::Tier1Datacenter => self.get_next_proxy(1).await,
            ProxyTier::Tier2Residential => self.get_next_proxy(2).await,
        };

        // * Build Client
        let client = FastClient::new(proxy_url.as_deref())?;
        
        // * TEST HOOK
        #[cfg(test)]
        let client = self.inject_simulation(client, tier, url);

        client.fetch(url).await
    }

    async fn get_next_proxy(&self, tier: u8) -> Option<String> {
        match tier {
            1 => {
                if self.tier1_proxies.is_empty() { return None; }
                let mut idx = self.t1_idx.write().await;
                let url = self.tier1_proxies[*idx].clone();
                *idx = (*idx + 1) % self.tier1_proxies.len();
                Some(url)
            },
            2 => {
                if self.tier2_proxies.is_empty() { return None; }
                let mut idx = self.t2_idx.write().await;
                let url = self.tier2_proxies[*idx].clone();
                *idx = (*idx + 1) % self.tier2_proxies.len();
                Some(url)
            },
            _ => None
        }
    }

    fn should_escalate(&self, err: &NetworkError) -> bool {
        match err {
            NetworkError::SoftBan(_) => true,
            NetworkError::HardBan(_) => true,
            NetworkError::EmptyResponse(_) => true, 
            _ => false,
        }
    }

    #[cfg(test)]
    fn inject_simulation(&self, client: FastClient, tier: ProxyTier, url: &str) -> FastClient {
        if url == "https://simulate.fail" {
             match tier {
                ProxyTier::Tier0Direct => return client.with_simulation_mode(403),
                ProxyTier::Tier1Datacenter => return client.with_simulation_mode(200),
                _ => client,
            }
        } else {
            client
        }
    }
}