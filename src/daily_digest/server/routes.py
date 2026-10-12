"""API endpoints for Daily Digest server."""

from datetime import datetime, timezone
from typing import List
from fastapi import APIRouter, BackgroundTasks, HTTPException, Request, status

from .models import (
    GenerateRequest,
    GenerateResponse,
    HealthResponse,
    TaskStatusResponse,
)

router = APIRouter()


@router.get("/health", response_model=HealthResponse)
async def health_check(request: Request) -> HealthResponse:
    """Health check endpoint to probe service readiness (Property 4)."""
    app_config = request.app.state.config
    mistral_ready = bool(app_config.mistral.enabled and app_config.mistral.api_key)
    return HealthResponse(
        status="ok",
        service="daily-digest",
        timestamp=datetime.now(timezone.utc).isoformat(),
        mistral_enabled=mistral_ready,
    )


@router.post("/generate", response_model=GenerateResponse, status_code=status.HTTP_202_ACCEPTED)
async def generate_digest(
    payload: GenerateRequest,
    background_tasks: BackgroundTasks,
    request: Request,
) -> GenerateResponse:
    """Non-blocking digest generation endpoint (Property 2 - Option B).

    Returns HTTP 202 Accepted with a unique task_id immediately.
    The digest generation executes in the background.
    """
    task_manager = request.app.state.task_manager
    task_record = task_manager.create_task(payload)
    background_tasks.add_task(task_manager.run_task, task_record)

    return GenerateResponse(
        task_id=task_record.task_id,
        status="accepted",
        message="Digest generation scheduled in background",
    )


@router.get("/tasks/{task_id}", response_model=TaskStatusResponse)
async def get_task_status(task_id: str, request: Request) -> TaskStatusResponse:
    """Poll background task status by task_id."""
    task_manager = request.app.state.task_manager
    task_record = task_manager.get_task(task_id)
    if not task_record:
        raise HTTPException(status_code=404, detail=f"Task '{task_id}' not found")
    return task_record.to_response()


@router.get("/tasks", response_model=List[TaskStatusResponse])
async def list_tasks(request: Request) -> List[TaskStatusResponse]:
    """List all recent generation tasks."""
    task_manager = request.app.state.task_manager
    return [record.to_response() for record in task_manager.list_tasks().values()]
