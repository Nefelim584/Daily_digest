"""Pydantic models for Daily Digest HTTP API."""

from typing import Optional
from pydantic import BaseModel, Field


class HealthResponse(BaseModel):
    """Health check response schema."""

    status: str = Field(default="ok", description="Service status")
    service: str = Field(default="daily-digest", description="Service name")
    timestamp: str = Field(description="ISO timestamp")
    mistral_enabled: bool = Field(description="Whether Mistral AI is configured")


class GenerateRequest(BaseModel):
    """Request payload for asynchronous digest generation."""

    hours: Optional[int] = Field(default=None, ge=1, description="Collection window in hours")
    format: Optional[str] = Field(default=None, description="Output format ('markdown' or 'text')")
    output_dir: Optional[str] = Field(default=None, description="Output directory path")
    enable_summarization: Optional[bool] = Field(default=None, description="Toggle AI summarization")
    webhook_url: Optional[str] = Field(default=None, description="Callback URL to notify upon completion")


class GenerateResponse(BaseModel):
    """Accepted response for async generation."""

    task_id: str = Field(description="Unique task identifier")
    status: str = Field(default="accepted", description="Initial task status")
    message: str = Field(description="Informational status message")


class TaskResult(BaseModel):
    """Execution results for completed task."""

    total_collected: int
    total_unique: int
    time_window_hours: int
    output_path: str
    items_count: int


class TaskStatusResponse(BaseModel):
    """Status query response for background task."""

    task_id: str
    status: str = Field(description="Status: 'pending', 'running', 'completed', or 'failed'")
    created_at: str
    completed_at: Optional[str] = None
    error: Optional[str] = None
    result: Optional[TaskResult] = None
