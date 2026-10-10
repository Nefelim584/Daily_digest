"""Unit tests for source collectors and factory."""

from datetime import datetime, timezone
from unittest.mock import MagicMock
from pathlib import Path

from daily_digest.collectors import (
    ArxivCollector,
    HuggingFacePapersCollector,
    RSSCollector,
    RSSHubCollector,
    create_collector,
    create_collectors,
)
from daily_digest.config.loader import load_config
from daily_digest.config.models import AppConfig, SourceConfig
from daily_digest.network import HTTPFetcher


def test_arxiv_collector_mocked():
    """Verify ArxivCollector processes XML feed entries into NewsItem instances."""
    mock_fetcher = MagicMock(spec=HTTPFetcher)
    mock_xml = """<?xml version="1.0" encoding="UTF-8"?>
    <feed xmlns="http://www.w3.org/2005/Atom">
      <title>arXiv Search</title>
      <entry>
        <id>http://arxiv.org/abs/2610.12345v1</id>
        <title>Novel Reasoning Architecture in LLMs</title>
        <summary>This paper proposes a പുതിയ approach to reasoning.</summary>
        <published>2026-10-10T15:00:00Z</published>
        <link href="http://arxiv.org/abs/2610.12345v1" rel="alternate" type="text/html"/>
      </entry>
    </feed>
    """
    mock_fetcher.fetch_text.return_value = mock_xml

    collector = ArxivCollector(fetcher=mock_fetcher)
    now = datetime(2026, 10, 10, 18, 0, 0, tzinfo=timezone.utc)
    items = collector.collect(hours=24, now=now)

    assert len(items) == 1
    assert items[0].title == "Novel Reasoning Architecture in LLMs"
    assert items[0].url == "http://arxiv.org/abs/2610.12345v1"
    assert "reasoning" in items[0].clean_snippet.lower()


def test_huggingface_collector_mocked():
    """Verify HuggingFacePapersCollector processes API response into NewsItem instances."""
    mock_fetcher = MagicMock(spec=HTTPFetcher)
    mock_json = [
        {
            "paper": {
                "id": "2610.99999",
                "title": "Scaling Laws for Agents",
                "summary": "We study scaling behaviors in autonomous agents.",
                "publishedAt": "2026-10-10T10:00:00.000Z",
            }
        }
    ]
    mock_fetcher.fetch_json.return_value = mock_json

    collector = HuggingFacePapersCollector(fetcher=mock_fetcher)
    now = datetime(2026, 10, 10, 18, 0, 0, tzinfo=timezone.utc)
    items = collector.collect(hours=24, now=now)

    assert len(items) == 1
    assert items[0].title == "Scaling Laws for Agents"
    assert items[0].url == "https://huggingface.co/papers/2610.99999"
    assert "autonomous agents" in items[0].clean_snippet


def test_rss_collector_mocked():
    """Verify RSSCollector parses standard RSS 2.0 feed items."""
    mock_fetcher = MagicMock(spec=HTTPFetcher)
    mock_rss = """<?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0">
      <channel>
        <title>AI Lab Updates</title>
        <item>
          <title>Announcing Model V3</title>
          <link>https://example.com/blog/v3</link>
          <pubDate>Sat, 10 Oct 2026 14:00:00 GMT</pubDate>
          <description>&lt;p&gt;We are excited to share &lt;b&gt;Model V3&lt;/b&gt;.&lt;/p&gt;</description>
        </item>
      </channel>
    </rss>
    """
    mock_fetcher.fetch_text.return_value = mock_rss

    collector = RSSCollector(name="AI Lab", url="https://example.com/rss", fetcher=mock_fetcher)
    now = datetime(2026, 10, 10, 18, 0, 0, tzinfo=timezone.utc)
    items = collector.collect(hours=24, now=now)

    assert len(items) == 1
    assert items[0].title == "Announcing Model V3"
    assert items[0].url == "https://example.com/blog/v3"
    assert items[0].clean_snippet == "We are excited to share Model V3."


def test_rsshub_collector_mocked():
    """Verify RSSHubCollector processes route feeds and tags usernames."""
    mock_fetcher = MagicMock(spec=HTTPFetcher)
    mock_rss = """<?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0">
      <channel>
        <title>Twitter / researcher</title>
        <item>
          <title>Exciting research update on multimodal learning</title>
          <link>https://x.com/researcher/status/123</link>
          <pubDate>Sat, 10 Oct 2026 12:00:00 GMT</pubDate>
          <description>Full tweet body here</description>
        </item>
      </channel>
    </rss>
    """
    mock_fetcher.fetch_text.return_value = mock_rss

    collector = RSSHubCollector(
        name="Key AI Researchers",
        routes=["/twitter/user/researcher"],
        fetcher=mock_fetcher,
    )
    now = datetime(2026, 10, 10, 18, 0, 0, tzinfo=timezone.utc)
    items = collector.collect(hours=24, now=now)

    assert len(items) == 1
    assert "researcher" in items[0].source
    assert items[0].url == "https://x.com/researcher/status/123"


def test_collector_factory():
    """Verify factory instantiates proper collector types from configuration."""
    app_config = load_config(Path("config/config.yaml"))
    collectors = create_collectors(app_config)

    assert len(collectors) > 0
    collector_names = [c.name for c in collectors]
    assert "arXiv AI & Security Papers" in collector_names
    assert "OpenAI Blog" in collector_names

    disabled_source = SourceConfig(name="Disabled", type="rss", enabled=False, url="https://example.com")
    assert create_collector(disabled_source, app_config) is None
