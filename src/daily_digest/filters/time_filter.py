"""Time-window filtering utilities for news items."""

from datetime import datetime, timedelta, timezone
from typing import Iterable, List, Optional
from ..models.news_item import NewsItem


def normalize_datetime(dt: datetime) -> datetime:
    """Ensure datetime is timezone-aware and set to UTC."""
    if dt.tzinfo is None:
        return dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def is_within_time_window(
    published_at: datetime,
    hours: int,
    now: Optional[datetime] = None,
) -> bool:
    """Check whether a publication timestamp falls within the last N hours."""
    current_time = normalize_datetime(now) if now is not None else datetime.now(timezone.utc)
    published_utc = normalize_datetime(published_at)

    cutoff = current_time - timedelta(hours=hours)
    return published_utc >= cutoff


def filter_by_time_window(
    items: Iterable[NewsItem],
    hours: int,
    now: Optional[datetime] = None,
    sort_newest_first: bool = True,
) -> List[NewsItem]:
    """Filter news items within the specified time window in hours.

    Args:
        items: An iterable of NewsItem instances.
        hours: Maximum age of items in hours.
        now: Optional reference datetime (defaults to current UTC time).
        sort_newest_first: If True, sort results by published_at descending.

    Returns:
        List of NewsItem instances published within the last N hours.
    """
    current_time = normalize_datetime(now) if now is not None else datetime.now(timezone.utc)

    filtered = [
        item for item in items
        if is_within_time_window(item.published_at, hours=hours, now=current_time)
    ]

    if sort_newest_first:
        filtered.sort(key=lambda item: normalize_datetime(item.published_at), reverse=True)

    return filtered
