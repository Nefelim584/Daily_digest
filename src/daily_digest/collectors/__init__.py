"""Sources collector adapters package (RSS, arXiv, HF, X)."""

from .base import BaseCollector
from .arxiv import ArxivCollector
from .huggingface import HuggingFacePapersCollector
from .rss import RSSCollector
from .rsshub import RSSHubCollector
from .factory import create_collector, create_collectors

__all__ = [
    "BaseCollector",
    "ArxivCollector",
    "HuggingFacePapersCollector",
    "RSSCollector",
    "RSSHubCollector",
    "create_collector",
    "create_collectors",
]
