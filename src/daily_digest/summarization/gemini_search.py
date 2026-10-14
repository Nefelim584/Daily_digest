"""Gemini AI Web Search fallback client using Google GenAI SDK and Search grounding."""

import json
import logging
import re
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

try:
    from google import genai
    from google.genai import types
    _HAS_GENAI = True
except ImportError:
    genai = None  # type: ignore
    types = None  # type: ignore
    _HAS_GENAI = False

from ..config.models import GeminiConfig
from ..models import NewsItem

logger = logging.getLogger(__name__)


class GeminiSearchFallback:
    """Provides Google Search-grounded fallback completions for failed news sources."""

    def __init__(self, config: Optional[GeminiConfig] = None) -> None:
        """Initialize Gemini Search Fallback client.

        Args:
            config: Gemini configuration settings.
        """
        self.config = config or GeminiConfig()
        self._client: Optional[Any] = None
        self._consecutive_failures: int = 0
        self._circuit_open: bool = False

        # Fallback model chain: primary flash model followed by flash-lite
        self._model_chain: List[str] = list(self.config.models) if self.config.models else [
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
        ]
        self._active_model_idx: int = 0

        if _HAS_GENAI and self.config.enabled and self.config.api_key:
            try:
                self._client = genai.Client(api_key=self.config.api_key)
            except Exception as err:
                logger.warning("Could not initialize Google GenAI client for search fallback: %s", err)
                self._client = None

    @property
    def is_available(self) -> bool:
        """Whether Gemini web search fallback is configured and operational."""
        return bool(
            _HAS_GENAI
            and self.config.enabled
            and self.config.api_key
            and self._client is not None
            and not self._circuit_open
        )

    @property
    def active_model(self) -> str:
        """Current active model name in the fallback chain."""
        if not self._model_chain:
            return "gemini-2.5-flash"
        return self._model_chain[self._active_model_idx % len(self._model_chain)]

    def _demote_active_model(self) -> None:
        """Switch to the next backup model in the chain."""
        if len(self._model_chain) > 1:
            self._active_model_idx = (self._active_model_idx + 1) % len(self._model_chain)
            logger.warning(
                "Demoting Gemini model to fallback backup: '%s'",
                self.active_model,
            )

    def _execute_search_query(self, prompt: str) -> Optional[str]:
        """Execute a completion with Google Search grounding tool, retries, and model demotion.

        Args:
            prompt: User prompt instructing Gemini to search and format results.

        Returns:
            Raw assistant text response, or None on failure.
        """
        if not self.is_available:
            return None

        # Pacing delay between search queries
        if self.config.search_fallback_pacing_seconds > 0:
            time.sleep(self.config.search_fallback_pacing_seconds)

        initial_model_idx = self._active_model_idx

        while True:
            model = self.active_model
            model_failed = False

            for attempt in range(self.config.max_retries + 1):
                try:
                    logger.info("Executing Gemini WebSearch fallback using model '%s'", model)
                    config_kwargs: Dict[str, Any] = {
                        "tools": [types.Tool(google_search=types.GoogleSearch())],
                        "temperature": self.config.temperature,
                    }
                    search_cfg = types.GenerateContentConfig(**config_kwargs)

                    response = self._client.models.generate_content(
                        model=model,
                        contents=prompt,
                        config=search_cfg,
                    )

                    text = getattr(response, "text", "") or ""
                    self._consecutive_failures = 0
                    return text.strip()

                except Exception as err:
                    err_str = str(err).lower()
                    is_rate_limit = any(
                        k in err_str
                        for k in ("429", "rate limit", "rate_limited", "resource_exhausted", "quota")
                    )

                    if is_rate_limit:
                        if attempt < self.config.max_retries:
                            sleep_time = (self.config.backoff_factor ** (attempt + 1)) * 2.0
                            logger.warning(
                                "Gemini search rate-limited on model '%s'. Retrying in %.1fs (attempt %d/%d)...",
                                model,
                                sleep_time,
                                attempt + 1,
                                self.config.max_retries,
                            )
                            time.sleep(sleep_time)
                            continue
                        else:
                            logger.error(
                                "Gemini model '%s' exhausted retries due to rate limit: %s",
                                model,
                                err,
                            )
                            model_failed = True
                            break
                    else:
                        logger.error("Gemini search query failed on model '%s': %s", model, err)
                        model_failed = True
                        break

            if model_failed:
                if len(self._model_chain) > 1:
                    self._demote_active_model()
                    if self._active_model_idx == initial_model_idx:
                        # Cycled through all fallback models
                        break
                else:
                    break

        self._consecutive_failures += 1
        if self._consecutive_failures >= self.config.circuit_breaker_threshold:
            logger.warning(
                "Gemini search circuit breaker opened after %d failures",
                self._consecutive_failures,
            )
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
            logger.debug("Could not parse JSON array from Gemini search response: %s", err)

        return []

    def search_source_fallback(
        self,
        source_name: str,
        source_url: Optional[str] = None,
        hours: int = 24,
    ) -> List[NewsItem]:
        """Perform a web search fallback for an official blog or website source using Gemini.

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

        logger.info("Triggering Gemini WebSearch fallback for source '%s'", source_name)
        raw_text = self._execute_search_query(prompt)
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
            "Gemini WebSearch fallback for '%s' retrieved %d items",
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
        """Perform a web search fallback for an X/Twitter account using Gemini.

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

        logger.info("Triggering Gemini WebSearch fallback for X/Twitter user @%s", clean_user)
        raw_text = self._execute_search_query(prompt)
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
            "Gemini WebSearch fallback for @%s retrieved %d items",
            clean_user,
            len(items),
        )
        return items
