"""Collector for feeds retrieved via RSSHub."""

import logging
from datetime import datetime
from typing import Dict, List, Optional
import feedparser

from ..config.models import CleaningConfig
from ..models import NewsItem
from ..network import HTTPFetcher, FetchError
from .base import BaseCollector

logger = logging.getLogger(__name__)


class RSSHubCollector(BaseCollector):
    """Collects feeds from an RSSHub instance across multiple configured routes."""

    def __init__(
        self,
        name: str,
        routes: List[str],
        base_url: str = "https://rsshub.app",
        access_key: str = "",
        fetcher: Optional[HTTPFetcher] = None,
        cleaning_config: Optional[CleaningConfig] = None,
        search_fallback: Optional[Any] = None,
    ) -> None:
        """Initialize RSSHub collector.

        Args:
            name: Source group name.
            routes: List of RSSHub route paths (e.g., ['/twitter/user/OpenAI']).
            base_url: Base URL of RSSHub server.
            access_key: Optional access key/token for RSSHub authentication.
            fetcher: HTTP fetcher instance.
            cleaning_config: Cleaning configuration.
            search_fallback: Optional MistralSearchFallback instance.
        """
        super().__init__(name=name, fetcher=fetcher, cleaning_config=cleaning_config)
        self.routes = routes
        self.base_url = base_url.rstrip("/")
        self.access_key = access_key
        self.search_fallback = search_fallback

    def _extract_content(self, entry: feedparser.FeedParserDict) -> str:
        """Extract post content or description from feed entry."""
        if hasattr(entry, "content") and entry.content:
            for item in entry.content:
                if isinstance(item, dict) and "value" in item and item["value"]:
                    return item["value"]

        if hasattr(entry, "summary") and entry.summary:
            return entry.summary

        if hasattr(entry, "description") and entry.description:
            return entry.description

        return ""

    def _trigger_fallback(self, route: str, url: str, source_label: str, hours: int) -> List[NewsItem]:
        """Trigger Mistral web search fallback for a failed RSSHub route."""
        if not self.search_fallback or not getattr(self.search_fallback, "is_available", False):
            return []

        route_clean = route.strip("/")
        is_twitter = "/twitter/user/" in route or route_clean.startswith("twitter/user/")
        route_label = route_clean.split("/")[-1]

        if is_twitter:
            return self.search_fallback.search_twitter_fallback(
                username=route_label,
                source_label=source_label,
                hours=hours,
            )
        return self.search_fallback.search_source_fallback(
            source_name=source_label,
            source_url=url,
            hours=hours,
        )

    def _collect_route(self, route: str, hours: int = 24) -> List[NewsItem]:
        """Fetch and parse a single RSSHub route, falling back to Mistral search on error."""
        url = f"{self.base_url}/{route.lstrip('/')}"
        params: Dict[str, str] = {}
        headers: Dict[str, str] = {}

        if self.access_key:
            params["key"] = self.access_key
            headers["Authorization"] = f"Bearer {self.access_key}"

        route_label = route.strip("/").split("/")[-1]
        source_label = f"{self.name} (@{route_label})" if "/twitter/user/" in route else self.name

        try:
            logger.info("Fetching RSSHub route %s", url)
            xml_text = self.fetcher.fetch_text(url, params=params if params else None, headers=headers if headers else None)
        except FetchError as err:
            logger.warning("Could not fetch RSSHub route %s: %s", url, err)
            return self._trigger_fallback(route, url, source_label, hours)

        parsed = feedparser.parse(xml_text)
        if parsed.bozo and not parsed.entries:
            logger.warning("Failed to parse RSSHub response for %s: %s", url, parsed.bozo_exception)
            return self._trigger_fallback(route, url, source_label, hours)

        items: List[NewsItem] = []
        for entry in parsed.entries:
            title = entry.get("title", "").replace("\n", " ").strip()
            link = entry.get("link", "").strip() or entry.get("id", "").strip()

            date_val = (
                entry.get("published_parsed")
                or entry.get("updated_parsed")
                or entry.get("published")
                or entry.get("updated")
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
                source_name=source_label,
            )
            items.append(item)

        return items

    def collect(self, hours: int, now: Optional[datetime] = None) -> List[NewsItem]:
        """Fetch items across all configured routes and filter by time window."""
        all_items: List[NewsItem] = []

        for route in self.routes:
            items = self._collect_route(route, hours=hours)
            all_items.extend(items)

        return self.filter_items(all_items, hours=hours, now=now)
