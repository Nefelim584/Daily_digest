"""Factory functions to instantiate collectors from configuration."""

import logging
from typing import List, Optional

from ..config.models import AppConfig, SourceConfig
from ..summarization.search_fallback import MistralSearchFallback
from .arxiv import ArxivCollector
from .base import BaseCollector
from .huggingface import HuggingFacePapersCollector
from .rss import RSSCollector
from .rsshub import RSSHubCollector

logger = logging.getLogger(__name__)


def create_collector(
    source: SourceConfig,
    app_config: AppConfig,
    search_fallback: Optional[MistralSearchFallback] = None,
) -> Optional[BaseCollector]:
    """Create appropriate collector instance based on source configuration."""
    if not source.enabled:
        return None

    collector_type = source.type.strip().lower()

    if collector_type == "arxiv_api":
        return ArxivCollector(
            name=source.name,
            categories=source.categories or None,
            max_results=source.max_results,
            endpoint=source.endpoint or ArxivCollector.DEFAULT_ENDPOINT,
            cleaning_config=app_config.cleaning,
        )

    if collector_type in {"huggingface_papers_api", "huggingface_daily_papers"}:
        return HuggingFacePapersCollector(
            name=source.name,
            endpoint=source.endpoint or HuggingFacePapersCollector.DEFAULT_ENDPOINT,
            cleaning_config=app_config.cleaning,
        )

    if collector_type == "rss":
        if not source.url:
            logger.warning("Source '%s' has type 'rss' but no URL specified; skipping.", source.name)
            return None
        return RSSCollector(
            name=source.name,
            url=source.url,
            cleaning_config=app_config.cleaning,
            search_fallback=search_fallback,
        )

    if collector_type == "rsshub":
        return RSSHubCollector(
            name=source.name,
            routes=source.routes,
            base_url=app_config.rsshub.base_url,
            access_key=app_config.rsshub.access_key,
            cleaning_config=app_config.cleaning,
            search_fallback=search_fallback,
        )

    logger.warning("Unknown collector type '%s' for source '%s'", source.type, source.name)
    return None


def create_collectors(app_config: AppConfig) -> List[BaseCollector]:
    """Instantiate all enabled collectors defined in app configuration."""
    search_fallback: Optional[MistralSearchFallback] = None
    if app_config.mistral.enabled and app_config.mistral.web_search_fallback and app_config.mistral.api_key:
        search_fallback = MistralSearchFallback(app_config.mistral)

    collectors: List[BaseCollector] = []
    for source_cfg in app_config.sources:
        collector = create_collector(source_cfg, app_config, search_fallback=search_fallback)
        if collector is not None:
            collectors.append(collector)
    return collectors
