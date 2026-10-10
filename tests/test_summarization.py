"""Unit tests for Mistral AI summarizer."""

from datetime import datetime, timezone
from unittest.mock import MagicMock
from mistralai.client import Mistral

from daily_digest.config.models import MistralConfig
from daily_digest.models import NewsItem
from daily_digest.summarization import MistralSummarizer


def test_summarizer_disabled():
    """Verify summarizer returns items untouched when disabled."""
    cfg = MistralConfig(enabled=False, api_key="secret-key")
    summarizer = MistralSummarizer(config=cfg)

    now = datetime.now(timezone.utc)
    item = NewsItem(title="Test", url="https://example.com", published_at=now, source="Test", clean_snippet="Snippet")
    result = summarizer.summarize_item(item)

    assert result.summary is None


def test_summarizer_missing_api_key_fallback():
    """Verify summarizer handles missing API key gracefully without errors."""
    cfg = MistralConfig(enabled=True, api_key=None)
    summarizer = MistralSummarizer(config=cfg)

    now = datetime.now(timezone.utc)
    item = NewsItem(title="Test", url="https://example.com", published_at=now, source="Test", clean_snippet="Snippet")
    result = summarizer.summarize_item(item)

    assert result.summary is None


def test_summarizer_successful_mock():
    """Verify summarizer parses chat completion response correctly."""
    cfg = MistralConfig(enabled=True, api_key="test-key")
    summarizer = MistralSummarizer(config=cfg)

    mock_client = MagicMock()
    mock_resp = MagicMock()
    mock_choice = MagicMock()
    mock_choice.message.content = "- Key contribution 1\n- Key contribution 2"
    mock_resp.choices = [mock_choice]
    mock_client.chat.complete.return_value = mock_resp

    summarizer._client = mock_client

    now = datetime.now(timezone.utc)
    item = NewsItem(
        title="Breakthrough Model",
        url="https://example.com/breakthrough",
        published_at=now,
        source="AI Blog",
        clean_snippet="Exciting model released.",
    )
    result = summarizer.summarize_item(item)

    assert result.summary == "- Key contribution 1\n- Key contribution 2"
    mock_client.chat.complete.assert_called_once()


def test_summarizer_api_exception_fallback():
    """Verify summarizer catches API exceptions and falls back without raising."""
    cfg = MistralConfig(enabled=True, api_key="test-key")
    summarizer = MistralSummarizer(config=cfg)

    mock_client = MagicMock()
    mock_client.chat.complete.side_effect = RuntimeError("Quota exceeded / 429 rate limit")
    summarizer._client = mock_client

    now = datetime.now(timezone.utc)
    item = NewsItem(
        title="Error Case",
        url="https://example.com/error",
        published_at=now,
        source="AI Blog",
        clean_snippet="Some snippet.",
    )
    result = summarizer.summarize_item(item)

    # Must not raise, item remains intact with summary=None
    assert result.summary is None
