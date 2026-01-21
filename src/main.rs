mod config;
mod network;
mod engine;
mod refinery;

#[tokio::main]
async fn main() {
    // Initialize Telemetry [NFR-01]
    tracing_subscriber::fmt()
        .with_env_filter("titan_flow=debug,info")
        .with_target(false)
        .json()
        .init();

    tracing::info!("Titan-Flow Engineering Orchestrator Initialized [Phase 0 Complete]");
}
