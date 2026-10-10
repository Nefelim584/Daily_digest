"""Markdown formatter for news digests."""

from datetime import datetime
from typing import Dict, List, Optional, Sequence

from ..filters import normalize_datetime
from ..models import NewsItem
from .base import BaseFormatter


class MarkdownFormatter(BaseFormatter):
    """Formats news items into structured GitHub-flavored Markdown."""

    SECTION_ICONS = {
        "Major Releases & Breakthroughs": "🚀",
        "Research & Benchmarks": "🔬",
        "General News & Updates": "📰",
    }

    def format(
        self,
        items: Sequence[NewsItem],
        time_window_hours: int,
        generated_at: Optional[datetime] = None,
    ) -> str:
        """Render a list of NewsItem objects into Markdown text."""
        gen_time = self.get_generated_time(generated_at)
        formatted_date = gen_time.strftime("%Y-%m-%d %H:%M UTC")

        lines: List[str] = [
            "# Daily AI & Research Digest",
            "",
            f"> **Generated:** {formatted_date} | **Time Window:** Last {time_window_hours} Hours | **Total Items:** {len(items)}",
            "",
        ]

        if not items:
            lines.extend([
                "No articles or papers were found for the selected time window.",
                "",
            ])
            return "\n".join(lines)

        # Group items by priority category while preserving priority ordering
        groups: Dict[str, List[NewsItem]] = {}
        for item in items:
            group_name = item.priority_group or "General News & Updates"
            if group_name not in groups:
                groups[group_name] = []
            groups[group_name].append(item)

        for group_name, group_items in groups.items():
            icon = self.SECTION_ICONS.get(group_name, "📌")
            lines.extend([
                f"## {icon} {group_name}",
                "",
            ])

            for idx, item in enumerate(group_items, 1):
                item_date = normalize_datetime(item.published_at).strftime("%Y-%m-%d %H:%M UTC")
                lines.append(f"### {idx}. [{item.title}]({item.url})")
                lines.append(f"- **Source:** {item.source} | **Published:** {item_date}")

                if item.summary:
                    lines.append("- **AI Summary:**")
                    # Indent or format bullet points if multiline
                    for summary_line in item.summary.strip().split("\n"):
                        summary_line = summary_line.strip()
                        if summary_line.startswith(("-", "*", "•")):
                            lines.append(f"  {summary_line}")
                        elif summary_line:
                            lines.append(f"  - {summary_line}")

                if item.clean_snippet:
                    lines.append("- **Snippet:**")
                    lines.append(f"  > {item.clean_snippet}")

                lines.append("")

            lines.append("---")
            lines.append("")

        return "\n".join(lines).rstrip() + "\n"
