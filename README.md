# Titan-Flow (Petabyte-Scale Data Refinery)

![Build Status](https://img.shields.io/badge/build-passing-brightgreen)
![Language](https://img.shields.io/badge/language-Rust_2021-orange)
![Architecture](https://img.shields.io/badge/architecture-Async_Tokio-blue)
![Tests](https://img.shields.io/badge/tests-184_passing-success)
![Status](https://img.shields.io/badge/status-All_Milestones_Complete-success)

**Titan-Flow** is a high-performance, distributed web crawler and data refinery designed to replace legacy Python systems. It achieves order-of-magnitude improvements by porting battle-tested heuristics from `crawl4ai` directly into a concurrent Rust architecture.

---

## Engineering Pillars (EDD v3.0)

1.  **Memory-Adaptive Dispatching:** Real-time system monitoring to pause ingestion when RAM usage exceeds 90%.
2.  **Hybrid Routing:** Optimistic TLS-impersonated HTTP requests (`reqwest`+`rustls`) falling back to headless browsers (`chromiumoxide`) only when DOM density metrics indicate dynamic content.
3.  **Intrinsic Traversal Scoring:** Priority queue system using `LinkIntrinsicScorer`.
4.  **Refined Extraction:** Heuristic-based table detection, regex-based entity extraction, and visual-based image filtering.
5.  **Smart Caching:** Head-based fingerprinting using `xxhash`.
6.  **Near-Duplicate Detection:** LSH MinHash with 20-band configuration for Jaccard similarity > 0.85.
7.  **Production Observability:** Prometheus metrics, structured JSON logging, and SEV-1/SEV-3 alerting.

---

## Engineering Roadmap & Status

### Milestone 1: The Network Kernel (COMPLETED)
The foundation of the system, capable of mimicking human TLS signatures and evading WAFs.
- [x] **[Task 1.1]** Repository Skeleton & Modular Architecture.
- [x] **[Task 1.2]** `IdentityProfile` (Chrome 120 Header Fingerprinting).
- [x] **[Task 1.3]** Strict URL Normalization (Dupe Prevention).
- [x] **[Task 1.4]** Fast Client with Soft-Ban Detection (Captcha/WAF analysis).
- [x] **[Task 1.5]** Tiered Proxy Escalation Manager (Direct -> DC -> Res).

### Milestone 2: Governance (COMPLETED)
Ensuring the crawler plays nice with resources and target servers.
- [x] **[Task 2.1]** Memory Adaptive Dispatcher (OOM Prevention via Hysteresis).
- [x] **[Task 2.2]** Global Rate Limiter & `robots.txt` Compliance (Redis-backed).

### Milestone 3: The Hybrid Engine (COMPLETED)
Intelligent routing between fast HTTP and slow Headless rendering.
- [x] **[Task 3.1]** Density Metric Calculator (Heuristic Routing).
- [x] **[Task 3.2]** Slow Path Renderer (Headless Chromium w/ Stealth).
- [x] **[Task 3.3]** Circuit Breaker (Failure Tracking & Handoff).

### Milestone 4: The Refinery (COMPLETED)
Turning raw HTML into structured, valuable data.
- [x] **[Task 4.1]** DOM Tree Shaking & Boilerplate Removal.
- [x] **[Task 4.2]** Heuristic Table Extraction.
- [x] **[Task 4.3]** Regex Entity Extraction (Email, URL, UUID, Dates, PII).
- [x] **[Task 4.4]** Smart Chunking (Unicode-aware Sliding Window).
- [x] **[Task 4.5]** Newsroom Metadata Extraction (JSON-LD, Meta Tags).

### Milestone 5: Persistence & AI (COMPLETED)
Storing data and enriching with AI capabilities.
- [x] **[Task 5.1]** LanceDB Schema (`MultimodalRecord` with 768-dim embeddings).
- [x] **[Task 5.2]** Deduplication (LSHBloom MinHash, Jaccard > 0.85).
- [x] **[Task 5.3]** Link Intrinsic Scorer (Crawl queue prioritization).
- [x] **[Task 5.4]** Async AI Enrichment Worker (Embeddings & Sentiment).

### Milestone 6: Operations & Deployment (COMPLETED)
Production-ready observability and containerization.
- [x] **[Task 6.1]** Telemetry (JSON logs, Prometheus metrics on `:9000/metrics`).
- [x] **[Task 6.2]** Alerting (SEV-1: Success Rate < 50%, SEV-3: Domain Ban > 90%).
- [x] **[Task 6.3]** Dockerfile (debian:bookworm-slim + Chromium headless).

---

## Setup & Installation

### Prerequisites
- **Rust:** Stable (1.75+)
- **Build Tools:** Visual Studio C++ Build Tools (Windows) or `build-essential` (Linux).
- **Protoc:** Protocol Buffers Compiler (Required for LanceDB).
- **Redis:** Required for Rate Limiting & Circuit Breaking state.
- **Chromium:** Required for headless rendering (auto-installed in Docker).

### Quick Start
```bash
# 1. Clone the repository
git clone https://github.com/your-org/titan-flow.git
cd titan-flow

# 2. Check the environment
cargo check

# 3. Run the Test Suite (184 tests)
cargo test --release
```

### Docker Deployment
```bash
# Build the container
docker build -t titan-flow:latest .

# Run with metrics exposed
docker run -p 9000:9000 titan-flow:latest
```

---

## Project Structure

```text
src/
├── config/           # Global constants and configuration
│   ├── mod.rs
│   └── constants.rs
├── network/          # HTTP Clients, Header Identity, Proxy Management
│   ├── mod.rs
│   ├── client.rs     # Fast path HTTP client
│   ├── errors.rs     # Network error types
│   ├── identity.rs   # Chrome 120 fingerprinting
│   └── proxy.rs      # Proxy escalation ladder
├── engine/           # Normalization, Dispatcher, Routing Logic
│   ├── mod.rs
│   ├── normalization.rs   # URL canonicalization
│   ├── dispatcher.rs      # Memory-adaptive dispatcher
│   ├── rate_limiter.rs    # Per-domain rate limiting
│   ├── fingerprint.rs     # Content fingerprinting
│   ├── density.rs         # DOM density metrics
│   ├── slow_path.rs       # Chromium headless renderer
│   └── circuit_breaker.rs # Failure tracking
├── refinery/         # Data Extraction Pipeline
│   ├── mod.rs             # Unified Refinery API
│   ├── content_cleaner.rs # Boilerplate removal
│   ├── tables.rs          # Table extraction
│   ├── regex_extractor.rs # Entity extraction
│   ├── chunker.rs         # Text chunking
│   └── metadata.rs        # JSON-LD/Meta extraction
├── persistence/      # Storage & Deduplication
│   ├── mod.rs
│   ├── schema.rs          # LanceDB MultimodalRecord
│   ├── dedup.rs           # LSH MinHash deduplication
│   ├── link_scorer.rs     # Link prioritization
│   └── ai_worker.rs       # Async AI enrichment
├── ops/              # Observability & Operations
│   ├── mod.rs
│   ├── telemetry.rs       # Prometheus metrics
│   └── alerting.rs        # SEV-1/SEV-3 alerts
├── bin/
│   └── main.rs       # Application Entry Point
└── lib.rs            # Library exports
```

---

## Metrics & Observability

Titan-Flow exposes Prometheus metrics on port 9000:

| Metric | Description |
|--------|-------------|
| `titan_crawlers_active` | Number of active crawler workers |
| `titan_throughput_mbps` | Current throughput in MB/s |
| `titan_global_success_rate` | Success rate (0.0 - 1.0) |
| `titan_global_error_rate` | Error rate (0.0 - 1.0) |
| `titan_requests_total` | Total requests by status |
| `titan_request_duration_seconds` | Request latency histogram |
| `titan_domain_ban_rate` | Per-domain ban rate |
| `titan_memory_usage_percent` | Memory usage percentage |

### Health Endpoints
- `GET /metrics` - Prometheus metrics
- `GET /health` - Health check
- `GET /ready` - Readiness check

---

## Key Configuration Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `PAGE_TIMEOUT_MS` | 60,000 | Page fetch timeout |
| `CHUNK_TOKEN_THRESHOLD` | 2,048 | Max words per chunk |
| `OVERLAP_RATE` | 0.1 | 10% chunk overlap |
| `EMBEDDING_DIM` | 768 | Embedding vector dimension |
| `JACCARD_THRESHOLD` | 0.85 | Near-duplicate threshold |
| `NUM_BANDS` | 20 | LSH band count |

---

## Disclaimer
This software is designed for **data engineering and research purposes**. Users are responsible for ensuring compliance with target website Terms of Service and `robots.txt` policies.
