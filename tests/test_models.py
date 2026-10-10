"""Unit tests for news item data models."""

from datetime import datetime, timezone
import pytest
from pydantic import ValidationError

from daily_digest.models import NewsItem


def test_news_item_creation():
    """Verify that NewsItem instantiates correctly with mandatory and optional fields."""
    now = datetime.now(timezone.utc)
    item = NewsItem(
        title="New LLM Released",
        url="https://example.com/item1",
        published_at=now,
        source="Test AI Blog",
        raw_content="<p>Detailed announcement text</p>",
        clean_snippet="Detailed announcement text",
        summary="Key bullet points",
        priority_score=100,
        priority_group="Major Releases",
    )

    assert item.title == "New LLM Released"
    assert item.url == "https://example.com/item1"
    assert item.published_at == now
    assert item.source == "Test AI Blog"
    assert item.raw_content == "<p>Detailed announcement text</p>"
    assert item.clean_snippet == "Detailed announcement text"
    assert item.summary == "Key bullet points"
    assert item.priority_score == 100
    assert item.priority_group == "Major Releases"


def test_news_item_defaults():
    """Verify default values for optional fields in NewsItem."""
    now = datetime.now(timezone.utc)
    item = NewsItem(
        title="Quick Update",
        url="https://example.com/quick",
        published_at=now,
        source="Feed",
    )

    assert item.raw_content is None
    assert item.clean_snippet is None
    assert item.summary is None
    assert item.priority_score == 10
    assert item.priority_group is None


def test_news_item_validation_error():
    """Verify that missing mandatory fields raise ValidationError."""
    with pytest.raises(ValidationError):
        NewsItem(title="Missing URL", source="Test")  # type: ignore
