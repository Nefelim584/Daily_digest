"""Unit tests for digest pipeline orchestrator."""

from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import MagicMock, patch

from daily_digest.config.models import AppConfig
from daily_digest.models import NewsItem
from daily_digest.pipeline import DigestPipeline, normalize_url


def test_normalize_url():
    """Verify URL normalization strips trailing slashes and lowers scheme/domain."""
    assert normalize_url("HTTPS://EXAMPLE.COM/path/") == "https://example.com/path"
    assert normalize_url("http://example.com/news") == "http://example.com/news"


def test_pipeline_deduplication_and_run(tmp_path: Path):
    """Verify pipeline deduplicates identical URLs and outputs a saved file."""
    now = datetime(2026, 10, 11, 10, 0, 0, tzinfo=timezone.utc)
    config = AppConfig()
    config.output.directory = tmp_path

    item1 = NewsItem(title="Title 1", url="https://example.com/item1", published_at=now, source="Source A")
    item2 = NewsItem(title="Title 1 duplicate", url="https://example.com/item1/", published_at=now, source="Source B")
    item3 = NewsItem(title="Title 2", url="https://example.com/item2", published_at=now, source="Source A")

    mock_collector = MagicMock()
    mock_collector.name = "Mock Collector"
    mock_collector.collect.return_value = [item1, item2, item3]

    pipeline = DigestPipeline(config)

    with patch("daily_digest.pipeline.orchestrator.create_collectors", return_value=[mock_collector]):
        result = pipeline.run(hours=24, now=now, enable_summarization=False)

        assert result.total_collected == 3
        assert result.total_unique == 2
        assert result.output_path.exists()
        assert len(result.items) == 2
