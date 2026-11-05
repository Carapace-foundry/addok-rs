# ==============================================================================
# Builder stage (100% Pure Rust - no C/C++ dependencies)
# ==============================================================================
FROM rust:1-slim-bookworm AS builder

WORKDIR /build

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary for addok and strip debug symbols
RUN cargo build --release --bin addok && \
    strip target/release/addok

# ==============================================================================
# Runtime stage (minimal, unprivileged and secure)
# ==============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies (certificates and curl for healthcheck)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/* /var/lib/dpkg/status-old

# Create non-root system user and group (UID 1000)
RUN groupadd -g 1000 addok && \
    useradd -u 1000 -g addok -s /bin/false -m addok

# Copy stripped binary from builder
COPY --from=builder --chown=addok:addok /build/target/release/addok /usr/local/bin/addok

# Setup data directory owned by unprivileged user
WORKDIR /data
RUN chown addok:addok /data

USER addok

# Expose HTTP REST API port
EXPOSE 7878

# Dedicated HTTP healthcheck endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:7878/health || exit 1

ENTRYPOINT ["addok"]
CMD ["serve", "--index", "/data/index", "--host", "0.0.0.0", "--port", "7878"]
