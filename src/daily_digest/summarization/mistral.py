import logging
import time
from typing import Iterable, List, Optional
from mistralai.client import Mistral

from ..config.models import MistralConfig
from ..models import NewsItem

logger = logging.getLogger(__name__)


class MistralSummarizer:
    """Summarizes news items using Mistral AI chat completions with rate limiting and resilience."""

    def __init__(self, config: Optional[MistralConfig] = None) -> None:
        """Initialize Mistral summarizer with configuration."""
        self.config = config or MistralConfig()
        self._client: Optional[Mistral] = None
        self._consecutive_failures: int = 0
        self._circuit_open: bool = False

        if self.config.enabled and self.config.api_key:
            try:
                self._client = Mistral(api_key=self.config.api_key)
            except Exception as err:
                logger.warning("Could not initialize Mistral client: %s", err)
                self._client = None

    def summarize_item(self, item: NewsItem) -> NewsItem:
        """Summarize an individual NewsItem with retry on 429 and circuit breaker fallback."""
        if not self.config.enabled:
            return item

        if not self.config.api_key or self._client is None:
            logger.debug("Mistral API key not configured or client disabled; skipping summarization for '%s'", item.title)
            return item

        if self._circuit_open:
            logger.debug("Circuit breaker open; skipping summarization for '%s'", item.title)
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

        # Rate-limiting pacing delay between calls
        if self.config.rate_limit_delay_seconds > 0:
            time.sleep(self.config.rate_limit_delay_seconds)

        for attempt in range(self.config.max_retries + 1):
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

                self._consecutive_failures = 0
                return item

            except Exception as err:
                err_str = str(err).lower()
                is_rate_limit = any(k in err_str for k in ("429", "rate limit", "rate_limited", "1300"))

                if is_rate_limit and attempt < self.config.max_retries:
                    backoff_delay = (self.config.backoff_factor ** attempt) * 2.0
                    logger.warning(
                        "Mistral rate limit encountered for '%s'. Retrying in %.1fs (attempt %d/%d)...",
                        item.title,
                        backoff_delay,
                        attempt + 1,
                        self.config.max_retries,
                    )
                    time.sleep(backoff_delay)
                    continue

                logger.warning(
                    "Mistral summarization failed for '%s' (fallback to clean snippet): %s",
                    item.title,
                    err,
                )

                self._consecutive_failures += 1
                if self._consecutive_failures >= self.config.circuit_breaker_threshold:
                    self._circuit_open = True
                    logger.warning(
                        "Mistral circuit breaker triggered after %d consecutive failures. "
                        "Skipping remaining AI summarizations for this run.",
                        self._consecutive_failures,
                    )
                break

        return item

    def summarize_items(self, items: Iterable[NewsItem]) -> List[NewsItem]:
        """Summarize a collection of NewsItem objects sequentially."""
        summarized: List[NewsItem] = []
        for item in items:
            summarized.append(self.summarize_item(item))
        return summarized

