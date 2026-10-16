# ==============================================================================
# Stage 1: Build release binary
# ==============================================================================
FROM rust:bookworm AS builder

WORKDIR /app

# Pre-cache dependencies layer
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Build actual binary
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ==============================================================================
# Stage 2: Minimal production runtime image
# ==============================================================================
FROM debian:bookworm-slim AS runner

# Configurable UID and GID for shared volume permissions alignment
ARG USER_ID=1000
ARG GROUP_ID=1000

RUN groupadd -g ${GROUP_ID} appuser && \
    useradd -u ${USER_ID} -g appuser -s /bin/bash -m appuser && \
    apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl tzdata && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary and configuration
COPY --from=builder /app/target/release/daily-digest /usr/local/bin/daily-digest
COPY --chown=appuser:appuser config /app/config

# Create default data directory for digests with appuser permissions
RUN mkdir -p /app/data/digests && chown -R appuser:appuser /app/data

ENV HOST=0.0.0.0 \
    PORT=8000 \
    DIGEST_OUTPUT_DIR=/app/data/digests

USER appuser

# Healthcheck directive probing /health endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://127.0.0.1:8000/health || exit 1

EXPOSE 8000

# Entrypoint supports both default --serve and on-demand CLI tasks
ENTRYPOINT ["daily-digest"]
CMD ["--serve"]
