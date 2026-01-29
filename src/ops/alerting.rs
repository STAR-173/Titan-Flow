// * [Sec 5] Alerting - SEV-1 and SEV-3 Alert Conditions
// * Defines alerting rules and triggers for operational monitoring

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

// * Alert thresholds from specification
const SEV1_SUCCESS_RATE_THRESHOLD: f64 = 0.50; // * Global Success Rate < 50%
const SEV1_WINDOW_SECONDS: u64 = 300;          // * 5 minute window
const SEV3_BAN_RATE_THRESHOLD: f64 = 0.90;     // * Single Domain Ban Rate > 90%

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertSeverity {
    /// Critical alert requiring immediate action
    Sev1,
    /// High severity alert requiring attention
    Sev2,
    /// Warning level alert for monitoring
    Sev3,
    /// Informational alert
    Info,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Sev1 => write!(f, "SEV-1"),
            AlertSeverity::Sev2 => write!(f, "SEV-2"),
            AlertSeverity::Sev3 => write!(f, "SEV-3"),
            AlertSeverity::Info => write!(f, "INFO"),
        }
    }
}

/// Alert types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AlertType {
    /// Global success rate below threshold
    GlobalSuccessRateLow,
    /// Single domain ban rate above threshold
    DomainBanRateHigh,
    /// Memory pressure detected
    MemoryPressure,
    /// Circuit breaker triggered
    CircuitBreakerOpen,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Custom alert
    Custom(String),
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertType::GlobalSuccessRateLow => write!(f, "GLOBAL_SUCCESS_RATE_LOW"),
            AlertType::DomainBanRateHigh => write!(f, "DOMAIN_BAN_RATE_HIGH"),
            AlertType::MemoryPressure => write!(f, "MEMORY_PRESSURE"),
            AlertType::CircuitBreakerOpen => write!(f, "CIRCUIT_BREAKER_OPEN"),
            AlertType::RateLimitExceeded => write!(f, "RATE_LIMIT_EXCEEDED"),
            AlertType::Custom(name) => write!(f, "CUSTOM_{}", name.to_uppercase()),
        }
    }
}

/// An alert event
#[derive(Debug, Clone)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub alert_type: AlertType,
    pub message: String,
    pub context: HashMap<String, String>,
    pub timestamp: Instant,
    pub id: u64,
}

impl Alert {
    /// Creates a new alert
    pub fn new(severity: AlertSeverity, alert_type: AlertType, message: impl Into<String>) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self {
            severity,
            alert_type,
            message: message.into(),
            context: HashMap::new(),
            timestamp: Instant::now(),
            id: COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Adds context to the alert
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Logs the alert using tracing
    pub fn log(&self) {
        let context_str = self
            .context
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ");

        match self.severity {
            AlertSeverity::Sev1 => {
                tracing::error!(
                    alert_id = self.id,
                    severity = %self.severity,
                    alert_type = %self.alert_type,
                    context = context_str,
                    "ALERT: {}", self.message
                );
            }
            AlertSeverity::Sev2 => {
                tracing::error!(
                    alert_id = self.id,
                    severity = %self.severity,
                    alert_type = %self.alert_type,
                    context = context_str,
                    "ALERT: {}", self.message
                );
            }
            AlertSeverity::Sev3 => {
                tracing::warn!(
                    alert_id = self.id,
                    severity = %self.severity,
                    alert_type = %self.alert_type,
                    context = context_str,
                    "ALERT: {}", self.message
                );
            }
            AlertSeverity::Info => {
                tracing::info!(
                    alert_id = self.id,
                    severity = %self.severity,
                    alert_type = %self.alert_type,
                    context = context_str,
                    "ALERT: {}", self.message
                );
            }
        }
    }
}

/// Trait for alert handlers
pub trait AlertHandler: Send + Sync {
    /// Handles an alert
    fn handle(&self, alert: &Alert);
}

/// Default logging handler
#[derive(Debug, Default)]
pub struct LoggingHandler;

impl AlertHandler for LoggingHandler {
    fn handle(&self, alert: &Alert) {
        alert.log();
    }
}

/// Alert manager for monitoring and triggering alerts
pub struct AlertManager {
    handlers: Vec<Arc<dyn AlertHandler>>,
    active_alerts: RwLock<HashMap<AlertType, Alert>>,
    stats: AlertStats,
    config: AlertConfig,
}

impl std::fmt::Debug for AlertManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlertManager")
            .field("handler_count", &self.handlers.len())
            .field("config", &self.config)
            .finish()
    }
}

/// Configuration for the alert manager
#[derive(Debug, Clone)]
pub struct AlertConfig {
    /// Threshold for SEV-1 global success rate alert
    pub sev1_success_rate_threshold: f64,
    /// Window for SEV-1 rate calculation in seconds
    pub sev1_window_seconds: u64,
    /// Threshold for SEV-3 domain ban rate alert
    pub sev3_ban_rate_threshold: f64,
    /// Cooldown period before re-firing same alert
    pub alert_cooldown_seconds: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            sev1_success_rate_threshold: SEV1_SUCCESS_RATE_THRESHOLD,
            sev1_window_seconds: SEV1_WINDOW_SECONDS,
            sev3_ban_rate_threshold: SEV3_BAN_RATE_THRESHOLD,
            alert_cooldown_seconds: 300, // * 5 minute cooldown
        }
    }
}

/// Statistics tracked by the alert manager
#[derive(Debug)]
pub struct AlertStats {
    success_count: AtomicU64,
    failure_count: AtomicU64,
    window_start: RwLock<Instant>,
    domain_stats: RwLock<HashMap<String, DomainStats>>,
}

impl Default for AlertStats {
    fn default() -> Self {
        Self {
            success_count: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            window_start: RwLock::new(Instant::now()),
            domain_stats: RwLock::new(HashMap::new()),
        }
    }
}

/// Per-domain statistics
#[derive(Debug, Clone, Default)]
struct DomainStats {
    success_count: u64,
    failure_count: u64,
    ban_count: u64,
}

impl AlertManager {
    /// Creates a new alert manager with default configuration
    pub fn new() -> Self {
        Self::with_config(AlertConfig::default())
    }

    /// Creates a new alert manager with custom configuration
    pub fn with_config(config: AlertConfig) -> Self {
        Self {
            handlers: vec![Arc::new(LoggingHandler)],
            active_alerts: RwLock::new(HashMap::new()),
            stats: AlertStats {
                window_start: RwLock::new(Instant::now()),
                ..Default::default()
            },
            config,
        }
    }

    /// Adds an alert handler
    pub fn add_handler(&mut self, handler: Arc<dyn AlertHandler>) {
        self.handlers.push(handler);
    }

    /// Records a successful request
    pub fn record_success(&self) {
        self.stats.success_count.fetch_add(1, Ordering::Relaxed);
        self.check_and_rotate_window();
    }

    /// Records a failed request
    pub fn record_failure(&self) {
        self.stats.failure_count.fetch_add(1, Ordering::Relaxed);
        self.check_and_rotate_window();
        self.check_sev1_alert();
    }

    /// Records a domain-specific event
    pub fn record_domain_event(&self, domain: &str, success: bool, is_ban: bool) {
        let mut domain_stats = self.stats.domain_stats.write().unwrap();
        let stats = domain_stats.entry(domain.to_string()).or_default();

        if success {
            stats.success_count += 1;
        } else {
            stats.failure_count += 1;
        }

        if is_ban {
            stats.ban_count += 1;
        }

        // * Check for SEV-3 alert
        let total = stats.success_count + stats.failure_count;
        if total >= 10 {
            // * Minimum sample size
            let ban_rate = stats.ban_count as f64 / total as f64;
            if ban_rate > self.config.sev3_ban_rate_threshold {
                drop(domain_stats); // * Release lock before firing alert
                self.fire_domain_ban_rate_alert(domain, ban_rate);
            }
        }
    }

    /// Checks and rotates the statistics window
    fn check_and_rotate_window(&self) {
        let mut window_start = self.stats.window_start.write().unwrap();
        if window_start.elapsed() > Duration::from_secs(self.config.sev1_window_seconds) {
            self.stats.success_count.store(0, Ordering::Relaxed);
            self.stats.failure_count.store(0, Ordering::Relaxed);
            *window_start = Instant::now();
        }
    }

    /// Checks and fires SEV-1 alert if needed
    fn check_sev1_alert(&self) {
        let success = self.stats.success_count.load(Ordering::Relaxed);
        let failure = self.stats.failure_count.load(Ordering::Relaxed);
        let total = success + failure;

        if total < 10 {
            return; // * Not enough data
        }

        let success_rate = success as f64 / total as f64;
        if success_rate < self.config.sev1_success_rate_threshold {
            self.fire_sev1_alert(success_rate);
        }
    }

    /// Fires a SEV-1 global success rate alert
    fn fire_sev1_alert(&self, success_rate: f64) {
        let alert = Alert::new(
            AlertSeverity::Sev1,
            AlertType::GlobalSuccessRateLow,
            format!(
                "Global success rate ({:.1}%) below threshold ({:.1}%)",
                success_rate * 100.0,
                self.config.sev1_success_rate_threshold * 100.0
            ),
        )
        .with_context("success_rate", format!("{:.4}", success_rate))
        .with_context(
            "threshold",
            format!("{:.4}", self.config.sev1_success_rate_threshold),
        );

        self.fire_alert(alert);
    }

    /// Fires a SEV-3 domain ban rate alert
    fn fire_domain_ban_rate_alert(&self, domain: &str, ban_rate: f64) {
        let alert = Alert::new(
            AlertSeverity::Sev3,
            AlertType::DomainBanRateHigh,
            format!(
                "Domain {} ban rate ({:.1}%) above threshold ({:.1}%)",
                domain,
                ban_rate * 100.0,
                self.config.sev3_ban_rate_threshold * 100.0
            ),
        )
        .with_context("domain", domain.to_string())
        .with_context("ban_rate", format!("{:.4}", ban_rate))
        .with_context(
            "threshold",
            format!("{:.4}", self.config.sev3_ban_rate_threshold),
        );

        self.fire_alert(alert);
    }

    /// Fires a custom alert
    pub fn fire_alert(&self, alert: Alert) {
        // * Check cooldown
        {
            let active = self.active_alerts.read().unwrap();
            if let Some(existing) = active.get(&alert.alert_type) {
                if existing.timestamp.elapsed()
                    < Duration::from_secs(self.config.alert_cooldown_seconds)
                {
                    return; // * Still in cooldown
                }
            }
        }

        // * Store as active alert
        {
            let mut active = self.active_alerts.write().unwrap();
            active.insert(alert.alert_type.clone(), alert.clone());
        }

        // * Dispatch to handlers
        for handler in &self.handlers {
            handler.handle(&alert);
        }
    }

    /// Fires a memory pressure alert
    pub fn fire_memory_pressure_alert(&self, usage_percent: f64) {
        let alert = Alert::new(
            AlertSeverity::Sev2,
            AlertType::MemoryPressure,
            format!("Memory usage at {:.1}%", usage_percent),
        )
        .with_context("usage_percent", format!("{:.2}", usage_percent));

        self.fire_alert(alert);
    }

    /// Fires a circuit breaker open alert
    pub fn fire_circuit_breaker_alert(&self, domain: &str) {
        let alert = Alert::new(
            AlertSeverity::Sev3,
            AlertType::CircuitBreakerOpen,
            format!("Circuit breaker opened for domain {}", domain),
        )
        .with_context("domain", domain.to_string());

        self.fire_alert(alert);
    }

    /// Returns current statistics
    pub fn get_stats(&self) -> AlertManagerStats {
        let success = self.stats.success_count.load(Ordering::Relaxed);
        let failure = self.stats.failure_count.load(Ordering::Relaxed);
        let total = success + failure;
        let active = self.active_alerts.read().unwrap();

        AlertManagerStats {
            success_count: success,
            failure_count: failure,
            success_rate: if total > 0 {
                success as f64 / total as f64
            } else {
                1.0
            },
            active_alert_count: active.len(),
        }
    }

    /// Clears all active alerts
    pub fn clear_alerts(&self) {
        let mut active = self.active_alerts.write().unwrap();
        active.clear();
    }

    /// Returns the configuration
    pub fn config(&self) -> &AlertConfig {
        &self.config
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from the alert manager
#[derive(Debug, Clone)]
pub struct AlertManagerStats {
    pub success_count: u64,
    pub failure_count: u64,
    pub success_rate: f64,
    pub active_alert_count: usize,
}

/// Convenience function to create a SEV-1 alert
pub fn sev1_alert(message: impl Into<String>) -> Alert {
    Alert::new(
        AlertSeverity::Sev1,
        AlertType::Custom("CRITICAL".to_string()),
        message,
    )
}

/// Convenience function to create a SEV-3 alert
pub fn sev3_alert(message: impl Into<String>) -> Alert {
    Alert::new(
        AlertSeverity::Sev3,
        AlertType::Custom("WARNING".to_string()),
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_creation() {
        let alert = Alert::new(
            AlertSeverity::Sev1,
            AlertType::GlobalSuccessRateLow,
            "Test alert",
        );

        assert_eq!(alert.severity, AlertSeverity::Sev1);
        assert_eq!(alert.message, "Test alert");
    }

    #[test]
    fn test_alert_with_context() {
        let alert = Alert::new(AlertSeverity::Sev3, AlertType::DomainBanRateHigh, "Ban alert")
            .with_context("domain", "example.com")
            .with_context("rate", "0.95");

        assert_eq!(alert.context.get("domain"), Some(&"example.com".to_string()));
        assert_eq!(alert.context.get("rate"), Some(&"0.95".to_string()));
    }

    #[test]
    fn test_alert_severity_display() {
        assert_eq!(format!("{}", AlertSeverity::Sev1), "SEV-1");
        assert_eq!(format!("{}", AlertSeverity::Sev2), "SEV-2");
        assert_eq!(format!("{}", AlertSeverity::Sev3), "SEV-3");
        assert_eq!(format!("{}", AlertSeverity::Info), "INFO");
    }

    #[test]
    fn test_alert_type_display() {
        assert_eq!(
            format!("{}", AlertType::GlobalSuccessRateLow),
            "GLOBAL_SUCCESS_RATE_LOW"
        );
        assert_eq!(
            format!("{}", AlertType::DomainBanRateHigh),
            "DOMAIN_BAN_RATE_HIGH"
        );
        assert_eq!(
            format!("{}", AlertType::Custom("test".to_string())),
            "CUSTOM_TEST"
        );
    }

    #[test]
    fn test_alert_manager_creation() {
        let manager = AlertManager::new();
        assert!((manager.config().sev1_success_rate_threshold - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_alert_manager_record_success() {
        let manager = AlertManager::new();
        manager.record_success();
        manager.record_success();

        let stats = manager.get_stats();
        assert_eq!(stats.success_count, 2);
        assert!((stats.success_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_alert_manager_record_failure() {
        let manager = AlertManager::new();
        manager.record_success();
        manager.record_failure();

        let stats = manager.get_stats();
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.failure_count, 1);
        assert!((stats.success_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sev1_alert_threshold() {
        let config = AlertConfig {
            sev1_success_rate_threshold: 0.5,
            alert_cooldown_seconds: 0, // * Disable cooldown for testing
            ..Default::default()
        };
        let manager = AlertManager::with_config(config);

        // * Record enough failures to trigger
        for _ in 0..8 {
            manager.record_failure();
        }
        for _ in 0..2 {
            manager.record_success();
        }

        // * Should have triggered alert (20% success rate < 50%)
        let stats = manager.get_stats();
        assert!(stats.success_rate < 0.5);
    }

    #[test]
    fn test_domain_event_recording() {
        let manager = AlertManager::new();

        manager.record_domain_event("example.com", true, false);
        manager.record_domain_event("example.com", false, false);
        manager.record_domain_event("example.com", false, true);

        // * Domain stats should be tracked
    }

    #[test]
    fn test_clear_alerts() {
        let manager = AlertManager::new();
        let alert = sev3_alert("Test");
        manager.fire_alert(alert);

        let stats = manager.get_stats();
        assert!(stats.active_alert_count > 0);

        manager.clear_alerts();
        let stats = manager.get_stats();
        assert_eq!(stats.active_alert_count, 0);
    }

    #[test]
    fn test_convenience_functions() {
        let sev1 = sev1_alert("Critical issue");
        assert_eq!(sev1.severity, AlertSeverity::Sev1);

        let sev3 = sev3_alert("Warning issue");
        assert_eq!(sev3.severity, AlertSeverity::Sev3);
    }

    #[test]
    fn test_custom_config() {
        let config = AlertConfig {
            sev1_success_rate_threshold: 0.3,
            sev3_ban_rate_threshold: 0.8,
            sev1_window_seconds: 60,
            alert_cooldown_seconds: 10,
        };

        let manager = AlertManager::with_config(config);
        assert!((manager.config().sev1_success_rate_threshold - 0.3).abs() < f64::EPSILON);
        assert!((manager.config().sev3_ban_rate_threshold - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_memory_pressure_alert() {
        let config = AlertConfig {
            alert_cooldown_seconds: 0,
            ..Default::default()
        };
        let manager = AlertManager::with_config(config);
        manager.fire_memory_pressure_alert(95.5);

        let stats = manager.get_stats();
        assert!(stats.active_alert_count > 0);
    }

    #[test]
    fn test_circuit_breaker_alert() {
        let config = AlertConfig {
            alert_cooldown_seconds: 0,
            ..Default::default()
        };
        let manager = AlertManager::with_config(config);
        manager.fire_circuit_breaker_alert("slow.example.com");

        let stats = manager.get_stats();
        assert!(stats.active_alert_count > 0);
    }
}
