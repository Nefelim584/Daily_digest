"""Mistral AI client for generating concise news summaries."""

import logging
from typing import Iterable, List, Optional
from mistralai.client import Mistral

from ..config.models import MistralConfig
from ..models import NewsItem

logger = logging.getLogger(__name__)


class MistralSummarizer:
    """Summarizes news items using Mistral AI chat completions."""

    def __init__(self, config: Optional[MistralConfig] = None) -> None:
        """Initialize Mistral summarizer with configuration."""
        self.config = config or MistralConfig()
        self._client: Optional[Mistral] = None

        if self.config.enabled and self.config.api_key:
            try:
                self._client = Mistral(api_key=self.config.api_key)
            except Exception as err:
                logger.warning("Could not initialize Mistral client: %s", err)
                self._client = None

    def summarize_item(self, item: NewsItem) -> NewsItem:
        """Summarize an individual NewsItem with fallback handling."""
        if not self.config.enabled:
            return item

        if not self.config.api_key or self._client is None:
            logger.debug("Mistral API key not configured or client disabled; skipping summarization for '%s'", item.title)
            return item

        content_to_summarize = item.clean_snippet or item.raw_content
        if not content_to_summarize:
            logger.debug("No text content available to summarize for '%s'", item.title)
            return item

        prompt = (
            f"Title: {item.title}\n"
            f"Source: {item.source}\n"
            f"Content:\n{content_to_summarize}"
        )

        try:
            logger.info("Requesting Mistral summary for '%s' using model %s", item.title, self.config.model)
            response = self._client.chat.complete(
                model=self.config.model,
                messages=[
                    {"role": "system", "content": self.config.prompt_template},
                    {"role": "user", "content": prompt},
                ],
                temperature=self.config.temperature,
                max_tokens=self.config.max_tokens,
            )

            if response and response.choices and response.choices[0].message:
                summary_content = response.choices[0].message.content
                if isinstance(summary_content, str):
                    item.summary = summary_content.strip()
                elif isinstance(summary_content, list):
                    item.summary = "".join(str(chunk) for chunk in summary_content).strip()

        except Exception as err:
            logger.warning(
                "Mistral summarization failed for '%s' (fallback to clean snippet): %s",
                item.title,
                err,
            )
            # Item remains valid with item.summary = None

        return item

    def summarize_items(self, items: Iterable[NewsItem]) -> List[NewsItem]:
        """Summarize a collection of NewsItem objects sequentially."""
        summarized: List[NewsItem] = []
        for item in items:
            summarized.append(self.summarize_item(item))
        return summarized
