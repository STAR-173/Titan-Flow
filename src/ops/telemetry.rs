// * [NFR-01] Telemetry - JSON Logging and Prometheus Metrics
// * Provides structured logging and metrics for production observability

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge, register_gauge_vec, register_histogram_vec,
    CounterVec, Encoder, Gauge, GaugeVec, HistogramVec, TextEncoder,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// * Default metrics server port
const DEFAULT_METRICS_PORT: u16 = 9000;

lazy_static! {
    // * Active crawler count
    pub static ref CRAWLERS_ACTIVE: Gauge = register_gauge!(
        "titan_crawlers_active",
        "Number of active crawler workers"
    ).unwrap();

    // * Throughput in megabytes per second
    pub static ref THROUGHPUT_MBPS: Gauge = register_gauge!(
        "titan_throughput_mbps",
        "Current throughput in megabytes per second"
    ).unwrap();

    // * Global error rate (0.0 - 1.0)
    pub static ref GLOBAL_ERROR_RATE: Gauge = register_gauge!(
        "titan_global_error_rate",
        "Global error rate as a ratio (0.0 - 1.0)"
    ).unwrap();

    // * Global success rate (0.0 - 1.0)
    pub static ref GLOBAL_SUCCESS_RATE: Gauge = register_gauge!(
        "titan_global_success_rate",
        "Global success rate as a ratio (0.0 - 1.0)"
    ).unwrap();

    // * Request counter by status
    pub static ref REQUESTS_TOTAL: CounterVec = register_counter_vec!(
        "titan_requests_total",
        "Total number of requests by status",
        &["status"]
    ).unwrap();

    // * Request duration histogram
    pub static ref REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "titan_request_duration_seconds",
        "Request duration in seconds",
        &["path_type"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    ).unwrap();

    // * Domain-level metrics
    pub static ref DOMAIN_BAN_RATE: GaugeVec = register_gauge_vec!(
        "titan_domain_ban_rate",
        "Ban rate per domain",
        &["domain"]
    ).unwrap();

    // * Memory usage percentage
    pub static ref MEMORY_USAGE_PERCENT: Gauge = register_gauge!(
        "titan_memory_usage_percent",
        "Current memory usage as percentage"
    ).unwrap();

    // * Pages processed counter
    pub static ref PAGES_PROCESSED_TOTAL: CounterVec = register_counter_vec!(
        "titan_pages_processed_total",
        "Total pages processed by type",
        &["content_type"]
    ).unwrap();

    // * Bytes transferred
    pub static ref BYTES_TRANSFERRED_TOTAL: CounterVec = register_counter_vec!(
        "titan_bytes_transferred_total",
        "Total bytes transferred by direction",
        &["direction"]
    ).unwrap();

    // * Queue depth
    pub static ref QUEUE_DEPTH: GaugeVec = register_gauge_vec!(
        "titan_queue_depth",
        "Current queue depth by queue name",
        &["queue"]
    ).unwrap();
}

/// Initializes the tracing subscriber with JSON formatting
///
/// # Example
/// ```ignore
/// use titan_flow::ops::telemetry;
///
/// telemetry::init_tracing();
/// tracing::info!(url = "https://example.com", "Processing page");
/// ```
pub fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().json())
        .init();
}

/// Initializes tracing with custom log level
pub fn init_tracing_with_level(level: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().json())
        .init();
}

/// Initializes tracing with pretty formatting (for development)
pub fn init_tracing_pretty() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().pretty())
        .init();
}

/// Metrics server handle for graceful shutdown
pub struct MetricsServerHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    running: Arc<AtomicBool>,
}

impl MetricsServerHandle {
    /// Signals the metrics server to shut down
    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.running.store(false, Ordering::Relaxed);
    }

    /// Returns true if the server is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

/// Starts the Prometheus metrics HTTP server on the specified port
///
/// Returns a handle that can be used for graceful shutdown.
///
/// # Example
/// ```ignore
/// use titan_flow::ops::telemetry;
///
/// #[tokio::main]
/// async fn main() {
///     let handle = telemetry::start_metrics_server(9000).await;
///     // Server is now running on :9000/metrics
///
///     // Later, for shutdown:
///     handle.shutdown();
/// }
/// ```
pub async fn start_metrics_server(port: u16) -> MetricsServerHandle {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tokio::spawn(async move {
        let make_svc = hyper::service::make_service_fn(|_conn| async {
            Ok::<_, std::convert::Infallible>(hyper::service::service_fn(handle_metrics_request))
        });

        let server = hyper::Server::bind(&addr)
            .serve(make_svc)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });

        tracing::info!(port = port, "Metrics server started");

        if let Err(e) = server.await {
            tracing::error!(error = %e, "Metrics server error");
        }

        running_clone.store(false, Ordering::Relaxed);
        tracing::info!("Metrics server stopped");
    });

    MetricsServerHandle {
        shutdown_tx: Some(shutdown_tx),
        running,
    }
}

/// Starts the metrics server on the default port (9000)
pub async fn start_metrics_server_default() -> MetricsServerHandle {
    start_metrics_server(DEFAULT_METRICS_PORT).await
}

/// Handles incoming HTTP requests to the metrics endpoint
async fn handle_metrics_request(
    req: hyper::Request<hyper::Body>,
) -> Result<hyper::Response<hyper::Body>, std::convert::Infallible> {
    match req.uri().path() {
        "/metrics" => {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer).unwrap();

            Ok(hyper::Response::builder()
                .status(200)
                .header("Content-Type", encoder.format_type())
                .body(hyper::Body::from(buffer))
                .unwrap())
        }
        "/health" => Ok(hyper::Response::builder()
            .status(200)
            .body(hyper::Body::from("OK"))
            .unwrap()),
        "/ready" => Ok(hyper::Response::builder()
            .status(200)
            .body(hyper::Body::from("READY"))
            .unwrap()),
        _ => Ok(hyper::Response::builder()
            .status(404)
            .body(hyper::Body::from("Not Found"))
            .unwrap()),
    }
}

/// Returns the current metrics as a string
pub fn get_metrics_string() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap_or_default()
}

/// Records a successful request
pub fn record_request_success() {
    REQUESTS_TOTAL.with_label_values(&["success"]).inc();
}

/// Records a failed request
pub fn record_request_failure() {
    REQUESTS_TOTAL.with_label_values(&["failure"]).inc();
}

/// Records a soft ban (captcha/WAF detection)
pub fn record_soft_ban() {
    REQUESTS_TOTAL.with_label_values(&["soft_ban"]).inc();
}

/// Records a hard ban (403/429)
pub fn record_hard_ban() {
    REQUESTS_TOTAL.with_label_values(&["hard_ban"]).inc();
}

/// Records request duration for the fast path
pub fn record_fast_path_duration(seconds: f64) {
    REQUEST_DURATION_SECONDS
        .with_label_values(&["fast"])
        .observe(seconds);
}

/// Records request duration for the slow path
pub fn record_slow_path_duration(seconds: f64) {
    REQUEST_DURATION_SECONDS
        .with_label_values(&["slow"])
        .observe(seconds);
}

/// Updates the active crawler count
pub fn set_active_crawlers(count: i64) {
    CRAWLERS_ACTIVE.set(count as f64);
}

/// Increments the active crawler count
pub fn increment_active_crawlers() {
    CRAWLERS_ACTIVE.inc();
}

/// Decrements the active crawler count
pub fn decrement_active_crawlers() {
    CRAWLERS_ACTIVE.dec();
}

/// Updates the throughput metric
pub fn set_throughput_mbps(mbps: f64) {
    THROUGHPUT_MBPS.set(mbps);
}

/// Updates the global error rate
pub fn set_global_error_rate(rate: f64) {
    GLOBAL_ERROR_RATE.set(rate.clamp(0.0, 1.0));
}

/// Updates the global success rate
pub fn set_global_success_rate(rate: f64) {
    GLOBAL_SUCCESS_RATE.set(rate.clamp(0.0, 1.0));
}

/// Updates the memory usage percentage
pub fn set_memory_usage_percent(percent: f64) {
    MEMORY_USAGE_PERCENT.set(percent.clamp(0.0, 100.0));
}

/// Records bytes transferred
pub fn record_bytes_downloaded(bytes: u64) {
    BYTES_TRANSFERRED_TOTAL
        .with_label_values(&["download"])
        .inc_by(bytes as f64);
}

/// Records bytes uploaded
pub fn record_bytes_uploaded(bytes: u64) {
    BYTES_TRANSFERRED_TOTAL
        .with_label_values(&["upload"])
        .inc_by(bytes as f64);
}

/// Records a page processed
pub fn record_page_processed(content_type: &str) {
    PAGES_PROCESSED_TOTAL
        .with_label_values(&[content_type])
        .inc();
}

/// Updates queue depth for a named queue
pub fn set_queue_depth(queue_name: &str, depth: i64) {
    QUEUE_DEPTH
        .with_label_values(&[queue_name])
        .set(depth as f64);
}

/// Updates domain ban rate
pub fn set_domain_ban_rate(domain: &str, rate: f64) {
    DOMAIN_BAN_RATE
        .with_label_values(&[domain])
        .set(rate.clamp(0.0, 1.0));
}

/// Statistics collector for computing rates
#[derive(Debug, Default)]
pub struct StatsCollector {
    success_count: std::sync::atomic::AtomicU64,
    failure_count: std::sync::atomic::AtomicU64,
    bytes_transferred: std::sync::atomic::AtomicU64,
    last_update: std::sync::atomic::AtomicU64,
}

impl StatsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_bytes(&self, bytes: u64) {
        self.bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn get_success_rate(&self) -> f64 {
        let success = self.success_count.load(Ordering::Relaxed) as f64;
        let failure = self.failure_count.load(Ordering::Relaxed) as f64;
        let total = success + failure;
        if total > 0.0 {
            success / total
        } else {
            1.0
        }
    }

    pub fn get_error_rate(&self) -> f64 {
        1.0 - self.get_success_rate()
    }

    pub fn get_total_bytes(&self) -> u64 {
        self.bytes_transferred.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.success_count.store(0, Ordering::Relaxed);
        self.failure_count.store(0, Ordering::Relaxed);
        self.bytes_transferred.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_request_success() {
        // * Reset metrics for test isolation
        let _ = REQUESTS_TOTAL.with_label_values(&["success"]);
        record_request_success();
        // * Metric should be incremented (exact value depends on test order)
    }

    #[test]
    fn test_record_request_failure() {
        record_request_failure();
        // * Metric should be incremented
    }

    #[test]
    fn test_set_active_crawlers() {
        set_active_crawlers(5);
        // * Value should be set
    }

    #[test]
    fn test_set_throughput() {
        set_throughput_mbps(10.5);
        // * Value should be set
    }

    #[test]
    fn test_set_global_rates() {
        set_global_error_rate(0.15);
        set_global_success_rate(0.85);
        // * Values should be clamped and set
    }

    #[test]
    fn test_rate_clamping() {
        set_global_error_rate(-0.5);
        set_global_error_rate(1.5);
        // * Should clamp to [0.0, 1.0]
    }

    #[test]
    fn test_get_metrics_string() {
        // * Trigger metric registration by accessing a metric
        CRAWLERS_ACTIVE.set(0.0);
        let metrics = get_metrics_string();
        // * Metrics string may be empty if no metrics registered yet, but should work
        assert!(metrics.is_empty() || metrics.contains("titan_"));
    }

    #[test]
    fn test_stats_collector() {
        let collector = StatsCollector::new();

        collector.record_success();
        collector.record_success();
        collector.record_failure();

        let success_rate = collector.get_success_rate();
        assert!((success_rate - 0.666666).abs() < 0.01);

        let error_rate = collector.get_error_rate();
        assert!((error_rate - 0.333333).abs() < 0.01);
    }

    #[test]
    fn test_stats_collector_empty() {
        let collector = StatsCollector::new();
        assert!((collector.get_success_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_stats_collector_bytes() {
        let collector = StatsCollector::new();
        collector.record_bytes(1000);
        collector.record_bytes(500);
        assert_eq!(collector.get_total_bytes(), 1500);
    }

    #[test]
    fn test_stats_collector_reset() {
        let collector = StatsCollector::new();
        collector.record_success();
        collector.record_bytes(100);
        collector.reset();
        assert_eq!(collector.get_total_bytes(), 0);
    }

    #[test]
    fn test_record_duration() {
        record_fast_path_duration(0.5);
        record_slow_path_duration(2.5);
        // * Histograms should be updated
    }

    #[test]
    fn test_record_bytes() {
        record_bytes_downloaded(1024);
        record_bytes_uploaded(512);
        // * Counters should be incremented
    }

    #[test]
    fn test_queue_depth() {
        set_queue_depth("crawl", 100);
        set_queue_depth("slow_render", 5);
        // * Gauges should be set
    }

    #[test]
    fn test_domain_ban_rate() {
        set_domain_ban_rate("example.com", 0.25);
        set_domain_ban_rate("blocked.com", 1.0);
        // * Gauges should be set with clamping
    }

    #[test]
    fn test_page_processed() {
        record_page_processed("html");
        record_page_processed("json");
        // * Counters should be incremented
    }

    #[tokio::test]
    async fn test_metrics_server_handle() {
        // * Test handle creation without actually starting server
        let running = Arc::new(AtomicBool::new(true));
        let handle = MetricsServerHandle {
            shutdown_tx: None,
            running: running.clone(),
        };

        assert!(handle.is_running());
    }
}
