"""Unit tests for Gemini AI Web Search fallback and multi-model cascade."""

from datetime import datetime, timezone
from unittest.mock import MagicMock, patch

from daily_digest.config.models import AppConfig, GeminiConfig, MistralConfig
from daily_digest.collectors.factory import create_collectors
from daily_digest.config.loader import load_config
from daily_digest.models import NewsItem
from daily_digest.summarization.gemini_search import GeminiSearchFallback
from daily_digest.summarization.search_fallback import MistralSearchFallback


def test_gemini_config_defaults():
    """Verify GeminiConfig default settings and fallback models."""
    cfg = GeminiConfig()
    assert cfg.enabled is True
    assert cfg.api_key_env == "GEMINI_API_KEY"
    assert cfg.models == ["gemini-2.5-flash", "gemini-2.5-flash-lite"]
    assert cfg.temperature == 0.2
    assert cfg.search_fallback_pacing_seconds == 1.0


def test_gemini_search_disabled_or_no_key():
    """Verify GeminiSearchFallback is unavailable when disabled or missing API key."""
    cfg_disabled = GeminiConfig(enabled=False, api_key="fake-key")
    fallback_disabled = GeminiSearchFallback(config=cfg_disabled)
    assert not fallback_disabled.is_available
    assert fallback_disabled.search_source_fallback("DeepMind") == []
    assert fallback_disabled.search_twitter_fallback("karpathy", "Twitter") == []

    cfg_no_key = GeminiConfig(enabled=True, api_key=None)
    fallback_no_key = GeminiSearchFallback(config=cfg_no_key)
    assert not fallback_no_key.is_available
    assert fallback_no_key.search_source_fallback("DeepMind") == []


def test_gemini_search_source_fallback_success():
    """Verify Gemini search parses JSON announcement array correctly."""
    cfg = GeminiConfig(
        enabled=True,
        api_key="test-key",
        search_fallback_pacing_seconds=0.0,
        models=["gemini-2.5-flash", "gemini-2.5-flash-lite"],
    )
    fallback = GeminiSearchFallback(config=cfg)

    mock_client = MagicMock()
    mock_resp = MagicMock()
    mock_resp.text = """```json
[
  {
    "title": "Google DeepMind Unveils Gemini 2.5",
    "url": "https://deepmind.google/discover/blog/gemini-2-5",
    "summary": "DeepMind announced Gemini 2.5 Flash and Flash Lite with advanced grounding."
  }
]
```"""
    mock_client.models.generate_content.return_value = mock_resp
    fallback._client = mock_client

    items = fallback.search_source_fallback(
        source_name="Google DeepMind",
        source_url="https://deepmind.google/blog/rss.xml",
        hours=24,
    )

    assert len(items) == 1
    assert items[0].title == "Google DeepMind Unveils Gemini 2.5"
    assert items[0].url == "https://deepmind.google/discover/blog/gemini-2-5"
    assert "Gemini 2.5 Flash" in items[0].clean_snippet
    assert items[0].summary is None  # Gemini never creates AI summaries
    assert "Google DeepMind (Web Search)" in items[0].source


def test_gemini_search_twitter_fallback_profile_url_enforced():
    """Verify Twitter fallback enforces profile URL and preserves external links."""
    cfg = GeminiConfig(
        enabled=True,
        api_key="test-key",
        search_fallback_pacing_seconds=0.0,
    )
    fallback = GeminiSearchFallback(config=cfg)

    mock_client = MagicMock()
    mock_resp = MagicMock()
    mock_resp.text = """```json
[
  {
    "title": "@demishassabis on Gemini 2.5 Benchmark",
    "url": "https://x.com/demishassabis/status/987654321",
    "content": "Demis shares benchmark details: check report at https://arxiv.org/abs/2501.12345"
  }
]
```"""
    mock_client.models.generate_content.return_value = mock_resp
    fallback._client = mock_client

    items = fallback.search_twitter_fallback(
        username="demishassabis",
        source_label="X (Twitter) Key AI Researchers (@demishassabis)",
        hours=24,
    )

    assert len(items) == 1
    assert items[0].title == "@demishassabis on Gemini 2.5 Benchmark"
    assert items[0].url == "https://x.com/demishassabis"
    assert "https://arxiv.org/abs/2501.12345" in items[0].clean_snippet
    assert items[0].summary is None
    assert "(@demishassabis) (Web Search)" in items[0].source



def test_gemini_model_demotion_to_flash_lite():
    """Verify Gemini demotes from gemini-2.5-flash to gemini-2.5-flash-lite on failure."""
    cfg = GeminiConfig(
        enabled=True,
        api_key="test-key",
        search_fallback_pacing_seconds=0.0,
        models=["gemini-2.5-flash", "gemini-2.5-flash-lite"],
        max_retries=1,
        backoff_factor=1.0,
    )
    fallback = GeminiSearchFallback(config=cfg)

    mock_client = MagicMock()
    # Flash fails, flash-lite succeeds
    mock_resp_lite = MagicMock()
    mock_resp_lite.text = """[{"title": "Lite Result", "url": "https://example.com/lite", "summary": "Success"}]"""

    def mock_generate(model, contents, config):
        if model == "gemini-2.5-flash":
            raise Exception("429 RESOURCE_EXHAUSTED quota exceeded")
        return mock_resp_lite

    mock_client.models.generate_content.side_effect = mock_generate
    fallback._client = mock_client

    with patch("time.sleep"):
        items = fallback.search_source_fallback("OpenAI Blog")

    assert len(items) == 1
    assert items[0].title == "Lite Result"
    assert fallback.active_model == "gemini-2.5-flash-lite"


def test_gemini_circuit_breaker():
    """Verify circuit breaker opens after repeated failures across all models."""
    cfg = GeminiConfig(
        enabled=True,
        api_key="test-key",
        search_fallback_pacing_seconds=0.0,
        models=["gemini-2.5-flash", "gemini-2.5-flash-lite"],
        max_retries=0,
        circuit_breaker_threshold=2,
    )
    fallback = GeminiSearchFallback(config=cfg)

    mock_client = MagicMock()
    mock_client.models.generate_content.side_effect = Exception("500 Internal Error")
    fallback._client = mock_client

    # Call 1 fails
    assert fallback.search_source_fallback("Source A") == []
    assert fallback._consecutive_failures == 1
    assert not fallback._circuit_open

    # Call 2 fails and opens circuit breaker
    assert fallback.search_source_fallback("Source B") == []
    assert fallback._consecutive_failures == 2
    assert fallback._circuit_open

    # Subsequent call immediately rejected
    assert fallback.search_source_fallback("Source C") == []


def test_mistral_search_fallback_cascades_to_gemini():
    """Verify MistralSearchFallback cascades to Gemini when Mistral fails or is unconfigured."""
    mistral_cfg = MistralConfig(enabled=True, web_search_fallback=True, api_key="mistral-key")
    mock_gemini = MagicMock()
    mock_gemini.is_available = True
    now = datetime.now(timezone.utc)
    mock_item = NewsItem(
        title="Gemini Fallback Article",
        url="https://example.com/gemini-fallback",
        published_at=now,
        source="Anthropic Research (Web Search)",
    )
    mock_gemini.search_source_fallback.return_value = [mock_item]

    fallback = MistralSearchFallback(config=mistral_cfg, gemini_fallback=mock_gemini)
    # Simulate Mistral call failing
    mock_mistral_client = MagicMock()
    mock_mistral_client.beta.conversations.start.side_effect = Exception("Mistral quota exhausted")
    fallback._client = mock_mistral_client

    with patch("time.sleep"):
        results = fallback.search_source_fallback(
            source_name="Anthropic Research",
            source_url="https://anthropic.com/feed.xml",
            hours=24,
        )

    assert results == [mock_item]
    mock_gemini.search_source_fallback.assert_called_once_with(
        source_name="Anthropic Research",
        source_url="https://anthropic.com/feed.xml",
        hours=24,
    )


def test_mistral_search_fallback_without_mistral_key_uses_gemini_directly():
    """Verify that if Mistral key is None, Gemini fallback is used directly."""
    mistral_cfg = MistralConfig(enabled=True, web_search_fallback=True, api_key=None)
    mock_gemini = MagicMock()
    mock_gemini.is_available = True
    now = datetime.now(timezone.utc)
    mock_item = NewsItem(
        title="Direct Gemini Result",
        url="https://example.com/direct",
        published_at=now,
        source="Google DeepMind (Web Search)",
    )
    mock_gemini.search_source_fallback.return_value = [mock_item]

    fallback = MistralSearchFallback(config=mistral_cfg, gemini_fallback=mock_gemini)
    assert fallback.is_available is True

    results = fallback.search_source_fallback("Google DeepMind")
    assert results == [mock_item]
    mock_gemini.search_source_fallback.assert_called_once()


def test_factory_assigns_gemini_search_fallback_directly():
    """Verify create_collectors assigns GeminiSearchFallback directly when Gemini key is configured."""
    app_cfg = load_config("config/config.yaml")
    app_cfg.mistral.api_key = "fake-mistral"
    app_cfg.gemini.api_key = "fake-gemini"

    collectors = create_collectors(app_cfg)
    rss_collectors = [c for c in collectors if getattr(c, "search_fallback", None) is not None]
    assert len(rss_collectors) > 0

    first_collector = rss_collectors[0]
    fallback = first_collector.search_fallback
    assert isinstance(fallback, GeminiSearchFallback)
    assert fallback.is_available is True


def test_factory_falls_back_to_mistral_when_gemini_not_available():
    """Verify create_collectors falls back to Mistral search if Gemini is not configured."""
    app_cfg = load_config("config/config.yaml")
    app_cfg.gemini.api_key = None
    app_cfg.mistral.api_key = "fake-mistral"
    app_cfg.mistral.web_search_fallback = True

    collectors = create_collectors(app_cfg)
    rss_collectors = [c for c in collectors if getattr(c, "search_fallback", None) is not None]
    assert len(rss_collectors) > 0

    first_collector = rss_collectors[0]
    fallback = first_collector.search_fallback
    assert isinstance(fallback, MistralSearchFallback)

