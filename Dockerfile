# ==============================================================================
# Builder stage
# ==============================================================================
FROM rust:1-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary for addok
RUN cargo build --release --bin addok && \
    strip target/release/addok

# ==============================================================================
# Runtime stage (minimal and secure)
# ==============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies (certificates)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root system user and group
RUN groupadd -g 1000 addok && \
    useradd -u 1000 -g addok -s /bin/false -m addok

# Copy binary from builder
COPY --from=builder --chown=addok:addok /build/target/release/addok /usr/local/bin/addok

# Setup data directory
WORKDIR /data
RUN chown addok:addok /data

USER addok

# Expose HTTP REST API port
EXPOSE 7878

# Healthcheck
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:7878/search/?q=paris || exit 1

ENTRYPOINT ["addok"]
CMD ["serve", "--index", "/data/index", "--host", "0.0.0.0", "--port", "7878"]
