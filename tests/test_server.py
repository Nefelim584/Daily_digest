"""Integration and unit tests for HTTP API server, healthcheck, and task worker."""

import asyncio
from pathlib import Path
from unittest.mock import AsyncMock, patch

import pytest
from fastapi.testclient import TestClient

from daily_digest.config.models import AppConfig, OutputConfig, ServerConfig
from daily_digest.pipeline.orchestrator import PipelineResult
from daily_digest.server.app import create_app
from daily_digest.server.models import GenerateRequest
from daily_digest.server.tasks import TaskManager, TaskRecord


@pytest.fixture
def test_config(tmp_path: Path) -> AppConfig:
    """Create test configuration with temporary output directory."""
    cfg = AppConfig()
    cfg.output = OutputConfig(directory=tmp_path / "digests")
    cfg.server = ServerConfig(host="0.0.0.0", port=8000)
    cfg.mistral.enabled = False
    return cfg


@pytest.fixture
def client(test_config: AppConfig) -> TestClient:
    """Create FastAPI test client with configured test state."""
    app = create_app(test_config)
    return TestClient(app)


def test_health_check_endpoint(client: TestClient):
    """Verify /health endpoint returns service status and metadata (Property 4)."""
    response = client.get("/health")
    assert response.status_code == 200
    data = response.json()
    assert data["status"] == "ok"
    assert data["service"] == "daily-digest"
    assert "timestamp" in data
    assert data["mistral_enabled"] is False


def test_get_nonexistent_task_returns_404(client: TestClient):
    """Verify unknown task_id query returns HTTP 404."""
    response = client.get("/tasks/non-existent-task-id")
    assert response.status_code == 404
    assert "not found" in response.json()["detail"].lower()


def test_generate_endpoint_accepted(client: TestClient):
    """Verify POST /generate returns 202 Accepted and creates a task (Property 2 - Option B)."""
    with patch("daily_digest.pipeline.orchestrator.DigestPipeline.run") as mock_run:
        mock_run.return_value = PipelineResult(
            total_collected=5,
            total_unique=4,
            time_window_hours=24,
            output_path=Path("/tmp/fake_digest.md"),
            items=[],
        )

        payload = {"hours": 24, "format": "markdown", "enable_summarization": False}
        response = client.post("/generate", json=payload)
        assert response.status_code == 202
        data = response.json()
        assert "task_id" in data
        assert data["status"] == "accepted"

        task_id = data["task_id"]

        # Verify task can be retrieved
        task_response = client.get(f"/tasks/{task_id}")
        assert task_response.status_code == 200
        task_data = task_response.json()
        assert task_data["task_id"] == task_id
        assert task_data["status"] == "completed"
        assert task_data["result"]["total_collected"] == 5
        assert task_data["result"]["total_unique"] == 4


def test_list_tasks_endpoint(client: TestClient):
    """Verify GET /tasks lists all registered generation tasks."""
    response = client.get("/tasks")
    assert response.status_code == 200
    assert isinstance(response.json(), list)


def test_webhook_dispatch_success(test_config: AppConfig):
    """Verify webhook dispatch sends POST callback upon task completion with X-Secret-Token header."""
    async def _run_test():
        manager = TaskManager(test_config)
        req = GenerateRequest(
            hours=12,
            webhook_url="http://test-bot:8080/callback",
            webhook_secret_token="secret_token_abc_123",
        )
        record = manager.create_task(req)
        record.status = "completed"

        with patch("httpx.AsyncClient.post", new_callable=AsyncMock) as mock_post:
            mock_response = AsyncMock()
            mock_response.status_code = 200
            mock_post.return_value = mock_response

            await manager._dispatch_webhook(record)

            assert mock_post.called
            target_url = mock_post.call_args[0][0]
            json_payload = mock_post.call_args[1]["json"]
            call_headers = mock_post.call_args[1]["headers"]
            assert target_url == "http://test-bot:8080/callback"
            assert json_payload["task_id"] == record.task_id
            assert json_payload["status"] == "completed"
            assert call_headers["X-Secret-Token"] == "secret_token_abc_123"

    asyncio.run(_run_test())



def test_server_network_binding_settings(test_config: AppConfig):
    """Verify server configuration binds to 0.0.0.0 (Property 1: Network openness)."""
    assert test_config.server.host == "0.0.0.0"
    assert test_config.server.port == 8000
