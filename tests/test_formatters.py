"""Unit tests for formatters and file exporter."""

from datetime import datetime, timezone
from pathlib import Path

from daily_digest.config.models import OutputConfig
from daily_digest.export import DigestExporter
from daily_digest.formatters import MarkdownFormatter, PlainTextFormatter
from daily_digest.models import NewsItem


def test_markdown_formatter_empty():
    """Verify MarkdownFormatter renders clean notice when items list is empty."""
    formatter = MarkdownFormatter()
    now = datetime(2026, 10, 11, 10, 0, 0, tzinfo=timezone.utc)
    result = formatter.format([], time_window_hours=24, generated_at=now)

    assert "# Daily AI & Research Digest" in result
    assert "No articles or papers were found" in result
    assert "Last 24 Hours" in result


def test_markdown_formatter_populated():
    """Verify MarkdownFormatter renders structured sections and links."""
    formatter = MarkdownFormatter()
    now = datetime(2026, 10, 11, 10, 0, 0, tzinfo=timezone.utc)

    item1 = NewsItem(
        title="GPT-5 Announced",
        url="https://openai.com/gpt-5",
        published_at=now,
        source="OpenAI Blog",
        clean_snippet="Exciting launch.",
        summary="- Huge benchmark leap\n- Multimodal natively",
        priority_group="Major Releases & Breakthroughs",
        priority_score=100,
    )
    item2 = NewsItem(
        title="New Architecture Paper",
        url="https://arxiv.org/abs/2610.12345",
        published_at=now,
        source="arXiv",
        clean_snippet="Paper snippet text.",
        priority_group="Research & Benchmarks",
        priority_score=50,
    )

    result = formatter.format([item1, item2], time_window_hours=48, generated_at=now)

    assert "## 🚀 Major Releases & Breakthroughs" in result
    assert "### 1. [GPT-5 Announced](https://openai.com/gpt-5)" in result
    assert "**Source:** OpenAI Blog" in result
    assert "**AI Summary:**" in result
    assert "- Huge benchmark leap" in result
    assert "## 🔬 Research & Benchmarks" in result
    assert "### 1. [New Architecture Paper](https://arxiv.org/abs/2610.12345)" in result


def test_plain_text_formatter_populated():
    """Verify PlainTextFormatter produces formatted plain text with ASCII borders."""
    formatter = PlainTextFormatter()
    now = datetime(2026, 10, 11, 10, 0, 0, tzinfo=timezone.utc)

    item = NewsItem(
        title="Model Release",
        url="https://example.com/model",
        published_at=now,
        source="Lab",
        clean_snippet="Snippet text",
        summary="- Point 1",
        priority_group="Major Releases",
        priority_score=100,
    )

    result = formatter.format([item], time_window_hours=24, generated_at=now)

    assert "DAILY AI & RESEARCH DIGEST" in result
    assert "[MAJOR RELEASES] (1 items)" in result
    assert "1. Model Release" in result
    assert "URL:       https://example.com/model" in result
    assert "Point 1" in result


def test_digest_exporter_save(tmp_path: Path):
    """Verify DigestExporter writes content to disk with expected name and path."""
    out_dir = tmp_path / "custom_digests"
    cfg = OutputConfig(
        format="markdown",
        directory=out_dir,
        filename_pattern="daily_{date}.{ext}",
    )
    exporter = DigestExporter(config=cfg)

    now = datetime(2026, 10, 11, 14, 30, 0, tzinfo=timezone.utc)
    saved_path = exporter.save("# Sample Content", format_name="md", generated_at=now)

    assert saved_path.exists()
    assert saved_path.name == "daily_2026-10-11_1430.md"
    assert saved_path.read_text(encoding="utf-8") == "# Sample Content"


def test_digest_exporter_export_digest(tmp_path: Path):
    """Verify export_digest formats and writes file in one call."""
    out_dir = tmp_path / "digests"
    cfg = OutputConfig(format="text", directory=out_dir)
    exporter = DigestExporter(config=cfg)

    now = datetime(2026, 10, 11, 12, 0, 0, tzinfo=timezone.utc)
    item = NewsItem(title="News", url="http://x.com", published_at=now, source="Src")

    saved_path = exporter.export_digest([item], time_window_hours=12, generated_at=now)

    assert saved_path.exists()
    assert saved_path.suffix == ".txt"
    content = saved_path.read_text(encoding="utf-8")
    assert "DAILY AI & RESEARCH DIGEST" in content
