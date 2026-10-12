"""Background task manager and webhook dispatcher."""

import asyncio
import logging
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Optional

import httpx

from ..config.models import AppConfig
from ..pipeline import DigestPipeline
from .models import GenerateRequest, TaskResult, TaskStatusResponse

logger = logging.getLogger(__name__)


class TaskRecord:
    """In-memory record tracking background task execution."""

    def __init__(self, task_id: str, request: GenerateRequest) -> None:
        self.task_id = task_id
        self.request = request
        self.status: str = "pending"  # pending, running, completed, failed
        self.created_at: datetime = datetime.now(timezone.utc)
        self.completed_at: Optional[datetime] = None
        self.error: Optional[str] = None
        self.result: Optional[TaskResult] = None

    def to_response(self) -> TaskStatusResponse:
        """Convert record to response model."""
        return TaskStatusResponse(
            task_id=self.task_id,
            status=self.status,
            created_at=self.created_at.isoformat(),
            completed_at=self.completed_at.isoformat() if self.completed_at else None,
            error=self.error,
            result=self.result,
        )


class TaskManager:
    """Coordinates background digest generation tasks and webhook dispatching."""

    def __init__(self, config: AppConfig) -> None:
        self.config = config
        self._tasks: Dict[str, TaskRecord] = {}

    def create_task(self, request: GenerateRequest) -> TaskRecord:
        """Create and register a new pending task record."""
        task_id = str(uuid.uuid4())
        record = TaskRecord(task_id, request)
        self._tasks[task_id] = record
        return record

    def get_task(self, task_id: str) -> Optional[TaskRecord]:
        """Retrieve task by unique identifier."""
        return self._tasks.get(task_id)

    def list_tasks(self) -> Dict[str, TaskRecord]:
        """Retrieve all recorded tasks."""
        return self._tasks

    async def run_task(self, record: TaskRecord) -> None:
        """Run pipeline in thread pool and deliver optional webhook callback."""
        record.status = "running"
        logger.info("Starting background generation task %s", record.task_id)

        try:
            req = record.request
            pipeline = DigestPipeline(self.config)

            out_dir = Path(req.output_dir) if req.output_dir else None
            pipeline_result = await asyncio.to_thread(
                pipeline.run,
                hours=req.hours,
                output_format=req.format,
                output_dir=out_dir,
                enable_summarization=req.enable_summarization,
            )

            record.status = "completed"
            record.completed_at = datetime.now(timezone.utc)
            record.result = TaskResult(
                total_collected=pipeline_result.total_collected,
                total_unique=pipeline_result.total_unique,
                time_window_hours=pipeline_result.time_window_hours,
                output_path=str(pipeline_result.output_path.resolve()),
                items_count=len(pipeline_result.items),
            )
            logger.info("Task %s completed successfully: %s", record.task_id, record.result.output_path)

        except Exception as exc:
            record.status = "failed"
            record.completed_at = datetime.now(timezone.utc)
            record.error = str(exc)
            logger.exception("Task %s failed: %s", record.task_id, exc)

        # Webhook dispatch if requested
        if record.request.webhook_url:
            await self._dispatch_webhook(record)

    async def _dispatch_webhook(self, record: TaskRecord) -> None:
        """Deliver status update payload to requested webhook callback URL with optional security header."""
        webhook_url = record.request.webhook_url
        if not webhook_url:
            return

        payload = record.to_response().model_dump()
        headers = {}
        if record.request.webhook_secret_token:
            headers["X-Secret-Token"] = record.request.webhook_secret_token

        logger.info("Dispatching webhook for task %s to %s", record.task_id, webhook_url)
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                resp = await client.post(webhook_url, json=payload, headers=headers)
                logger.info("Webhook for task %s returned status %d", record.task_id, resp.status_code)
        except Exception as err:
            logger.warning("Failed to deliver webhook for task %s to %s: %s", record.task_id, webhook_url, err)

