"""FastAPI application factory for Daily Digest server."""

from contextlib import asynccontextmanager
from typing import Optional
from fastapi import FastAPI

from ..config.loader import load_config
from ..config.models import AppConfig
from ..export import DigestExporter
from .routes import router
from .tasks import TaskManager


def create_app(config: Optional[AppConfig] = None) -> FastAPI:
    """Create and configure FastAPI application instance."""
    app_config = config or load_config()

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        # Startup: clean up orphan temporary files to prevent corrupted state
        exporter = DigestExporter(app_config.output, clean_orphans_on_init=True)
        exporter.cleanup_temporary_files()
        yield

    app = FastAPI(
        title="Daily Digest Service",
        description="Asynchronous news digest collector and summarization service for multi-container architectures.",
        version="0.1.0",
        lifespan=lifespan,
    )

    app.state.config = app_config
    app.state.task_manager = TaskManager(app_config)
    app.include_router(router)

    return app


# Default app instance for standard ASGI servers
app = create_app()
