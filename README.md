# Daily Digest 🗞️

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Docker](https://img.shields.io/badge/docker-ready-blue.svg)](docker/docker-compose.yml)
[![Release](https://img.shields.io/badge/release-rust--v2.0.0-green.svg)](#releases--version-tags)

**Daily Digest** is an autonomous, high-performance tool designed for collecting, sanitizing, deduplicating, and summarizing fresh artificial intelligence and technology news and scientific publications.

The project is implemented in **pure Rust** as a **single, standalone, static binary** — completely eliminating external runtimes (Python, Node.js) and proprietary paid cloud LLM APIs (Mistral, Gemini, OpenAI).

---

## 🚀 Key Advantages & Architecture

- ⚡ **Exceptional Performance**: Sub-second pipeline execution with a peak RAM footprint under **32 MB** (an ~80% reduction compared to the Python baseline).
- 🔒 **100% Local & Free Processing (Zero API Keys)**:
  - **Extractive Summarization**: Local **TextRank** algorithm (sentence graph + stationary PageRank distribution) extracting the top 2–3 most salient sentences in **~20 ms** per run.
  - **Semantic Deduplication & Topic Clustering**: Local **TF-IDF** vectorization, cosine similarity deduplication, and spherical **k-means** clustering.
  - **Resilient Fallback Web Scraper**: Built-in HTML scraper (`reqwest` + `scraper`) automatically engaging when primary RSS feeds or X (Twitter) routes fail or return HTTP 404/429.
- 📊 **Multi-Format Output & Visualizations**: Exports digests into **Markdown (`.md`)**, **Plain Text (`.txt`)**, and interactive **HTML (`.html`)** containing an embedded, zero-dependency vector SVG cluster scatterplot (PCA/MDS projection).
- 🛡️ **Production-Grade Resilience**: Exponential backoff retries, HTTP 429 `Retry-After` adherence, circuit breaker protection, and atomic filesystem exports (`.tmp` ➔ `rename`).
- 🌐 **Built-in HTTP Microservice (Axum)**: Asynchronous REST API (`/health`, `/generate`, `/tasks/{id}`) supporting background worker queues and webhook callbacks with `X-Secret-Token` verification.

---

## 📈 Performance Benchmarks (Rust vs. Python)

Detailed A/B benchmarking between the original Python container implementation and the standalone Rust binary is documented in [**`docs/BENCHMARK.md`**](docs/BENCHMARK.md).

| Metric | Python 3.14 (uv) | Rust 1.99 (Release) | Improvement / Delta |
| :--- | :--- | :--- | :--- |
| **Peak RAM (Peak RSS, `--no-summary`)** | 154.9 MB | **30.3 MB** | 🏆 **Rust uses 5.1x less RAM (-80%)** |
| **Peak RAM (Peak RSS, full pipeline)** | 120.3 MB | **31.4 MB** | 🏆 **Rust uses 3.8x less RAM (-74%)** |
| **CPU Time (`user + sys`, `--no-summary`)** | 2.82 s | **0.73 s** | 🏆 **Rust uses 3.9x less CPU** |
| **CPU Time (`user + sys`, full pipeline)** | 1.71 s | **0.74 s** | 🏆 **Rust uses 2.3x less CPU** |
| **Recovered Articles** | 97 unique | **115 unique** | 🏆 **+19% more articles** (via scraper fallback) |
| **Summarization Latency** | Skipped (API quota required) | **0.02 s (20 ms)** | 🏆 **Instant, offline, zero token fees** |
| **Operational Cost per Run** | Cloud API fees | **$0.00** | 🏆 **100% cost-free operation** |

👉 Read the full test report in [docs/BENCHMARK.md](docs/BENCHMARK.md).

---

## 🏷️ Releases & Version Tags

The project maintains independent releases, allowing you to choose between the pure Rust binary and the containerized Python version:

| Release Tag | Description | Target Environment |
| :--- | :--- | :--- |
| [**`rust-v2.0.0`**](https://github.com/Nefelim584/Daily_Digest/releases) *(Current)* | Pure Rust standalone binary, local TextRank + TF-IDF, Axum server | Single binary, static MUSL, micro-Docker (~35 MB) |
| [**`python-v1.0.0`**](https://github.com/Nefelim584/Daily_Digest/releases/tag/python-v1.0.0) *(Legacy)* | Original Python 3.14 + uv implementation using Mistral/Gemini APIs | Docker container, preserved on branch [`python`](https://github.com/Nefelim584/Daily_Digest/tree/python) |

Each release can be downloaded independently from the **GitHub Releases** page without pulling unnecessary assets.

---

## 📐 Pipeline Architecture

```
[arXiv API / Hugging Face Papers / AI Research Blogs / X (Twitter) Mirrors]
                                │
                                ▼
  1. Resilient Network Collector (reqwest, backoff, HTTP 429 Retry-After, circuit breaker)
     └─► Fallback Web Scraper (automatically activated upon RSS / mirror failure)
                                │
                                ▼
  2. Sanitization & Text Normalization (strip HTML, inline scripts, styles, length cap)
                                │
                                ▼
  3. Time-Window Filtering (filter publications older than N hours)
                                │
                                ▼
  4. Exact Deduplication (canonical URL & title normalization)
                                │
                                ▼
  5. Priority Classification (regex tiers: Major Releases, Research, General News)
                                │
                                ▼
  6. Local TextRank Summarizer (graph construction, PageRank centrality extraction)
                                │
                                ▼
  7. TF-IDF Semantic Deduplication & Topic Clustering (cosine threshold + k-means + PCA)
                                │
                                ▼
  8. Sorting & Ordering (by priority rank and publication timestamp)
                                │
                                ▼
  9. Formatting & Atomic Export (Markdown / Plain Text / HTML with SVG cluster map)
```

---

## 🛠️ Quickstart & Installation

### Prerequisites
To build from source, install the [Rust toolchain](https://rustup.rs/) (v1.80+):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build Optimized Release Binary
```bash
# Clone the repository
git clone https://github.com/Nefelim584/Daily_Digest.git
cd Daily_Digest

# Build with link-time optimization (LTO) and binary stripping:
cargo build --release

# The compiled executable is available at:
./target/release/daily-digest
```

### Static Linux Binary (MUSL)
To produce a 100% portable, statically linked binary for Linux distributions with zero runtime dependencies:
```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl

# Static binary:
# ./target/x86_64-unknown-linux-musl/release/daily-digest
```

---

## 💻 CLI Usage

```bash
# Basic run (collect last 24 hours in Markdown format):
./target/release/daily-digest

# Collect news over a custom time window (e.g. 72 hours / 3 days):
./target/release/daily-digest --hours 72

# Export to interactive HTML with embedded SVG cluster scatterplot:
./target/release/daily-digest --format html

# Export to plain text:
./target/release/daily-digest --format text

# Specify custom destination directory:
./target/release/daily-digest --output /var/data/digests

# Fast run without extractive summaries (cleaned snippets only):
./target/release/daily-digest --no-summary

# Verbose logging (tracing debug info):
./target/release/daily-digest -v

# Launch background HTTP REST API daemon:
./target/release/daily-digest --serve
```

---

## ⚙️ Configuration (`config/config.yaml`)

All pipeline behaviors and collector targets are configured via [`config/config.yaml`](config/config.yaml):

```yaml
server:
  host: "0.0.0.0"
  port: 8000

time_window:
  default_hours: 24

output:
  format: "markdown"              # "markdown", "text", or "html"
  directory: "data/digests"       # Export destination directory
  filename_pattern: "digest_{date}.{ext}"

cleaning:
  max_characters: 1500            # Word-boundary text truncation limit
  strip_html_tags: true
  strip_images: true
  strip_links: true
  strip_scripts: true

rsshub:
  base_url: "https://rsshub.app"
  timeout_seconds: 15

summarization:
  enabled: true
  max_sentences: 3                # TextRank salient sentences per item

clustering:
  enabled: true
  dedup_threshold: 0.80           # TF-IDF cosine similarity deduplication threshold
  num_clusters: 0                 # 0 = automated topic cluster selection

search:
  enabled: true                   # Resilient HTML fallback scraper
  pacing_seconds: 1.0

priorities:
  - name: "Major Releases & Breakthroughs"
    priority: 100
    regex: "(?i)(release|introducing|launch|breakthrough|gpt-\\d|claude-\\d|gemini|llama|mistral|deepseek|sota)"
  - name: "Research & Benchmarks"
    priority: 50
    regex: "(?i)(paper|benchmark|dataset|architecture|evaluation|reasoning|alignment|vulnerability|jailbreak)"
  - name: "General News & Updates"
    priority: 10
    regex: ".*"

sources:
  - name: "arXiv AI & Security Papers"
    type: "arxiv_api"
    enabled: true
    categories: ["cs.AI", "cs.CL", "cs.LG", "cs.CV", "stat.ML", "cs.CR"]
    max_results: 50
  - name: "Hugging Face Daily Papers"
    type: "huggingface"
    enabled: true
  - name: "OpenAI Research News"
    type: "rss"
    url: "https://openai.com/news/rss.xml"
    enabled: true
  # ... additional lab blogs and Twitter/RSSHub feeds
```

### Environment Variables (12-Factor App)
Any configuration parameter can be overridden using environment variables or a local `.env` file:
- `DIGEST_OUTPUT_DIR` / `BOT_DIGESTS_PATH` — Destination folder for generated digests.
- `DIGEST_OUTPUT_FORMAT` — Default export format (`markdown`, `text`, or `html`).
- `DIGEST_DEFAULT_HOURS` — Default lookback window in hours.
- `HOST` and `PORT` — Network interface and port for the Axum HTTP server.
- `RSSHUB_BASE_URL` — Base URL for the RSSHub proxy instance.

---

## 🌐 HTTP API & Microservice Integration

For headless deployments and integration with Telegram bots (such as `Utility_bot`), orchestrators, or cron triggers, Daily Digest includes an asynchronous Axum HTTP service.

### Endpoints

#### 1. Health & Readiness Probe
```http
GET /health
```
Response (`200 OK`):
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

#### 2. Trigger Async Digest Generation
```http
POST /generate
Content-Type: application/json

{
  "hours": 24,
  "format": "html",
  "webhook_url": "http://utility_bot:8080/callback",
  "webhook_secret_token": "secret-auth-token"
}
```
Response (`202 Accepted`):
```json
{
  "task_id": "a50c822e-131d-4007-8898-d14cf1b8dfd5",
  "status": "pending"
}
```

#### 3. Task Status Polling
```http
GET /tasks/{task_id}
```
Response (`200 OK`):
```json
{
  "task_id": "a50c822e-131d-4007-8898-d14cf1b8dfd5",
  "status": "completed",
  "created_at": "2026-10-16T12:00:00Z",
  "completed_at": "2026-10-16T12:00:02Z",
  "output_path": "data/digests/digest_2026-10-16_1200.html",
  "error": null
}
```

#### 4. Webhook Notification
Upon task completion, Daily Digest executes an HTTP `POST` to the supplied `webhook_url` with the header `X-Secret-Token: secret-auth-token` and payload:
```json
{
  "task_id": "a50c822e-131d-4007-8898-d14cf1b8dfd5",
  "status": "completed",
  "output_path": "data/digests/digest_2026-10-16_1200.html",
  "error": null
}
```

---

## 🐳 Docker Deployment

The project provides a multi-stage `Dockerfile` producing a secure, minimal container running under an unprivileged user (`appuser`, UID/GID 1000):

```bash
# Build the container image:
docker compose -f docker/docker-compose.yml build

# Run a one-off digest generation:
docker compose -f docker/docker-compose.yml run --rm daily-digest

# Run with scheduled execution via Ofelia scheduler:
docker compose -f docker/docker-compose.yml up -d scheduler
```

---

## 🧪 Testing & Verification

The Rust test suite covers unit logic (sanitizer, HTML cleaner, TextRank graph centrality, TF-IDF vector math, source collectors, regex classifier) and end-to-end integration tests for the Axum HTTP API:

```bash
# Run all unit and integration tests:
cargo test

# Run linter checks:
cargo clippy -- -D warnings
```

---

## 📄 License

This project is licensed under the **MIT License**.

```text
MIT License

Copyright (c) 2026 Daily Digest Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
