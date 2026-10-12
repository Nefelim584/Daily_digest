"""Digest pipeline orchestrator coordinating collectors, classifiers, summarizers, and export."""

import logging
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import List, Optional, Set
from urllib.parse import urlparse

from ..classifier import PriorityClassifier
from ..collectors import create_collectors, BaseCollector
from ..config.models import AppConfig, OutputConfig
from ..export import DigestExporter
from ..models import NewsItem
from ..summarization import MistralSummarizer

logger = logging.getLogger(__name__)


@dataclass
class PipelineResult:
    """Summary of pipeline execution results."""

    total_collected: int
    total_unique: int
    time_window_hours: int
    output_path: Path
    items: List[NewsItem] = field(default_factory=list)


def normalize_url(url: str) -> str:
    """Normalize URL for deduplication by stripping tracking parameters, lowercase domain, and trailing slashes."""
    parsed = urlparse(url.strip())
    netloc = parsed.netloc.lower()
    path = parsed.path.rstrip("/")
    # Filter out common tracking query parameters
    tracking_prefixes = ("utm_", "ref", "source", "fbclid", "gclid", "mc_")
    query_parts = []
    if parsed.query:
        for part in parsed.query.split("&"):
            k = part.split("=")[0].lower()
            if not any(k.startswith(p) for p in tracking_prefixes):
                query_parts.append(part)
    query_str = f"?{'&'.join(query_parts)}" if query_parts else ""
    return f"{parsed.scheme.lower()}://{netloc}{path}{query_str}"


class DigestPipeline:
    """End-to-end pipeline orchestrator for Daily Digest."""

    def __init__(self, config: AppConfig) -> None:
        """Initialize pipeline with application configuration."""
        self.config = config

    def run(
        self,
        hours: Optional[int] = None,
        output_format: Optional[str] = None,
        output_dir: Optional[Path] = None,
        enable_summarization: Optional[bool] = None,
        now: Optional[datetime] = None,
    ) -> PipelineResult:
        """Execute full daily digest pipeline."""
        active_hours = hours if hours is not None else self.config.time_window.default_hours
        active_format = (output_format or self.config.output.format).strip().lower()
        active_dir = output_dir or self.config.output.directory

        should_summarize = (
            enable_summarization
            if enable_summarization is not None
            else self.config.mistral.enabled
        )

        logger.info("Starting Daily Digest pipeline for last %d hours", active_hours)

        # 1. Collection
        collectors: List[BaseCollector] = create_collectors(self.config)
        all_raw_items: List[NewsItem] = []

        for collector in collectors:
            try:
                logger.info("Running collector: %s", collector.name)
                items = collector.collect(hours=active_hours, now=now)
                logger.info("Collector '%s' retrieved %d items", collector.name, len(items))
                all_raw_items.extend(items)
            except Exception as err:
                logger.error("Collector '%s' raised an unexpected error: %s", collector.name, err)

        # 2. Idempotent Deduplication (by normalized canonical URL and normalized title)
        seen_urls: Set[str] = set()
        seen_titles: Set[str] = set()
        deduped_items: List[NewsItem] = []
        for item in all_raw_items:
            norm_u = normalize_url(item.url)
            norm_t = " ".join(item.title.strip().lower().split())
            if norm_u in seen_urls:
                continue
            if norm_t and norm_t in seen_titles:
                continue
            seen_urls.add(norm_u)
            if norm_t:
                seen_titles.add(norm_t)
            deduped_items.append(item)

        logger.info(
            "Collected %d total items, %d unique after deduplication",
            len(all_raw_items),
            len(deduped_items),
        )

        # 3. Priority classification
        classifier = PriorityClassifier(self.config.priorities)
        classified_items = classifier.classify_items(deduped_items)

        # 4. Summarization
        if should_summarize and self.config.mistral.api_key:
            logger.info("Running Mistral AI summarization for %d items", len(classified_items))
            summarizer = MistralSummarizer(self.config.mistral)
            processed_items = summarizer.summarize_items(classified_items)
        else:
            if should_summarize and not self.config.mistral.api_key:
                logger.warning("Mistral summarization is enabled but no API key was provided; skipping.")
            processed_items = classified_items

        # 5. Sorting by priority and publication timestamp
        sorted_items = classifier.sort_items_by_priority(processed_items)

        # 6. Formatting & Export
        out_cfg = OutputConfig(
            format=active_format,
            directory=Path(active_dir),
            filename_pattern=self.config.output.filename_pattern,
        )
        exporter = DigestExporter(out_cfg)
        output_file = exporter.export_digest(
            items=sorted_items,
            time_window_hours=active_hours,
            format_name=active_format,
            generated_at=now,
        )

        logger.info("Daily Digest saved to %s", output_file)

        return PipelineResult(
            total_collected=len(all_raw_items),
            total_unique=len(sorted_items),
            time_window_hours=active_hours,
            output_path=output_file,
            items=sorted_items,
        )
