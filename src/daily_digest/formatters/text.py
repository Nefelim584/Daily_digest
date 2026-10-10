"""Plain text formatter for news digests."""

from datetime import datetime
from typing import Dict, List, Optional, Sequence

from ..filters import normalize_datetime
from ..models import NewsItem
from .base import BaseFormatter


class PlainTextFormatter(BaseFormatter):
    """Formats news items into structured plain text."""

    DIVIDER_MAJOR = "=" * 80
    DIVIDER_MINOR = "-" * 80

    def format(
        self,
        items: Sequence[NewsItem],
        time_window_hours: int,
        generated_at: Optional[datetime] = None,
    ) -> str:
        """Render a list of NewsItem objects into plain text."""
        gen_time = self.get_generated_time(generated_at)
        formatted_date = gen_time.strftime("%Y-%m-%d %H:%M UTC")

        lines: List[str] = [
            self.DIVIDER_MAJOR,
            "DAILY AI & RESEARCH DIGEST",
            f"Generated: {formatted_date} | Window: Last {time_window_hours} Hours | Total Items: {len(items)}",
            self.DIVIDER_MAJOR,
            "",
        ]

        if not items:
            lines.extend([
                "No articles or papers were found for the selected time window.",
                "",
            ])
            return "\n".join(lines)

        groups: Dict[str, List[NewsItem]] = {}
        for item in items:
            group_name = item.priority_group or "General News & Updates"
            if group_name not in groups:
                groups[group_name] = []
            groups[group_name].append(item)

        for group_name, group_items in groups.items():
            lines.extend([
                self.DIVIDER_MINOR,
                f"[{group_name.upper()}] ({len(group_items)} items)",
                self.DIVIDER_MINOR,
                "",
            ])

            for idx, item in enumerate(group_items, 1):
                item_date = normalize_datetime(item.published_at).strftime("%Y-%m-%d %H:%M UTC")
                lines.append(f"{idx}. {item.title}")
                lines.append(f"   URL:       {item.url}")
                lines.append(f"   Source:    {item.source} | Published: {item_date}")

                if item.summary:
                    lines.append("   Summary:")
                    for s_line in item.summary.strip().split("\n"):
                        s_line = s_line.strip()
                        if s_line:
                            lines.append(f"     {s_line}")

                if item.clean_snippet:
                    lines.append("   Snippet:")
                    for c_line in item.clean_snippet.strip().split("\n"):
                        c_line = c_line.strip()
                        if c_line:
                            lines.append(f"     {c_line}")

                lines.append("")

        return "\n".join(lines).rstrip() + "\n"
