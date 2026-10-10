"""Unit tests for time-window filtering logic."""

from datetime import datetime, timedelta, timezone

from daily_digest.filters import (
    filter_by_time_window,
    is_within_time_window,
    normalize_datetime,
)
from daily_digest.models import NewsItem


def test_normalize_datetime():
    """Verify normalize_datetime converts naive to UTC and transforms existing timezones."""
    naive = datetime(2026, 10, 10, 12, 0, 0)
    normalized = normalize_datetime(naive)
    assert normalized.tzinfo == timezone.utc

    offset_tz = timezone(timedelta(hours=2))
    aware = datetime(2026, 10, 10, 14, 0, 0, tzinfo=offset_tz)
    normalized_aware = normalize_datetime(aware)
    assert normalized_aware.tzinfo == timezone.utc
    assert normalized_aware.hour == 12


def test_is_within_time_window():
    """Verify is_within_time_window checks boundaries correctly."""
    now = datetime(2026, 10, 10, 12, 0, 0, tzinfo=timezone.utc)

    recent = now - timedelta(hours=2)
    border = now - timedelta(hours=24)
    old = now - timedelta(hours=25)

    assert is_within_time_window(recent, hours=24, now=now) is True
    assert is_within_time_window(border, hours=24, now=now) is True
    assert is_within_time_window(old, hours=24, now=now) is False


def test_filter_by_time_window_sorting():
    """Verify filter_by_time_window filters old items and sorts descending by published_at."""
    now = datetime(2026, 10, 10, 12, 0, 0, tzinfo=timezone.utc)

    item_1h = NewsItem(
        title="Item 1h",
        url="https://example.com/1",
        published_at=now - timedelta(hours=1),
        source="Test",
    )
    item_5h = NewsItem(
        title="Item 5h",
        url="https://example.com/5",
        published_at=now - timedelta(hours=5),
        source="Test",
    )
    item_30h = NewsItem(
        title="Item 30h",
        url="https://example.com/30",
        published_at=now - timedelta(hours=30),
        source="Test",
    )

    filtered = filter_by_time_window([item_5h, item_30h, item_1h], hours=24, now=now)

    assert len(filtered) == 2
    assert filtered[0].title == "Item 1h"
    assert filtered[1].title == "Item 5h"
