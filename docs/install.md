# Installation

Unlike the original Python Addok which requires installing Python 3, virtual environments, compilation dependencies (like C extensions), and setting up a separate Redis instance, **`addok-rs` is a single standalone native binary with zero runtime dependencies**.

---

## Prerequisites

To build `addok-rs` from source, you only need the **Rust toolchain** (version 1.75 or later).

If you don't have Rust installed:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Building from Source

Clone the repository and build the release binary:

```bash
git clone https://github.com/carapace-foundry/addok-rs.git
cd addok-rs
cargo build --release
```

The compiled standalone binary will be located at:
```bash
./target/release/addok
```

You can install it system-wide (e.g. to `/usr/local/bin`):
```bash
sudo cp target/release/addok /usr/local/bin/
```

Verify that the CLI is accessible:
```bash
addok --version
addok --help
```

---

## Auxiliary Binaries

Building with `cargo build --release` also compiles two high-performance benchmarking and utility binaries located in `target/release/`:

- **`benchmark`**: Automated performance test runner comparing latency percentiles (p50, p90, p99) and QPS.
- **`export_ndjson`**: High-speed BAN CSV to NDJSON converter.

---

## Docker Installation

You can run `addok-rs` inside a minimal Docker container (using a lightweight `debian:bookworm-slim` or `scratch`/`alpine` base):

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/addok /usr/local/bin/addok
EXPOSE 7878
ENTRYPOINT ["addok"]
CMD ["serve", "--index", "/data/index", "--host", "0.0.0.0", "--port", "7878"]
```

Build the container image:
```bash
docker build -t addok-rs:latest .
```

---

## Differences with Python Addok Installation

| Step | Python Addok | `addok-rs` |
|---|---|---|
| **Python Runtime** | Required (Python 3.9+) | **Not needed** |
| **Redis Server** | Required (`redis-server`, port 6379) | **Not needed (Zero daemons)** |
| **Plugin Packages** | `pip install addok-france addok-csv` | **Built directly into the binary** |
| **Deployment Artifact** | Virtualenv with multiple site-packages | **Single static/native binary** |
