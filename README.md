# 🏗️ Titan-Flow (Petabyte-Scale Data Refinery)

![Build Status](https://img.shields.io/badge/build-passing-brightgreen)
![Language](https://img.shields.io/badge/language-Rust_2021-orange)
![Architecture](https://img.shields.io/badge/architecture-Async_Tokio-blue)
![Status](https://img.shields.io/badge/status-Milestone_1_Complete-success)

**Titan-Flow** is a high-performance, distributed web crawler and data refinery designed to replace legacy Python systems. It achieves order-of-magnitude improvements by porting battle-tested heuristics from `crawl4ai` directly into a concurrent Rust architecture.

---

## 🚀 Engineering Pillars (EDD v3.0)

1.  **Memory-Adaptive Dispatching:** Real-time system monitoring to pause ingestion when RAM usage exceeds 90%.
2.  **Hybrid Routing:** Optimistic TLS-impersonated HTTP requests falling back to headless browsers only when DOM density metrics indicate dynamic content.
3.  **Intrinsic Traversal Scoring:** Priority queue system using `LinkIntrinsicScorer`.
4.  **Refined Extraction:** Heuristic-based table detection, regex-based entity extraction, and visual-based image filtering.
5.  **Smart Caching:** Head-based fingerprinting using `xxhash`.

---

## 🗺️ Engineering Roadmap & Status

### ✅ Milestone 1: The Network Kernel (COMPLETED)
The foundation of the system, capable of mimicking human TLS signatures and evading WAFs.
- [x] **[Task 1.1]** Repository Skeleton & Modular Architecture.
- [x] **[Task 1.2]** `IdentityProfile` (Chrome 120 TLS Fingerprinting).
- [x] **[Task 1.3]** Strict URL Normalization (Dupe Prevention).
- [x] **[Task 1.4]** Fast Client with Soft-Ban Detection (Captcha/WAF analysis).
- [x] **[Task 1.5]** Tiered Proxy Escalation Manager (Direct -> DC -> Res).

### 🚧 Milestone 2: Governance (CURRENT GOAL)
Ensuring the crawler plays nice with resources and target servers.
- [ ] **[Task 2.1]** Memory Adaptive Dispatcher (OOM Prevention).
- [ ] **[Task 2.2]** Global Rate Limiter & `robots.txt` Compliance.

### 🔮 Upcoming Milestones
- **Milestone 3: The Hybrid Engine** (Router, Chromium Headless, Circuit Breaker).
- **Milestone 4: The Refinery** (Table Extraction, Regex PII, Smart Chunking).
- **Milestone 5: Persistence & AI** (LanceDB Schema, Vector Embeddings, Dedupe).
- **Milestone 6: Operations** (Telemetry, Docker, Alerting).

---

## 🛠️ Setup & Installation

### Prerequisites
- **Rust:** Stable (1.8x+)
- **Build Tools:** Visual Studio C++ Build Tools (Windows) or `build-essential` (Linux).
- **Protoc:** Protocol Buffers Compiler (Required for LanceDB).

### Quick Start
```bash
# 1. Clone the repository
git clone https://github.com/your-org/titan-flow.git
cd titan-flow

# 2. Check the environment
cargo check

# 3. Run the Test Suite (Release mode required for BoringSSL integration on Windows)
cargo test --release
```

---

## 📂 Project Structure

```text
src/
├── config/       # Global constants and configuration
├── network/      # HTTP Clients, TLS Identity, Proxy Management
├── engine/       # Normalization, Dispatcher, Routing Logic
├── refinery/     # Data Extraction, Parsing, Chunking
└── main.rs       # Application Entry Point
```

---

## ⚠️ Disclaimer
This software is designed for **data engineering and research purposes**. Users are responsible for ensuring compliance with target website Terms of Service and `robots.txt` policies.
