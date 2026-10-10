"""Base collector interface and common utilities."""

from abc import ABC, abstractmethod
from datetime import datetime, timezone
from typing import Any, List, Optional
import time
from email.utils import parsedate_to_datetime

from ..config.models import CleaningConfig
from ..cleaning import sanitize_and_truncate
from ..filters import filter_by_time_window, normalize_datetime
from ..models import NewsItem
from ..network import HTTPFetcher


class BaseCollector(ABC):
    """Abstract base class for all source collectors."""

    def __init__(
        self,
        name: str,
        fetcher: Optional[HTTPFetcher] = None,
        cleaning_config: Optional[CleaningConfig] = None,
    ) -> None:
        """Initialize base collector."""
        self.name = name
        self.fetcher = fetcher or HTTPFetcher()
        self.cleaning_config = cleaning_config or CleaningConfig()

    @abstractmethod
    def collect(self, hours: int, now: Optional[datetime] = None) -> List[NewsItem]:
        """Fetch, sanitize, and return items published within the specified hours window."""
        pass

    def parse_datetime(self, date_val: Any) -> Optional[datetime]:
        """Parse various date formats (RFC 2822, ISO 8601, time.struct_time) into UTC datetime."""
        if isinstance(date_val, datetime):
            return normalize_datetime(date_val)

        if isinstance(date_val, time.struct_time):
            dt = datetime(*date_val[:6], tzinfo=timezone.utc)
            return dt

        if isinstance(date_val, str):
            clean_str = date_val.strip()
            # Try RFC 2822 format (standard for RSS pubDate)
            try:
                return normalize_datetime(parsedate_to_datetime(clean_str))
            except Exception:
                pass

            # Try ISO 8601 format (standard for Atom / JSON APIs)
            try:
                # Handle trailing 'Z'
                if clean_str.endswith("Z"):
                    clean_str = clean_str[:-1] + "+00:00"
                return normalize_datetime(datetime.fromisoformat(clean_str))
            except Exception:
                pass

        return None

    def build_news_item(
        self,
        title: str,
        url: str,
        published_at: datetime,
        raw_content: Optional[str] = None,
        source_name: Optional[str] = None,
    ) -> NewsItem:
        """Create a standardized NewsItem with sanitized snippet."""
        clean_snippet = sanitize_and_truncate(
            raw_content or "",
            config=self.cleaning_config,
        )
        return NewsItem(
            title=title.strip(),
            url=url.strip(),
            published_at=normalize_datetime(published_at),
            source=source_name or self.name,
            raw_content=raw_content,
            clean_snippet=clean_snippet,
        )

    def filter_items(
        self,
        items: List[NewsItem],
        hours: int,
        now: Optional[datetime] = None,
    ) -> List[NewsItem]:
        """Filter and sort items according to the requested time window."""
        return filter_by_time_window(items, hours=hours, now=now)
