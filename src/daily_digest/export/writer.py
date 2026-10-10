"""File export handler for saving generated news digests."""

import logging
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional, Sequence

from ..config.models import OutputConfig
from ..formatters import MarkdownFormatter, PlainTextFormatter
from ..models import NewsItem

logger = logging.getLogger(__name__)


class DigestExporter:
    """Handles writing formatted digests to disk with templated filenames."""

    def __init__(self, config: Optional[OutputConfig] = None) -> None:
        """Initialize exporter with output configuration."""
        self.config = config or OutputConfig()

    def generate_filename(
        self,
        format_name: str,
        generated_at: Optional[datetime] = None,
    ) -> str:
        """Generate filename using configured pattern and current timestamp."""
        gen_time = generated_at or datetime.now(timezone.utc)
        date_str = gen_time.strftime("%Y-%m-%d_%H%M")
        ext = "md" if format_name.lower() in {"markdown", "md"} else "txt"

        try:
            return self.config.filename_pattern.format(date=date_str, ext=ext)
        except (KeyError, IndexError):
            return f"digest_{date_str}.{ext}"

    def save(
        self,
        content: str,
        format_name: Optional[str] = None,
        generated_at: Optional[datetime] = None,
    ) -> Path:
        """Save formatted text content to destination file."""
        fmt = (format_name or self.config.format).strip().lower()
        filename = self.generate_filename(fmt, generated_at=generated_at)

        target_dir = Path(self.config.directory)
        target_dir.mkdir(parents=True, exist_ok=True)

        target_file = target_dir / filename
        logger.info("Writing digest to %s", target_file)
        target_file.write_text(content, encoding="utf-8")
        return target_file

    def export_digest(
        self,
        items: Sequence[NewsItem],
        time_window_hours: int,
        format_name: Optional[str] = None,
        generated_at: Optional[datetime] = None,
    ) -> Path:
        """Format and save items to disk in one step."""
        fmt = (format_name or self.config.format).strip().lower()
        if fmt in {"markdown", "md"}:
            formatter = MarkdownFormatter()
        else:
            formatter = PlainTextFormatter()

        content = formatter.format(items, time_window_hours=time_window_hours, generated_at=generated_at)
        return self.save(content, format_name=fmt, generated_at=generated_at)
