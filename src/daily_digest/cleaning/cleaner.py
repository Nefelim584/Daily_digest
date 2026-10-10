"""Regex-based HTML cleaner and text sanitizer."""

import html
import re
from typing import Optional
from ..config.models import CleaningConfig


class HTMLCleaner:
    """Sanitizes HTML and raw text content using regular expressions."""

    SCRIPT_STYLE_PATTERN = re.compile(
        r"<(script|style|noscript)\b[^>]*>.*?</\1>",
        flags=re.IGNORECASE | re.DOTALL,
    )
    IMAGE_PATTERN = re.compile(
        r"<img\b[^>]*>|<picture\b[^>]*>.*?</picture>|<figure\b[^>]*>.*?</figure>|!\[.*?\]\(.*?\)",
        flags=re.IGNORECASE | re.DOTALL,
    )
    LINK_TAG_PATTERN = re.compile(
        r"<a\b[^>]*>(.*?)</a>|\[([^\]]+)\]\([^\)]+\)",
        flags=re.IGNORECASE | re.DOTALL,
    )
    HTML_TAG_PATTERN = re.compile(r"<[^>]+>", flags=re.DOTALL)
    WHITESPACE_PATTERN = re.compile(r"[ \t\r\f\v]+")
    PUNCT_WHITESPACE_PATTERN = re.compile(r"\s+([.,!?;:)\]])")
    MULTIPLE_NEWLINES_PATTERN = re.compile(r"\n{3,}")

    def __init__(self, config: Optional[CleaningConfig] = None) -> None:
        """Initialize cleaner with optional cleaning configuration."""
        self.config = config or CleaningConfig()

    def clean(self, raw_text: Optional[str]) -> str:
        """Clean and sanitize the input text/HTML according to configuration."""
        if not raw_text:
            return ""

        text = raw_text

        # 1. Strip script and style blocks
        if self.config.strip_scripts:
            text = self.SCRIPT_STYLE_PATTERN.sub(" ", text)

        # 2. Strip images and media
        if self.config.strip_images:
            text = self.IMAGE_PATTERN.sub(" ", text)

        # 3. Strip links (keep inner anchor text if present)
        if self.config.strip_links:
            # Replace <a href="...">text</a> or [text](url) with just text
            text = self.LINK_TAG_PATTERN.sub(lambda m: m.group(1) or m.group(2) or "", text)

        # 4. Strip remaining HTML tags
        if self.config.strip_html_tags:
            text = self.HTML_TAG_PATTERN.sub(" ", text)

        # 5. Decode HTML entities (e.g. &amp;, &quot;, &#39;)
        text = html.unescape(text)

        # 6. Normalize whitespace
        text = self.WHITESPACE_PATTERN.sub(" ", text)
        text = self.PUNCT_WHITESPACE_PATTERN.sub(r"\1", text)
        lines = [line.strip() for line in text.split("\n")]
        text = "\n".join(line for line in lines if line)
        text = self.MULTIPLE_NEWLINES_PATTERN.sub("\n\n", text)

        return text.strip()


def clean_html(text: Optional[str], config: Optional[CleaningConfig] = None) -> str:
    """Convenience function to sanitize HTML/text using HTMLCleaner."""
    cleaner = HTMLCleaner(config=config)
    return cleaner.clean(text)
