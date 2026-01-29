# * [EDD-8] Titan-Flow Production Dockerfile
# * Multi-stage build for optimized container size

# =============================================================================
# Stage 1: Build Stage
# =============================================================================
FROM rust:1.75-bookworm AS builder

# * Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# * Create app directory
WORKDIR /app

# * Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# * Create dummy source to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/main.rs

# * Build dependencies only (cached layer)
RUN cargo build --release && rm -rf src

# * Copy actual source code
COPY src ./src
COPY tests ./tests

# * Build the application
RUN touch src/lib.rs src/bin/main.rs && \
    cargo build --release --bin main

# =============================================================================
# Stage 2: Runtime Stage
# =============================================================================
FROM debian:bookworm-slim AS runtime

# * Install runtime dependencies for Chromium headless
RUN apt-get update && apt-get install -y --no-install-recommends \
    # * Chromium browser
    chromium \
    # * Required libraries for Chromium
    libnss3 \
    libatk1.0-0 \
    libatk-bridge2.0-0 \
    libcups2 \
    libdrm2 \
    libxkbcommon0 \
    libxcomposite1 \
    libxdamage1 \
    libxfixes3 \
    libxrandr2 \
    libgbm1 \
    libasound2 \
    libpango-1.0-0 \
    libcairo2 \
    # * TLS support
    ca-certificates \
    # * Networking utilities
    curl \
    && rm -rf /var/lib/apt/lists/*

# * Create non-root user for security
RUN useradd -m -u 1000 -s /bin/bash titan

# * Set working directory
WORKDIR /app

# * Copy binary from builder
COPY --from=builder /app/target/release/main /app/titan-flow

# * Set proper permissions
RUN chown -R titan:titan /app

# * Switch to non-root user
USER titan

# * Expose metrics port
EXPOSE 9000

# * Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9000/health || exit 1

# * Set environment variables
ENV RUST_LOG=info
ENV CHROME_BIN=/usr/bin/chromium
ENV CHROME_FLAGS="--no-sandbox --disable-dev-shm-usage --disable-gpu --headless"

# * Run the application
ENTRYPOINT ["/app/titan-flow"]
