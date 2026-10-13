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

        # Build prioritized chain of models: primary model first, followed by fallbacks
        self._model_chain: List[str] = [self.config.model]
        for fb_model in self.config.fallback_models:
            if fb_model not in self._model_chain:
                self._model_chain.append(fb_model)
        self._active_model_idx: int = 0

        if self.config.enabled and self.config.api_key:
            try:
                self._client = Mistral(api_key=self.config.api_key)
            except Exception as err:
                logger.warning("Could not initialize Mistral client: %s", err)
                self._client = None

    def summarize_item(self, item: NewsItem) -> NewsItem:
        """Summarize an individual NewsItem with model fallback, 429 retry, and circuit breaker."""
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

        # Iterate through model chain starting from the currently active model
        for model_idx in range(self._active_model_idx, len(self._model_chain)):
            current_model = self._model_chain[model_idx]
            model_succeeded = False

            for attempt in range(self.config.max_retries + 1):
                try:
                    logger.info("Requesting Mistral summary for '%s' using model %s", item.title, current_model)
                    response = self._client.chat.complete(
                        model=current_model,
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
                    # If fallback model worked, promote it as active to avoid repeating failed model calls
                    if model_idx > self._active_model_idx:
                        logger.info("Promoting working fallback model %s to active", current_model)
                        self._active_model_idx = model_idx
                    return item

                except Exception as err:
                    err_str = str(err).lower()
                    is_rate_limit = any(k in err_str for k in ("429", "rate limit", "rate_limited", "1300"))
                    has_fallback = model_idx + 1 < len(self._model_chain)

                    if is_rate_limit:
                        # If a fallback model is available, switch immediately without wasting slow retries
                        if has_fallback:
                            logger.warning(
                                "Mistral model '%s' rate limited/exhausted for '%s' (%s). "
                                "Switching to next fallback model '%s'...",
                                current_model,
                                item.title,
                                err,
                                self._model_chain[model_idx + 1],
                            )
                            break  # Exit attempt loop to advance to next model in model_chain

                        if attempt < self.config.max_retries:
                            backoff_delay = (self.config.backoff_factor ** attempt) * 2.0
                            logger.warning(
                                "Mistral rate limit on last fallback '%s' for '%s'. Retrying in %.1fs (attempt %d/%d)...",
                                current_model,
                                item.title,
                                backoff_delay,
                                attempt + 1,
                                self.config.max_retries,
                            )
                            time.sleep(backoff_delay)
                            continue

                    logger.warning(
                        "Mistral summarization with model '%s' failed for '%s': %s",
                        current_model,
                        item.title,
                        err,
                    )
                    if has_fallback:
                        logger.info("Switching to next fallback model '%s'...", self._model_chain[model_idx + 1])
                        break  # Try next model

        # If all models in the fallback chain failed for this item:
        self._consecutive_failures += 1
        if self._consecutive_failures >= self.config.circuit_breaker_threshold:
            self._circuit_open = True
            logger.warning(
                "Mistral circuit breaker triggered after %d consecutive failures across all fallback models. "
                "Skipping remaining AI summarizations for this run.",
                self._consecutive_failures,
            )

        return item

    def summarize_items(self, items: Iterable[NewsItem]) -> List[NewsItem]:
        """Summarize a collection of NewsItem objects sequentially."""
        summarized: List[NewsItem] = []
        for item in items:
            summarized.append(self.summarize_item(item))
        return summarized

