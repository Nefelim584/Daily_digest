"""Regex-based title classifier to assign priority weights and category groups."""

import logging
import re
from typing import Dict, Iterable, List, Optional, Tuple
from ..config.models import PriorityRule
from ..filters import normalize_datetime
from ..models import NewsItem

logger = logging.getLogger(__name__)


class PriorityClassifier:
    """Classifies news items into priority tiers using regular expressions against titles."""

    def __init__(self, rules: Optional[List[PriorityRule]] = None) -> None:
        """Initialize classifier with configured priority rules sorted by weight descending."""
        self.rules = sorted(rules or [], key=lambda r: r.priority, reverse=True)
        self._compiled_rules: List[Tuple[PriorityRule, re.Pattern[str]]] = []

        for rule in self.rules:
            try:
                pattern = re.compile(rule.regex)
                self._compiled_rules.append((rule, pattern))
            except re.error as err:
                logger.error("Invalid regex in priority rule '%s' (%s): %s", rule.name, rule.regex, err)

    def classify_item(self, item: NewsItem) -> NewsItem:
        """Evaluate item title against priority rules and assign priority score and group name."""
        for rule, pattern in self._compiled_rules:
            if pattern.search(item.title):
                item.priority_score = rule.priority
                item.priority_group = rule.name
                return item

        # Default fallback if no rules match
        item.priority_score = 10
        item.priority_group = "General News & Updates"
        return item

    def classify_items(self, items: Iterable[NewsItem]) -> List[NewsItem]:
        """Classify a collection of items."""
        return [self.classify_item(item) for item in items]

    def sort_items_by_priority(
        self,
        items: Iterable[NewsItem],
        reverse_time: bool = True,
    ) -> List[NewsItem]:
        """Sort items primarily by priority score descending, and secondarily by publication time."""
        return sorted(
            items,
            key=lambda it: (it.priority_score, normalize_datetime(it.published_at) if reverse_time else 0),
            reverse=True,
        )

    def group_by_priority(self, items: Iterable[NewsItem]) -> Dict[str, List[NewsItem]]:
        """Group classified items by priority category preserving priority order."""
        grouped: Dict[str, List[NewsItem]] = {}
        for rule in self.rules:
            grouped[rule.name] = []
        if "General News & Updates" not in grouped:
            grouped["General News & Updates"] = []

        for item in items:
            group = item.priority_group or "General News & Updates"
            if group not in grouped:
                grouped[group] = []
            grouped[group].append(item)

        # Remove empty groups
        return {k: v for k, v in grouped.items() if v}
