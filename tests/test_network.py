"""Unit tests for resilient HTTP fetcher."""

from unittest.mock import MagicMock, patch
import httpx
import pytest

from daily_digest.network import HTTPFetcher, FetchError


def test_fetch_text_success():
    """Verify fetch_text returns response string on 200 OK."""
    fetcher = HTTPFetcher(timeout_seconds=5)

    mock_resp = MagicMock(spec=httpx.Response)
    mock_resp.is_success = True
    mock_resp.status_code = 200
    mock_resp.text = "Mocked Response Body"

    with patch.object(httpx.Client, "get", return_value=mock_resp) as mock_get:
        result = fetcher.fetch_text("https://example.com/test")
        assert result == "Mocked Response Body"
        mock_get.assert_called_once()


def test_fetch_json_success():
    """Verify fetch_json parses and returns JSON structure on 200 OK."""
    fetcher = HTTPFetcher(timeout_seconds=5)

    mock_resp = MagicMock(spec=httpx.Response)
    mock_resp.is_success = True
    mock_resp.status_code = 200
    mock_resp.json.return_value = {"key": "value", "count": 42}

    with patch.object(httpx.Client, "get", return_value=mock_resp):
        data = fetcher.fetch_json("https://example.com/api")
        assert data == {"key": "value", "count": 42}


def test_retry_on_server_error_and_succeed():
    """Verify fetcher retries on transient 500 error and succeeds on subsequent try."""
    fetcher = HTTPFetcher(timeout_seconds=5, max_retries=2, backoff_factor=0.01)

    fail_resp = MagicMock(spec=httpx.Response)
    fail_resp.is_success = False
    fail_resp.status_code = 500
    fail_resp.text = "Internal Server Error"

    succ_resp = MagicMock(spec=httpx.Response)
    succ_resp.is_success = True
    succ_resp.status_code = 200
    succ_resp.text = "Success on retry"

    with patch.object(httpx.Client, "get", side_effect=[fail_resp, succ_resp]) as mock_get:
        with patch("time.sleep") as mock_sleep:
            result = fetcher.fetch_text("https://example.com/flaky")
            assert result == "Success on retry"
            assert mock_get.call_count == 2
            mock_sleep.assert_called_once()


def test_retry_exhaustion_raises_fetch_error():
    """Verify exceeding max_retries raises FetchError."""
    fetcher = HTTPFetcher(timeout_seconds=5, max_retries=1, backoff_factor=0.01)

    fail_resp = MagicMock(spec=httpx.Response)
    fail_resp.is_success = False
    fail_resp.status_code = 503
    fail_resp.text = "Service Unavailable"

    with patch.object(httpx.Client, "get", return_value=fail_resp):
        with patch("time.sleep"):
            with pytest.raises(FetchError, match="Service Unavailable"):
                fetcher.fetch_text("https://example.com/unavailable")
