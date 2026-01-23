// * [NFR-02] Memory Adaptive Dispatcher
// * Monitors system RAM usage and throttles crawling when memory pressure is detected

use std::sync::Arc;
use std::time::Duration;
use sysinfo::System;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

// * Memory pressure thresholds (percentage)
const PRESSURE_THRESHOLD_HIGH: f64 = 90.0;
const PRESSURE_THRESHOLD_LOW: f64 = 85.0;
const MONITOR_INTERVAL_SECS: u64 = 1;

// * MemoryMonitor tracks system RAM usage and signals when memory pressure is detected
// * Uses hysteresis to prevent rapid state changes (enters at 90%, exits at 85%)
pub struct MemoryMonitor {
    is_under_pressure: Arc<RwLock<bool>>,
    system: Arc<RwLock<System>>,
}

impl MemoryMonitor {
    // * Creates a new MemoryMonitor instance
    pub fn new() -> Self {
        Self {
            is_under_pressure: Arc::new(RwLock::new(false)),
            system: Arc::new(RwLock::new(System::new_all())),
        }
    }

    // * Returns a clone of the pressure state handle for external consumers
    pub fn pressure_handle(&self) -> Arc<RwLock<bool>> {
        Arc::clone(&self.is_under_pressure)
    }

    // * Checks if the system is currently under memory pressure
    pub async fn is_under_pressure(&self) -> bool {
        *self.is_under_pressure.read().await
    }

    // * Calculates current RAM usage percentage
    async fn get_ram_usage_percent(&self) -> f64 {
        let mut system = self.system.write().await;
        system.refresh_memory();

        let total_memory = system.total_memory();
        let used_memory = system.used_memory();

        if total_memory == 0 {
            return 0.0;
        }

        (used_memory as f64 / total_memory as f64) * 100.0
    }

    // * Spawns the background monitoring loop
    // * Runs every 1 second and updates pressure state based on RAM usage
    pub fn spawn_monitor(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(MONITOR_INTERVAL_SECS));
            info!("MemoryMonitor started - polling every {}s", MONITOR_INTERVAL_SECS);

            loop {
                tick.tick().await;

                let ram_percent = self.get_ram_usage_percent().await;
                let currently_under_pressure = *self.is_under_pressure.read().await;

                // * Apply hysteresis logic to prevent rapid state oscillation
                if ram_percent > PRESSURE_THRESHOLD_HIGH && !currently_under_pressure {
                    // * Enter pressure state when RAM exceeds 90%
                    *self.is_under_pressure.write().await = true;
                    warn!(
                        "Memory pressure ACTIVATED - RAM at {:.1}% (threshold: {}%)",
                        ram_percent, PRESSURE_THRESHOLD_HIGH
                    );
                } else if ram_percent < PRESSURE_THRESHOLD_LOW && currently_under_pressure {
                    // * Exit pressure state when RAM drops below 85%
                    *self.is_under_pressure.write().await = false;
                    info!(
                        "Memory pressure RELEASED - RAM at {:.1}% (threshold: {}%)",
                        ram_percent, PRESSURE_THRESHOLD_LOW
                    );
                } else {
                    debug!(
                        "Memory status: {:.1}% RAM used, pressure={}",
                        ram_percent, currently_under_pressure
                    );
                }
            }
        })
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_monitor_creation() {
        let monitor = MemoryMonitor::new();
        // * Initially should not be under pressure
        assert!(!monitor.is_under_pressure().await);
    }

    #[tokio::test]
    async fn test_ram_usage_returns_valid_percentage() {
        let monitor = MemoryMonitor::new();
        let usage = monitor.get_ram_usage_percent().await;
        // * RAM usage should be between 0 and 100
        assert!(usage >= 0.0 && usage <= 100.0);
    }

    #[tokio::test]
    async fn test_pressure_handle_is_shared() {
        let monitor = MemoryMonitor::new();
        let handle = monitor.pressure_handle();

        // * Modify through handle
        *handle.write().await = true;

        // * Should reflect in monitor
        assert!(monitor.is_under_pressure().await);
    }
}
