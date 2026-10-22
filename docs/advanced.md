# Advanced Usage & Production Deployment

This guide covers performance tuning, high-concurrency production deployments, and benchmarking for `addok-rs`.

---

## 🚀 Production Deployment with Systemd

Create a dedicated systemd service unit to manage the `addok` daemon:

`/etc/systemd/system/addok.service`:
```ini
[Unit]
Description=Addok-rs Geocoder Engine
After=network.target

[Service]
Type=simple
User=addok
Group=addok
WorkingDirectory=/var/lib/addok
ExecStart=/usr/local/bin/addok serve --index /var/lib/addok/index_france --host 127.0.0.1 --port 7878
Restart=always
RestartSec=3s
LimitNOFILE=65536

# Sandboxing & Security
ProtectSystem=strict
ProtectHome=true
ReadOnlyPaths=/var/lib/addok/index_france

[Install]
WantedBy=multi-user.target
```

Enable and start the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now addok
sudo systemctl status addok
```

---

## 🌐 Nginx Reverse Proxy Configuration

Put Nginx in front of `addok-rs` for TLS termination, rate limiting, and HTTP caching:

```nginx
server {
    listen 80;
    server_name api-adresse.example.com;

    location / {
        proxy_pass http://127.0.0.1:7878;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Keepalive connections
        proxy_http_version 1.1;
        proxy_set_header Connection "";

        # Enable gzip / brotli compression
        gzip on;
        gzip_types application/json application/geo+json text/csv;
    }
}
```

---

## ⚡ Performance Tuning

### 1. Concurrency Controls
- **Tokio Worker Threads**: By default, `addok-rs` matches the number of CPU cores available. You can restrict or increase workers via:
  ```bash
  export TOKIO_WORKER_THREADS=8
  ```
- **Rayon Parallelism**: For bulk indexing, parallel aggregation threads can be configured via:
  ```bash
  export RAYON_NUM_THREADS=12
  ```

### 2. Linux Kernel Page Cache
Because `addok-rs` utilizes `memmap2` for zero-copy index access, ensuring sufficient `max_map_count` is recommended for large national datasets:
```bash
sudo sysctl -w vm.max_map_count=262144
```

---

## 📊 Running the Benchmark Suite

`addok-rs` includes a dedicated benchmark executable:

```bash
cargo run --release --bin benchmark
```

It measures and outputs real-world statistics:
- Cold startup time (ms)
- Process RSS memory (MB)
- Text search latencies (p50, p90, p95, p99)
- Single-thread and multi-thread QPS
- Reverse geocoding latencies and QPS
