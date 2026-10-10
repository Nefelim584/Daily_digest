"""Resilient HTTP fetcher with exponential backoff retries, timeouts, and rate-limit handling."""

import logging
import time
from typing import Any, Dict, Optional
import httpx

logger = logging.getLogger(__name__)


class FetchError(Exception):
    """Exception raised when an HTTP fetch operation fails after retries."""

    def __init__(self, message: str, status_code: Optional[int] = None) -> None:
        super().__init__(message)
        self.status_code = status_code


class HTTPFetcher:
    """HTTP client wrapper with retry logic, exponential backoff, and rate-limit handling."""

    DEFAULT_USER_AGENT = (
        "Mozilla/5.0 (compatible; DailyDigest/0.1.0; +https://github.com/daily-digest)"
    )

    def __init__(
        self,
        timeout_seconds: float = 20.0,
        max_retries: int = 3,
        backoff_factor: float = 1.5,
        user_agent: Optional[str] = None,
    ) -> None:
        """Initialize HTTPFetcher.

        Args:
            timeout_seconds: Default request timeout in seconds.
            max_retries: Number of retry attempts on transient network or server errors.
            backoff_factor: Multiplier for exponential backoff between retries.
            user_agent: Custom User-Agent header string.
        """
        self.timeout_seconds = timeout_seconds
        self.max_retries = max_retries
        self.backoff_factor = backoff_factor
        self.headers = {
            "User-Agent": user_agent or self.DEFAULT_USER_AGENT,
            "Accept": "*/*",
        }

    def _should_retry(self, status_code: int) -> bool:
        """Determine whether an HTTP status code indicates a transient, retryable error."""
        # 429: Too Many Requests; 500, 502, 503, 504: Server errors
        return status_code in {429, 500, 502, 503, 504}

    def _get_sleep_duration(self, attempt: int, response: Optional[httpx.Response]) -> float:
        """Calculate wait time before retry, respecting Retry-After header when present."""
        if response is not None and response.status_code == 429:
            retry_after = response.headers.get("Retry-After")
            if retry_after:
                try:
                    return max(1.0, float(retry_after))
                except ValueError:
                    pass
        return self.backoff_factor * (2 ** attempt)

    def fetch_response(
        self,
        url: str,
        params: Optional[Dict[str, Any]] = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> httpx.Response:
        """Fetch an HTTP response with automatic retry and rate-limit handling."""
        combined_headers = {**self.headers, **(headers or {})}

        with httpx.Client(timeout=self.timeout_seconds, follow_redirects=True) as client:
            last_exception: Optional[Exception] = None
            last_status: Optional[int] = None

            for attempt in range(self.max_retries + 1):
                try:
                    response = client.get(url, params=params, headers=combined_headers)
                    last_status = response.status_code

                    if response.is_success:
                        return response

                    if self._should_retry(response.status_code) and attempt < self.max_retries:
                        sleep_time = self._get_sleep_duration(attempt, response)
                        logger.warning(
                            "HTTP %d fetching %s (attempt %d/%d). Retrying in %.1fs...",
                            response.status_code,
                            url,
                            attempt + 1,
                            self.max_retries,
                            sleep_time,
                        )
                        time.sleep(sleep_time)
                        continue

                    raise FetchError(
                        f"HTTP request to {url} failed with status {response.status_code}: {response.text[:200]}",
                        status_code=response.status_code,
                    )

                except (httpx.TimeoutException, httpx.NetworkError) as err:
                    last_exception = err
                    if attempt < self.max_retries:
                        sleep_time = self._get_sleep_duration(attempt, None)
                        logger.warning(
                            "Network/timeout error fetching %s (attempt %d/%d): %s. Retrying in %.1fs...",
                            url,
                            attempt + 1,
                            self.max_retries,
                            err,
                            sleep_time,
                        )
                        time.sleep(sleep_time)
                        continue
                    break

            raise FetchError(
                f"Failed to fetch {url} after {self.max_retries + 1} attempts. Error: {last_exception}",
                status_code=last_status,
            )

    def fetch_text(
        self,
        url: str,
        params: Optional[Dict[str, Any]] = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> str:
        """Fetch raw response text from the given URL."""
        response = self.fetch_response(url, params=params, headers=headers)
        return response.text

    def fetch_json(
        self,
        url: str,
        params: Optional[Dict[str, Any]] = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> Any:
        """Fetch and parse JSON from the given URL."""
        response = self.fetch_response(url, params=params, headers=headers)
        try:
            return response.json()
        except Exception as err:
            raise FetchError(f"Failed to parse JSON response from {url}: {err}") from err
