#!/usr/bin/env python3
"""
Baseline benchmark script running identical workloads against the upstream Addok
reference implementation (Python + Redis) or addok-rs for parity verification.

Standard: Section 4.2 of repository standards.
Uses standard library only (no third-party dependencies required).
"""

import argparse
import concurrent.futures
import json
import statistics
import sys
import time
import urllib.parse
import urllib.request

PARIS_QUERIES = [
    "8 bd st michel",
    "10 avenue des champs elysees",
    "12 rue de la paix",
    "5 place de la concorde",
    "18 cours victor hugo",
    "4 faubourg saint antoine",
    "1 allee des lilas",
    "25 quai branly",
    "3 impasse du moulin",
    "14 rue de rivoli",
    "boulevard saint germain",
    "place de la republique",
    "rue de l eglise",
    "8 bis bd st michel",
    "10 av champs elysees",
    # Autocomplete prefixes
    "boulev",
    "avenue des cha",
    "rue de la p",
    "place de la con",
    # Typos / Fuzzy
    "8 boulevrad saint michel",
    "10 avneu champ elysee",
    "12 r paix",
    "5 palce concorde",
]

PARIS_COORDS = [
    (48.8512, 2.3431),
    (48.8698, 2.3082),
    (48.8688, 2.3308),
    (48.8656, 2.3212),
    (48.8530, 2.3499),
    (48.8584, 2.2945),
    (48.8606, 2.3376),
    (48.8566, 2.3522),
]

FRANCE_QUERIES = [
    "8 bd st michel paris",
    "10 av champs elysees paris",
    "15 rue de la republique lyon",
    "2 place bellecour lyon",
    "10 la canebiere marseille",
    "5 rue de rome marseille",
    "12 cours de l intendance bordeaux",
    "4 place de la bourse bordeaux",
    "20 rue faidherbe lille",
    "1 grand place lille",
    "8 rue crebillon nantes",
    "14 cours des 50 otages nantes",
    "3 place du capitole toulouse",
    "10 rue alsace lorraine toulouse",
    "5 place kleber strasbourg",
    "12 rue des grandes arcades strasbourg",
    "1 promenade des anglais nice",
    "8 avenue jean medecin nice",
    "la chevre aberbergement clemenciat",
    "route de la fontaine 01400",
    "impasse des epis 01400",
    "rue de la rep",
    "cours de l in",
    "place du cap",
    "10 canbiere marseille",
    "15 r republiq lyon",
    "12 cours intendanc bordaux",
]


def query_search(base_url: str, query: str, limit: int = 5) -> float:
    params = urllib.parse.urlencode({"q": query, "limit": limit})
    url = f"{base_url}/search/?{params}"
    t0 = time.perf_counter()
    req = urllib.request.Request(url, headers={"User-Agent": "addok-benchmark/1.0"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        _ = resp.read()
    return (time.perf_counter() - t0) * 1000.0


def query_reverse(base_url: str, lat: float, lon: float, limit: int = 5) -> float:
    params = urllib.parse.urlencode({"lat": lat, "lon": lon, "limit": limit})
    url = f"{base_url}/reverse/?{params}"
    t0 = time.perf_counter()
    req = urllib.request.Request(url, headers={"User-Agent": "addok-benchmark/1.0"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        _ = resp.read()
    return (time.perf_counter() - t0) * 1000.0


def run_benchmark(base_url: str, queries: list[str], coords: list[tuple[float, float]],
                  single_thread_count: int = 5000, multi_thread_count: int = 10000,
                  workers: int = 8):
    print("=======================================================")
    print(f" 🚀 RUNNING BASELINE BENCHMARK AGAINST: {base_url}")
    print("=======================================================")

    # Warm-up (200 requests)
    print("🔥 Warming up server (200 requests)...")
    for i in range(200):
        q = queries[i % len(queries)]
        try:
            query_search(base_url, q, limit=5)
        except Exception as e:
            print(f"❌ Error during warm-up: {e}")
            sys.exit(1)

    # 1. Single-threaded latency profile
    print(f"\n📊 Measuring single-thread latency profile ({single_thread_count} queries)...")
    latencies: list[float] = []
    t_start = time.perf_counter()
    for i in range(single_thread_count):
        q = queries[i % len(queries)]
        lat = query_search(base_url, q, limit=5)
        latencies.push(lat) if hasattr(latencies, 'push') else latencies.append(lat)
    t_total = time.perf_counter() - t_start

    latencies.sort()
    n = len(latencies)
    p50 = latencies[int(n * 0.50)]
    p90 = latencies[int(n * 0.90)]
    p95 = latencies[int(n * 0.95)]
    p99 = latencies[int(n * 0.99)]
    single_thread_qps = n / t_total

    print(f"   p50 (median):  {p50:.3f} ms")
    print(f"   p90:           {p90:.3f} ms")
    print(f"   p95:           {p95:.3f} ms")
    print(f"   p99:           {p99:.3f} ms")
    print(f"   Mean:          {statistics.mean(latencies):.3f} ms")
    print(f"   Min:           {latencies[0]:.3f} ms")
    print(f"   Max:           {latencies[-1]:.3f} ms")
    print(f"   Throughput 1T: {single_thread_qps:.1f} req/s")

    # 2. Multi-threaded throughput saturation
    print(f"\n🔥 Measuring multi-threaded throughput ({workers} workers, {multi_thread_count} queries)...")
    work_items = [queries[i % len(queries)] for i in range(multi_thread_count)]
    t_start = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
        futures = [executor.submit(query_search, base_url, q, 5) for q in work_items]
        concurrent.futures.wait(futures)
    t_multi = time.perf_counter() - t_start
    multi_thread_qps = multi_thread_count / t_multi
    print(f"   Multi-thread Throughput ({workers} workers): {multi_thread_qps:.1f} req/s")

    # 3. Reverse geocoding
    print(f"\n📍 Measuring reverse geocoding ({len(coords) * 200} queries)...")
    reverse_latencies: list[float] = []
    t_start = time.perf_counter()
    for _ in range(200):
        for lat, lon in coords:
            reverse_latencies.append(query_reverse(base_url, lat, lon, limit=5))
    t_rev = time.perf_counter() - t_start
    reverse_latencies.sort()
    rev_n = len(reverse_latencies)
    rev_p50 = reverse_latencies[int(rev_n * 0.50)]
    rev_p99 = reverse_latencies[int(rev_n * 0.99)]
    rev_qps = rev_n / t_rev

    print(f"   Reverse p50:   {rev_p50:.3f} ms")
    print(f"   Reverse p99:   {rev_p99:.3f} ms")
    print(f"   Reverse QPS:   {rev_qps:.1f} req/s")

    print("\n=======================================================")
    print(" 📋 SUMMARY METRICS TABLE")
    print("=======================================================")
    print(f"{'Metric':<30} | Value")
    print(f"{'-'*30}-+-------------------")
    print(f"{'p50 Latency (median)':<30} | {p50:.3f} ms")
    print(f"{'p90 Latency':<30} | {p90:.3f} ms")
    print(f"{'p95 Latency':<30} | {p95:.3f} ms")
    print(f"{'p99 Latency':<30} | {p99:.3f} ms")
    print(f"{'Single-thread Throughput':<30} | {single_thread_qps:.1f} req/s")
    print(f"{'Multi-thread Throughput':<30} | {multi_thread_qps:.1f} req/s")
    print(f"{'Reverse p50':<30} | {rev_p50:.3f} ms")
    print(f"{'Reverse Throughput':<30} | {rev_qps:.1f} req/s")
    print("=======================================================")


def main():
    parser = argparse.ArgumentParser(
        description="Run benchmark queries against an Addok server (Python or Rust)."
    )
    parser.add_argument("--url", default="http://127.0.0.1:7878", help="Base URL of Addok server")
    parser.add_argument("--dataset", choices=["paris", "france"], default="paris", help="Query dataset")
    parser.add_argument("--queries", type=int, default=5000, help="Number of queries for 1T test")
    parser.add_argument("--multi-queries", type=int, default=10000, help="Number of queries for MT test")
    parser.add_argument("--workers", type=int, default=8, help="Worker threads for MT test")
    args = parser.parse_args()

    queries = PARIS_QUERIES if args.dataset == "paris" else FRANCE_QUERIES
    run_benchmark(
        base_url=args.url.rstrip("/"),
        queries=queries,
        coords=PARIS_COORDS,
        single_thread_count=args.queries,
        multi_thread_count=args.multi_queries,
        workers=args.workers,
    )


if __name__ == "__main__":
    main()

