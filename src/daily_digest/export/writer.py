"""File export handler for saving generated news digests."""

import logging
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional, Sequence

from ..config.models import OutputConfig
from ..formatters import MarkdownFormatter, PlainTextFormatter
from ..models import NewsItem

logger = logging.getLogger(__name__)


class DigestExporter:
    """Handles writing formatted digests to disk with templated filenames and atomic file operations."""

    def __init__(
        self,
        config: Optional[OutputConfig] = None,
        clean_orphans_on_init: bool = True,
    ) -> None:
        """Initialize exporter with output configuration and clean orphan temp files."""
        self.config = config or OutputConfig()
        if clean_orphans_on_init:
            self.cleanup_temporary_files()

    def cleanup_temporary_files(self) -> int:
        """Clean up any orphan temporary files (.tmp) left by interrupted/crashed runs.

        Returns:
            Number of removed orphan temporary files.
        """
        target_dir = Path(self.config.directory)
        if not target_dir.exists():
            return 0

        removed = 0
        for pattern in (".*.tmp", "*.tmp"):
            for tmp_file in target_dir.glob(pattern):
                try:
                    if tmp_file.is_file():
                        tmp_file.unlink()
                        removed += 1
                        logger.info("Cleaned up orphan temporary file: %s", tmp_file)
                except OSError as err:
                    logger.warning("Failed to remove orphan temp file %s: %s", tmp_file, err)
        return removed

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
        """Save formatted text content to destination file atomically."""
        fmt = (format_name or self.config.format).strip().lower()
        filename = self.generate_filename(fmt, generated_at=generated_at)

        target_dir = Path(self.config.directory)
        target_dir.mkdir(parents=True, exist_ok=True)

        target_file = target_dir / filename
        # Atomic writing: write to temporary file in the same directory, then rename
        tmp_file = target_dir / f".{filename}.tmp"
        logger.info("Writing digest atomically via temp file %s -> %s", tmp_file, target_file)

        try:
            tmp_file.write_text(content, encoding="utf-8")
            os.replace(tmp_file, target_file)
        except Exception as err:
            if tmp_file.exists():
                try:
                    tmp_file.unlink()
                except OSError:
                    pass
            logger.error("Failed to write digest atomically: %s", err)
            raise

        logger.info("Digest successfully written to %s", target_file)
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
