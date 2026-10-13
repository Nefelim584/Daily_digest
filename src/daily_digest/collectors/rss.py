"""Collector for standard RSS and Atom feeds."""

import logging
from datetime import datetime
from typing import Any, List, Optional
import feedparser

from ..config.models import CleaningConfig
from ..models import NewsItem
from ..network import HTTPFetcher, FetchError
from .base import BaseCollector

logger = logging.getLogger(__name__)


class RSSCollector(BaseCollector):
    """Collects news items from standard RSS or Atom feeds."""

    def __init__(
        self,
        name: str,
        url: str,
        fetcher: Optional[HTTPFetcher] = None,
        cleaning_config: Optional[CleaningConfig] = None,
        search_fallback: Optional[Any] = None,
    ) -> None:
        """Initialize RSS collector.

        Args:
            name: Source name.
            url: RSS/Atom feed URL.
            fetcher: HTTP fetcher instance.
            cleaning_config: Cleaning configuration.
            search_fallback: Optional MistralSearchFallback instance.
        """
        super().__init__(name=name, fetcher=fetcher, cleaning_config=cleaning_config)
        self.url = url
        self.search_fallback = search_fallback

    def _extract_content(self, entry: feedparser.FeedParserDict) -> str:
        """Extract article content or description from feed entry."""
        if hasattr(entry, "content") and entry.content:
            for item in entry.content:
                if isinstance(item, dict) and "value" in item and item["value"]:
                    return item["value"]

        if hasattr(entry, "summary") and entry.summary:
            return entry.summary

        if hasattr(entry, "description") and entry.description:
            return entry.description

        return ""

    def collect(self, hours: int, now: Optional[datetime] = None) -> List[NewsItem]:
        """Fetch and parse RSS/Atom feed, filtering entries by time window."""
        try:
            logger.info("Fetching RSS feed for '%s' from %s", self.name, self.url)
            xml_text = self.fetcher.fetch_text(self.url)
        except FetchError as err:
            logger.warning("Could not fetch RSS feed for '%s' (%s): %s", self.name, self.url, err)
            if self.search_fallback and getattr(self.search_fallback, "is_available", False):
                return self.search_fallback.search_source_fallback(
                    source_name=self.name,
                    source_url=self.url,
                    hours=hours,
                )
            return []

        parsed = feedparser.parse(xml_text)
        if parsed.bozo and not parsed.entries:
            logger.warning(
                "Failed to parse XML feed for '%s' (%s): %s",
                self.name,
                self.url,
                parsed.bozo_exception,
            )
            if self.search_fallback and getattr(self.search_fallback, "is_available", False):
                return self.search_fallback.search_source_fallback(
                    source_name=self.name,
                    source_url=self.url,
                    hours=hours,
                )
            return []

        items: List[NewsItem] = []

        for entry in parsed.entries:
            title = entry.get("title", "").replace("\n", " ").strip()
            link = entry.get("link", "").strip() or entry.get("id", "").strip()

            date_val = (
                entry.get("published_parsed")
                or entry.get("updated_parsed")
                or entry.get("created_parsed")
                or entry.get("published")
                or entry.get("updated")
                or entry.get("pubDate")
            )
            published_at = self.parse_datetime(date_val)

            if not title or not link or published_at is None:
                continue

            raw_content = self._extract_content(entry)
            item = self.build_news_item(
                title=title,
                url=link,
                published_at=published_at,
                raw_content=raw_content,
            )
            items.append(item)

        return self.filter_items(items, hours=hours, now=now)
