"""Unit tests for HTML cleaning and text truncation."""

from daily_digest.cleaning import (
    HTMLCleaner,
    clean_html,
    truncate_text,
    sanitize_and_truncate,
)
from daily_digest.config.models import CleaningConfig


def test_clean_html_strips_scripts_and_styles():
    """Verify that script, style, and noscript blocks are entirely removed."""
    raw = (
        "<div><script>alert('bad');</script>"
        "<p>Real Content</p>"
        "<style>body { color: red; }</style>"
        "<noscript>Disabled JS</noscript></div>"
    )
    cleaned = clean_html(raw)
    assert cleaned == "Real Content"
    assert "alert" not in cleaned
    assert "body" not in cleaned
    assert "Disabled JS" not in cleaned


def test_clean_html_strips_images_and_links():
    """Verify that image tags and link tags are stripped, retaining link anchor text."""
    raw = (
        "<p>Check out <a href='https://example.com'>this model</a> "
        "<img src='photo.png' alt='illustration' /> and ![alt](fig.png).</p>"
    )
    cleaned = clean_html(raw)
    assert "this model" in cleaned
    assert "https://example.com" not in cleaned
    assert "photo.png" not in cleaned
    assert "fig.png" not in cleaned


def test_clean_html_unescapes_entities():
    """Verify that common HTML entities are properly unescaped."""
    raw = "<p>AT&amp;T &lt;DeepMind&gt; &quot;AI&quot; &#39;v1&#39;</p>"
    cleaned = clean_html(raw)
    assert cleaned == "AT&T <DeepMind> \"AI\" 'v1'"


def test_truncate_text_under_limit():
    """Verify that text shorter than max_characters is untouched."""
    text = "Short news announcement."
    assert truncate_text(text, max_characters=100) == text


def test_truncate_text_over_limit():
    """Verify that text exceeding max_characters is cleanly truncated with suffix."""
    words = ["word"] * 50
    text = " ".join(words)
    truncated = truncate_text(text, max_characters=50, suffix="...")
    assert len(truncated) <= 50
    assert truncated.endswith("...")
    assert not truncated.endswith(" ...")


def test_sanitize_and_truncate_integration():
    """Verify combined sanitization and truncation."""
    cfg = CleaningConfig(max_characters=100)
    raw = (
        "<p>This is a <b>very long announcement</b> about recent AI progress that needs truncation. "
        "It includes a lot of extra details, benchmarking numbers, and commentary that exceed 100 chars.</p>"
    )
    result = sanitize_and_truncate(raw, config=cfg)
    assert len(result) <= 100
    assert "<b>" not in result
    assert result.endswith("...")

