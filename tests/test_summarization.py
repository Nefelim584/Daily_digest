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


def test_summarizer_429_retry_success(monkeypatch):
    """Verify summarizer retries on 429 rate limit with backoff and succeeds on next attempt."""
    monkeypatch.setattr("time.sleep", lambda s: None)
    cfg = MistralConfig(enabled=True, api_key="test-key", max_retries=2, rate_limit_delay_seconds=0.0)
    summarizer = MistralSummarizer(config=cfg)

    mock_client = MagicMock()
    mock_success_resp = MagicMock()
    mock_choice = MagicMock()
    mock_choice.message.content = "- Summary after retry"
    mock_success_resp.choices = [mock_choice]

    # First call raises 429, second call succeeds
    mock_client.chat.complete.side_effect = [
        RuntimeError("Status 429: Rate limit exceeded"),
        mock_success_resp,
    ]
    summarizer._client = mock_client

    now = datetime.now(timezone.utc)
    item = NewsItem(title="Test 429", url="https://example.com/429", published_at=now, source="Src", clean_snippet="Text")
    result = summarizer.summarize_item(item)

    assert result.summary == "- Summary after retry"
    assert mock_client.chat.complete.call_count == 2
    assert summarizer._consecutive_failures == 0


def test_summarizer_circuit_breaker_triggers(monkeypatch):
    """Verify circuit breaker opens after repeated failures and skips subsequent API calls."""
    monkeypatch.setattr("time.sleep", lambda s: None)
    cfg = MistralConfig(
        enabled=True,
        api_key="test-key",
        max_retries=1,
        rate_limit_delay_seconds=0.0,
        circuit_breaker_threshold=2,
    )
    summarizer = MistralSummarizer(config=cfg)

    mock_client = MagicMock()
    mock_client.chat.complete.side_effect = RuntimeError("Status 429: Rate limit exceeded (code 1300)")
    summarizer._client = mock_client

    now = datetime.now(timezone.utc)
    items = [
        NewsItem(title="Item 1", url="https://example.com/1", published_at=now, source="Src", clean_snippet="Text 1"),
        NewsItem(title="Item 2", url="https://example.com/2", published_at=now, source="Src", clean_snippet="Text 2"),
        NewsItem(title="Item 3", url="https://example.com/3", published_at=now, source="Src", clean_snippet="Text 3"),
    ]

    results = summarizer.summarize_items(items)

    assert summarizer._circuit_open is True
    # Item 1 and Item 2 each tried 2 times (attempt 0 + 1 retry) = 4 calls.
    # Item 3 was skipped immediately by the circuit breaker without calling API!
    assert mock_client.chat.complete.call_count == 4
    for it in results:
        assert it.summary is None

