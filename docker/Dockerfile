# ==============================================================================
# Stage 1: Build virtual environment with uv
# ==============================================================================
FROM ghcr.io/astral-sh/uv:latest AS uv_bin
FROM python:3.14-slim AS builder

COPY --from=uv_bin /uv /uvx /bin/

WORKDIR /app

ENV UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy

# Install dependencies using cached layers
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

# Copy project source and install application package
COPY src ./src
COPY README.md ./
RUN uv sync --frozen --no-dev

# ==============================================================================
# Stage 2: Production runtime image
# ==============================================================================
FROM python:3.14-slim AS runner

# Configurable UID and GID for shared volume permissions alignment (Property 2.1)
ARG USER_ID=1000
ARG GROUP_ID=1000

RUN groupadd -g ${GROUP_ID} appuser && \
    useradd -u ${USER_ID} -g appuser -s /bin/bash -m appuser

WORKDIR /app

# Copy virtualenv and application from builder
COPY --from=builder --chown=appuser:appuser /app/.venv /app/.venv
COPY --chown=appuser:appuser config /app/config
COPY --chown=appuser:appuser src /app/src

# Create default data directory for digests with appuser permissions
RUN mkdir -p /app/data/digests && chown -R appuser:appuser /app/data

ENV PATH="/app/.venv/bin:$PATH" \
    PYTHONUNBUFFERED=1 \
    HOST=0.0.0.0 \
    PORT=8000 \
    DIGEST_OUTPUT_DIR=/app/data/digests

USER appuser

# Healthcheck directive probing /health endpoint (Property 4)
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8000/health').read()" || exit 1

EXPOSE 8000

# Entrypoint supports both default --serve and on-demand CLI tasks (Option C)
ENTRYPOINT ["daily-digest"]
CMD ["--serve"]
