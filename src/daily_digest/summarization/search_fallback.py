"""Mistral AI Web Search fallback client for failed source feeds and routes."""

import json
import logging
import re
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from mistralai.client import Mistral
from mistralai.client.models import WebSearchTool

from ..config.models import MistralConfig
from ..models import NewsItem

logger = logging.getLogger(__name__)


class MistralSearchFallback:
    """Provides WebSearch-augmented fallback completions for failed news sources."""

    def __init__(self, config: Optional[MistralConfig] = None) -> None:
        """Initialize Mistral Search Fallback client.

        Args:
            config: Mistral configuration settings.
        """
        self.config = config or MistralConfig()
        self._client: Optional[Mistral] = None
        self._consecutive_failures: int = 0
        self._circuit_open: bool = False

        if self.config.enabled and self.config.web_search_fallback and self.config.api_key:
            try:
                self._client = Mistral(api_key=self.config.api_key)
            except Exception as err:
                logger.warning("Could not initialize Mistral client for search fallback: %s", err)
                self._client = None

    @property
    def is_available(self) -> bool:
        """Whether web search fallback is configured and active."""
        return bool(
            self.config.enabled
            and self.config.web_search_fallback
            and self.config.api_key
            and self._client is not None
            and not self._circuit_open
        )

    def _execute_search_conversation(self, prompt: str) -> Optional[str]:
        """Execute a single completion with WebSearchTool, with 429 retry and rate limiting.

        Args:
            prompt: User prompt instructing Mistral to search and format results.

        Returns:
            Raw assistant text response, or None on failure.
        """
        if not self.is_available:
            return None

        # Pacing delay between search queries
        if self.config.search_fallback_pacing_seconds > 0:
            time.sleep(self.config.search_fallback_pacing_seconds)

        model = self.config.model
        for attempt in range(self.config.max_retries + 1):
            try:
                logger.info("Executing Mistral WebSearch fallback using model '%s'", model)
                resp = self._client.beta.conversations.start(
                    model=model,
                    inputs=[{"role": "user", "content": prompt}],
                    tools=[WebSearchTool()],
                )

                # Extract response text across outputs
                extracted_text = ""
                for out in getattr(resp, "outputs", []):
                    content = getattr(out, "content", None)
                    if isinstance(content, str):
                        extracted_text += content
                    elif isinstance(content, list):
                        for chunk in content:
                            if isinstance(chunk, str):
                                extracted_text += chunk
                            elif hasattr(chunk, "text") and chunk.text:
                                extracted_text += chunk.text

                self._consecutive_failures = 0
                return extracted_text.strip()

            except Exception as err:
                err_str = str(err).lower()
                is_rate_limit = any(k in err_str for k in ("429", "rate limit", "rate_limited", "token rate limit"))

                if is_rate_limit:
                    if attempt < self.config.max_retries:
                        sleep_time = (self.config.backoff_factor ** (attempt + 1)) * 2.0
                        logger.warning(
                            "Mistral search rate-limited (429). Retrying in %.1fs (attempt %d/%d)...",
                            sleep_time,
                            attempt + 1,
                            self.config.max_retries,
                        )
                        time.sleep(sleep_time)
                        continue
                    else:
                        logger.error("Mistral search exhausted all retries due to rate limiting: %s", err)
                else:
                    logger.error("Mistral search query failed: %s", err)
                    break

        self._consecutive_failures += 1
        if self._consecutive_failures >= self.config.circuit_breaker_threshold:
            logger.warning("Mistral search circuit breaker opened after %d failures", self._consecutive_failures)
            self._circuit_open = True

        return None

    def _parse_json_array(self, raw_text: str) -> List[Dict[str, Any]]:
        """Extract and parse a JSON array from model output."""
        if not raw_text:
            return []

        # Try to locate markdown code block ```json ... ```
        json_match = re.search(r"```(?:json)?\s*(\[.*?\])\s*```", raw_text, re.DOTALL)
        candidate = json_match.group(1) if json_match else raw_text.strip()

        # Fallback: search for first [ and last ]
        if not candidate.startswith("["):
            start = candidate.find("[")
            end = candidate.rfind("]")
            if start != -1 and end != -1 and end > start:
                candidate = candidate[start : end + 1]

        try:
            parsed = json.loads(candidate)
            if isinstance(parsed, list):
                return parsed
        except Exception as err:
            logger.debug("Could not parse JSON array from search response: %s", err)

        return []

    def search_source_fallback(
        self,
        source_name: str,
        source_url: Optional[str] = None,
        hours: int = 24,
    ) -> List[NewsItem]:
        """Perform a web search fallback for an official blog or website source.

        Args:
            source_name: Name of the failed source (e.g. 'Anthropic Research').
            source_url: Optional original URL of the failed feed.
            hours: Time window in hours to search for news.

        Returns:
            List of NewsItem instances retrieved via search.
        """
        if not self.is_available:
            return []

        url_clause = f"({source_url})" if source_url else ""
        prompt = (
            f"Search for the latest official news, research releases, or announcements from '{source_name}' {url_clause} "
            f"published in the last {hours} hours.\n"
            "Return a valid JSON array of objects. Each object must have:\n"
            "- \"title\": clear headline of the announcement\n"
            "- \"url\": direct link to the official article, blog post, or announcement\n"
            "- \"summary\": 2-3 sentence summary focusing on key findings or releases\n\n"
            f"If no announcements were published in the last {hours} hours, return an empty array [].\n"
            "Output ONLY the JSON array inside a ```json ``` code block."
        )

        logger.info("Triggering Mistral WebSearch fallback for source '%s'", source_name)
        raw_text = self._execute_search_conversation(prompt)
        if not raw_text:
            return []

        parsed_items = self._parse_json_array(raw_text)
        items: List[NewsItem] = []
        now = datetime.now(timezone.utc)

        for obj in parsed_items:
            if not isinstance(obj, dict):
                continue
            title = str(obj.get("title", "")).strip()
            url = str(obj.get("url", "")).strip()
            summary = str(obj.get("summary", "")).strip()

            if not title or not url:
                continue

            item = NewsItem(
                title=title,
                url=url,
                published_at=now,
                source=f"{source_name} (Web Search)",
                raw_content=summary,
                clean_snippet=summary,
                summary=summary,
            )
            items.append(item)

        logger.info(
            "Mistral WebSearch fallback for '%s' retrieved %d items",
            source_name,
            len(items),
        )
        return items

    def search_twitter_fallback(
        self,
        username: str,
        source_label: str,
        hours: int = 24,
    ) -> List[NewsItem]:
        """Perform a web search fallback for an X/Twitter account.

        Requirements:
        - Do not link to individual tweets/posts (use user's profile URL).
        - Provide concise summary of posts over the last 24h.
        - Preserve external URLs mentioned in posts inside the summary text.

        Args:
            username: Twitter/X handle (without @).
            source_label: Human-readable source name.
            hours: Time window in hours.

        Returns:
            List of NewsItem instances.
        """
        if not self.is_available:
            return []

        clean_user = username.lstrip("@").strip()
        profile_url = f"https://x.com/{clean_user}"

        prompt = (
            f"Search for the latest tweets, posts, or activity from the Twitter/X account @{clean_user} "
            f"in the last {hours} hours.\n"
            "Return a valid JSON array of objects.\n"
            "Requirements:\n"
            f"- Each object represents an update/topic posted in the last {hours} hours.\n"
            f"- \"title\": Descriptive headline, e.g. \"@{clean_user} on [Topic]\"\n"
            f"- \"url\": \"{profile_url}\" (do NOT link to individual status/tweet IDs)\n"
            "- \"summary\": Concise summary of what was shared. IMPORTANT: Preserve any external links "
            "(e.g. GitHub repos, papers, articles) mentioned inside the post directly in the summary text.\n\n"
            f"If there were no posts from @{clean_user} in the last {hours} hours, return an empty array [].\n"
            "Output ONLY the JSON array inside a ```json ``` code block."
        )

        logger.info("Triggering Mistral WebSearch fallback for X/Twitter user @%s", clean_user)
        raw_text = self._execute_search_conversation(prompt)
        if not raw_text:
            return []

        parsed_items = self._parse_json_array(raw_text)
        items: List[NewsItem] = []
        now = datetime.now(timezone.utc)

        for obj in parsed_items:
            if not isinstance(obj, dict):
                continue
            title = str(obj.get("title", "")).strip() or f"@{clean_user} update"
            summary = str(obj.get("summary", "")).strip()
            # Enforce user profile URL (no status IDs)
            url = profile_url

            if not summary:
                continue

            item = NewsItem(
                title=title,
                url=url,
                published_at=now,
                source=f"{source_label} (Web Search)",
                raw_content=summary,
                clean_snippet=summary,
                summary=summary,
            )
            items.append(item)

        logger.info(
            "Mistral WebSearch fallback for @%s retrieved %d items",
            clean_user,
            len(items),
        )
        return items
