"""Collector for Hugging Face Daily Papers API."""

import logging
from datetime import datetime
from typing import Any, Dict, List, Optional

from ..config.models import CleaningConfig
from ..models import NewsItem
from ..network import HTTPFetcher, FetchError
from .base import BaseCollector

logger = logging.getLogger(__name__)


class HuggingFacePapersCollector(BaseCollector):
    """Collects trending research papers from Hugging Face Daily Papers."""

    DEFAULT_ENDPOINT = "https://huggingface.co/api/daily_papers"

    def __init__(
        self,
        name: str = "Hugging Face Daily Papers",
        endpoint: str = DEFAULT_ENDPOINT,
        fetcher: Optional[HTTPFetcher] = None,
        cleaning_config: Optional[CleaningConfig] = None,
    ) -> None:
        """Initialize Hugging Face Daily Papers collector."""
        super().__init__(name=name, fetcher=fetcher, cleaning_config=cleaning_config)
        self.endpoint = endpoint

    def collect(self, hours: int, now: Optional[datetime] = None) -> List[NewsItem]:
        """Fetch papers from Hugging Face Daily Papers API and filter by time window."""
        try:
            logger.info("Fetching Hugging Face Daily Papers from %s", self.endpoint)
            data = self.fetcher.fetch_json(self.endpoint)
        except FetchError as err:
            logger.error("Failed to fetch Hugging Face Daily Papers: %s", err)
            return []

        if not isinstance(data, list):
            logger.warning("Unexpected response structure from Hugging Face API: expected list")
            return []

        items: List[NewsItem] = []

        for record in data:
            if not isinstance(record, dict):
                continue

            paper: Dict[str, Any] = record.get("paper", {})
            title = (
                paper.get("title")
                or record.get("title")
                or ""
            ).replace("\n", " ").strip()

            paper_id = paper.get("id") or record.get("id")
            if paper_id:
                url = f"https://huggingface.co/papers/{paper_id}"
            else:
                url = "https://huggingface.co/papers"

            date_str = (
                paper.get("publishedAt")
                or record.get("publishedAt")
                or paper.get("submittedOnDailyAt")
            )
            published_at = self.parse_datetime(date_str) if date_str else None

            if not title or published_at is None:
                continue

            summary = paper.get("summary") or record.get("summary") or ""

            item = self.build_news_item(
                title=title,
                url=url,
                published_at=published_at,
                raw_content=summary,
            )
            items.append(item)

        return self.filter_items(items, hours=hours, now=now)
