# ============================================================================
# Frontend Dockerfile — Multi-stage build for Scholarly Dioxus Web App
# ============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Build the Dioxus WASM application
# ---------------------------------------------------------------------------
FROM rust:1.88-bookworm AS builder

# Install dioxus-cli matching the dioxus 0.6 library used by the project.
# Pinning prevents silent breakage when a newer CLI with incompatible build
# behaviour (or a higher rustc requirement) is published to crates.io.
RUN cargo install dioxus-cli --version "^0.6" --locked

WORKDIR /build

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock* Dioxus.toml ./

# Copy source and assets
COPY src/ src/
COPY assets/ assets/

# Build the release bundle
RUN dx build --release

# ---------------------------------------------------------------------------
# Stage 2: Serve with Nginx
# ---------------------------------------------------------------------------
FROM nginx:alpine

# Remove the default nginx config
RUN rm /etc/nginx/conf.d/default.conf

# Copy custom nginx config
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Copy the built static files from the builder stage
# dioxus-cli 0.6 outputs to target/dx/<name>/release/web/public/
COPY --from=builder /build/target/dx/scholarly-frontend/release/web/public/ /usr/share/nginx/html/

# dioxus-cli does not copy [web.resource] stylesheets — copy manually
COPY assets/main.css /usr/share/nginx/html/assets/main.css

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
