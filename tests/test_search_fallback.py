"""Unit tests for Mistral AI Web Search fallback mechanism."""

from datetime import datetime, timezone
from unittest.mock import MagicMock, patch
from daily_digest.config.models import MistralConfig
from daily_digest.collectors.rss import RSSCollector
from daily_digest.collectors.rsshub import RSSHubCollector
from daily_digest.models import NewsItem
from daily_digest.network import FetchError
from daily_digest.summarization import MistralSearchFallback


def test_search_fallback_disabled():
    """Verify fallback returns empty when disabled or unconfigured."""
    cfg = MistralConfig(enabled=False, web_search_fallback=True, api_key="test-key")
    fallback = MistralSearchFallback(config=cfg)
    assert not fallback.is_available
    assert fallback.search_source_fallback("Anthropic Research") == []
    assert fallback.search_twitter_fallback("OpenAI", "Twitter OpenAI") == []


def test_search_fallback_missing_key():
    """Verify fallback is unavailable when api_key is None."""
    cfg = MistralConfig(enabled=True, web_search_fallback=True, api_key=None)
    fallback = MistralSearchFallback(config=cfg)
    assert not fallback.is_available
    assert fallback.search_source_fallback("Anthropic Research") == []


def test_search_source_fallback_success():
    """Verify blog/article fallback parses title, url, and summary from JSON."""
    cfg = MistralConfig(enabled=True, web_search_fallback=True, api_key="test-key", search_fallback_pacing_seconds=0.0)
    fallback = MistralSearchFallback(config=cfg)

    mock_client = MagicMock()
    mock_resp = MagicMock()
    mock_output = MagicMock()
    mock_output.content = """```json
[
  {
    "title": "Anthropic Announces Claude 3.5 Sonnet Upgrades",
    "url": "https://www.anthropic.com/news/claude-3-5-sonnet",
    "summary": "Anthropic released an updated Claude 3.5 Sonnet with improved coding and reasoning capabilities."
  }
]
```"""
    mock_resp.outputs = [mock_output]
    mock_client.beta.conversations.start.return_value = mock_resp
    fallback._client = mock_client

    items = fallback.search_source_fallback(
        source_name="Anthropic Research",
        source_url="https://www.anthropic.com/feed.xml",
        hours=24,
    )

    assert len(items) == 1
    assert items[0].title == "Anthropic Announces Claude 3.5 Sonnet Upgrades"
    assert items[0].url == "https://www.anthropic.com/news/claude-3-5-sonnet"
    assert "Anthropic released an updated" in items[0].summary
    assert "Anthropic Research (Web Search)" in items[0].source


def test_search_twitter_fallback_profile_url_enforced():
    """Verify Twitter fallback enforces profile URL and preserves links in summary."""
    cfg = MistralConfig(enabled=True, web_search_fallback=True, api_key="test-key", search_fallback_pacing_seconds=0.0)
    fallback = MistralSearchFallback(config=cfg)

    mock_client = MagicMock()
    mock_resp = MagicMock()
    mock_output = MagicMock()
    mock_output.content = """```json
[
  {
    "title": "@karpathy on Micrograd v2",
    "url": "https://x.com/karpathy/status/123456789",
    "summary": "Karpathy shared insights on educational neural nets: check code at https://github.com/karpathy/micrograd"
  }
]
```"""
    mock_resp.outputs = [mock_output]
    mock_client.beta.conversations.start.return_value = mock_resp
    fallback._client = mock_client

    items = fallback.search_twitter_fallback(
        username="karpathy",
        source_label="X (Twitter) Key AI & Security Researchers (@karpathy)",
        hours=24,
    )

    assert len(items) == 1
    assert items[0].title == "@karpathy on Micrograd v2"
    # Must enforce profile URL and NOT status link
    assert items[0].url == "https://x.com/karpathy"
    # Preserves external URL inside summary
    assert "https://github.com/karpathy/micrograd" in items[0].summary
    assert "(@karpathy) (Web Search)" in items[0].source


def test_search_fallback_retry_and_circuit_breaker():
    """Verify 429 backoff retries and circuit breaker activation."""
    cfg = MistralConfig(
        enabled=True,
        web_search_fallback=True,
        api_key="test-key",
        search_fallback_pacing_seconds=0.0,
        max_retries=1,
        backoff_factor=1.0,
        circuit_breaker_threshold=2,
    )
    fallback = MistralSearchFallback(config=cfg)

    mock_client = MagicMock()
    mock_client.beta.conversations.start.side_effect = Exception("Status 429 Token rate limit reached")
    fallback._client = mock_client

    # First call fails after retries
    with patch("time.sleep"):
        res1 = fallback.search_source_fallback("Source 1")
    assert res1 == []
    assert fallback._consecutive_failures == 1
    assert not fallback._circuit_open

    # Second call fails and opens circuit breaker
    with patch("time.sleep"):
        res2 = fallback.search_source_fallback("Source 2")
    assert res2 == []
    assert fallback._consecutive_failures == 2
    assert fallback._circuit_open

    # Third call is blocked by circuit breaker
    assert fallback.search_source_fallback("Source 3") == []


def test_rss_collector_triggers_search_fallback_on_fetch_error():
    """Verify RSSCollector triggers search fallback when HTTP request fails."""
    mock_fallback = MagicMock()
    mock_fallback.is_available = True
    now = datetime.now(timezone.utc)
    mock_item = NewsItem(
        title="Test Announcement",
        url="https://example.com/item",
        published_at=now,
        source="Anthropic Research (Web Search)",
    )
    mock_fallback.search_source_fallback.return_value = [mock_item]

    mock_fetcher = MagicMock()
    mock_fetcher.fetch_text.side_effect = FetchError("404 Not Found")

    collector = RSSCollector(
        name="Anthropic Research",
        url="https://www.anthropic.com/feed.xml",
        fetcher=mock_fetcher,
        search_fallback=mock_fallback,
    )

    items = collector.collect(hours=24)
    assert items == [mock_item]
    mock_fallback.search_source_fallback.assert_called_once_with(
        source_name="Anthropic Research",
        source_url="https://www.anthropic.com/feed.xml",
        hours=24,
    )


def test_rsshub_collector_triggers_twitter_fallback_on_fetch_error():
    """Verify RSSHubCollector triggers Twitter search fallback for twitter routes."""
    mock_fallback = MagicMock()
    mock_fallback.is_available = True
    now = datetime.now(timezone.utc)
    mock_item = NewsItem(
        title="@OpenAI on Release",
        url="https://x.com/OpenAI",
        published_at=now,
        source="X (Twitter) AI Labs (@OpenAI) (Web Search)",
    )
    mock_fallback.search_twitter_fallback.return_value = [mock_item]

    mock_fetcher = MagicMock()
    mock_fetcher.fetch_text.side_effect = FetchError("404 Not Found")

    collector = RSSHubCollector(
        name="X (Twitter) AI Labs",
        routes=["/twitter/user/OpenAI"],
        fetcher=mock_fetcher,
        search_fallback=mock_fallback,
    )

    items = collector.collect(hours=24)
    assert len(items) == 1
    mock_fallback.search_twitter_fallback.assert_called_once_with(
        username="OpenAI",
        source_label="X (Twitter) AI Labs (@OpenAI)",
        hours=24,
    )
