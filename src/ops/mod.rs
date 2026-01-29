// * Milestone 6: Operations & Deployment
// * Goal: Production observability with telemetry, alerting, and containerization
// * This module provides metrics, logging, and alerting infrastructure

pub mod alerting;
pub mod telemetry;

// * Re-exports for convenient access
pub use alerting::{
    sev1_alert, sev3_alert, Alert, AlertConfig, AlertHandler, AlertManager, AlertManagerStats,
    AlertSeverity, AlertType, LoggingHandler,
};
pub use telemetry::{
    decrement_active_crawlers, get_metrics_string, increment_active_crawlers, init_tracing,
    init_tracing_pretty, init_tracing_with_level, record_bytes_downloaded, record_bytes_uploaded,
    record_fast_path_duration, record_hard_ban, record_page_processed, record_request_failure,
    record_request_success, record_slow_path_duration, record_soft_ban, set_active_crawlers,
    set_domain_ban_rate, set_global_error_rate, set_global_success_rate, set_memory_usage_percent,
    set_queue_depth, set_throughput_mbps, start_metrics_server, start_metrics_server_default,
    MetricsServerHandle, StatsCollector,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // * Verify all major types are accessible
        let _manager = AlertManager::new();
        let _collector = StatsCollector::new();
        let _alert = sev3_alert("Test");
    }

    #[test]
    fn test_telemetry_metrics() {
        set_active_crawlers(5);
        set_throughput_mbps(10.0);
        set_global_error_rate(0.05);
        set_global_success_rate(0.95);

        let metrics = get_metrics_string();
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_alerting_integration() {
        let manager = AlertManager::new();

        for _ in 0..10 {
            manager.record_success();
        }
        manager.record_failure();

        let stats = manager.get_stats();
        assert!(stats.success_rate > 0.5);
    }
}
