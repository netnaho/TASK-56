# ============================================================================
# Backend Dockerfile — Multi-stage build for Scholarly Rocket API
# ============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Build the Rust binary
# ---------------------------------------------------------------------------
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock* ./

# Create dummy lib.rs + main.rs to pre-build dependencies (Cargo.toml declares both)
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    touch src/lib.rs && \
    cargo build --release && \
    rm -rf src

# Copy the real source code and rebuild
COPY src/ src/
RUN touch src/lib.rs src/main.rs && cargo build --release

# ---------------------------------------------------------------------------
# Stage 2: Minimal runtime image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        default-mysql-client && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary
COPY --from=builder /build/target/release/scholarly-backend /app/scholarly-backend

# Copy migrations, seeds, and scripts
COPY migrations/ /app/migrations/
COPY seeds/ /app/seeds/
COPY scripts/ /app/scripts/

RUN chmod +x /app/scripts/entrypoint.sh

EXPOSE 8000

ENTRYPOINT ["/app/scripts/entrypoint.sh"]
