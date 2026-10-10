"""Text truncation utility."""

from typing import Optional
from ..config.models import CleaningConfig
from .cleaner import clean_html


def truncate_text(
    text: str,
    max_characters: int = 1500,
    suffix: str = "...",
    break_on_word: bool = True,
) -> str:
    """Truncate text to at most max_characters while preserving word boundaries if possible."""
    if not text:
        return ""

    text = text.strip()
    if len(text) <= max_characters:
        return text

    if max_characters <= len(suffix):
        return suffix[:max_characters]

    target_length = max_characters - len(suffix)

    if break_on_word:
        # Find last whitespace within target_length
        cut_text = text[:target_length]
        last_space = cut_text.rfind(" ")
        # Only break on space if it doesn't discard too much text (e.g., at least 70% of target)
        if last_space != -1 and last_space >= int(target_length * 0.7):
            return cut_text[:last_space].rstrip() + suffix

    return text[:target_length].rstrip() + suffix


def sanitize_and_truncate(
    raw_text: Optional[str],
    config: Optional[CleaningConfig] = None,
) -> str:
    """Clean HTML tags and truncate text according to CleaningConfig."""
    active_config = config or CleaningConfig()
    cleaned = clean_html(raw_text, config=active_config)
    return truncate_text(
        cleaned,
        max_characters=active_config.max_characters,
    )
