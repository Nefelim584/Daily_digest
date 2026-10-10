"""Collector for arXiv papers API."""

import logging
from datetime import datetime
from typing import List, Optional
import feedparser

from ..config.models import CleaningConfig
from ..models import NewsItem
from ..network import HTTPFetcher, FetchError
from .base import BaseCollector

logger = logging.getLogger(__name__)


class ArxivCollector(BaseCollector):
    """Collects recent research papers from the arXiv API."""

    DEFAULT_ENDPOINT = "https://export.arxiv.org/api/query"

    def __init__(
        self,
        name: str = "arXiv AI & Security Papers",
        categories: Optional[List[str]] = None,
        max_results: int = 50,
        endpoint: str = DEFAULT_ENDPOINT,
        fetcher: Optional[HTTPFetcher] = None,
        cleaning_config: Optional[CleaningConfig] = None,
    ) -> None:
        """Initialize arXiv collector.

        Args:
            name: Source name.
            categories: List of arXiv category codes (e.g., cs.AI, cs.LG).
            max_results: Maximum number of records to request from arXiv.
            endpoint: ArXiv API query endpoint URL.
            fetcher: HTTP fetcher instance.
            cleaning_config: Cleaning configuration.
        """
        super().__init__(name=name, fetcher=fetcher, cleaning_config=cleaning_config)
        self.categories = categories or ["cs.AI", "cs.CL", "cs.LG", "cs.CV", "stat.ML", "cs.CR"]
        self.max_results = max_results
        self.endpoint = endpoint

    def _build_search_query(self) -> str:
        """Construct arXiv search query string from configured categories."""
        if not self.categories:
            return "cat:cs.AI"
        return " OR ".join(f"cat:{cat.strip()}" for cat in self.categories)

    def collect(self, hours: int, now: Optional[datetime] = None) -> List[NewsItem]:
        """Fetch papers from arXiv API and filter by time window."""
        search_query = self._build_search_query()
        params = {
            "search_query": search_query,
            "sortBy": "submittedDate",
            "sortOrder": "descending",
            "max_results": self.max_results,
        }

        try:
            logger.info("Fetching arXiv papers with query: %s", search_query)
            response_text = self.fetcher.fetch_text(self.endpoint, params=params)
        except FetchError as err:
            logger.error("Failed to fetch arXiv papers: %s", err)
            return []

        parsed = feedparser.parse(response_text)
        items: List[NewsItem] = []

        for entry in parsed.entries:
            title = entry.get("title", "").replace("\n", " ").strip()
            link = entry.get("link", "").strip() or entry.get("id", "").strip()

            date_val = entry.get("published_parsed") or entry.get("published") or entry.get("updated")
            published_at = self.parse_datetime(date_val)

            if not title or not link or published_at is None:
                continue

            summary = entry.get("summary", "")
            item = self.build_news_item(
                title=title,
                url=link,
                published_at=published_at,
                raw_content=summary,
            )
            items.append(item)

        return self.filter_items(items, hours=hours, now=now)
