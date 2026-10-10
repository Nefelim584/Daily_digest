"""Unit tests for regex-based priority classifier."""

from datetime import datetime, timedelta, timezone

from daily_digest.classifier import PriorityClassifier
from daily_digest.config.models import PriorityRule
from daily_digest.models import NewsItem


def test_priority_classifier_rules():
    """Verify classifier assigns correct priority weights and category names."""
    rules = [
        PriorityRule(name="Major Releases", priority=100, regex=r"(?i)(release|introducing|gpt-\d|gemini)"),
        PriorityRule(name="Research", priority=50, regex=r"(?i)(paper|benchmark|reasoning)"),
        PriorityRule(name="General Updates", priority=10, regex=r".*"),
    ]
    classifier = PriorityClassifier(rules=rules)
    now = datetime.now(timezone.utc)

    item1 = NewsItem(title="Introducing GPT-5: A new frontier", url="http://a", published_at=now, source="OpenAI")
    item2 = NewsItem(title="A new reasoning benchmark for agents", url="http://b", published_at=now, source="arXiv")
    item3 = NewsItem(title="Weekly team notes and newsletter", url="http://c", published_at=now, source="Blog")

    c1 = classifier.classify_item(item1)
    c2 = classifier.classify_item(item2)
    c3 = classifier.classify_item(item3)

    assert c1.priority_score == 100
    assert c1.priority_group == "Major Releases"

    assert c2.priority_score == 50
    assert c2.priority_group == "Research"

    assert c3.priority_score == 10
    assert c3.priority_group == "General Updates"


def test_sort_items_by_priority():
    """Verify sorting items primarily by priority score and secondarily by publication time."""
    now = datetime(2026, 10, 11, 12, 0, 0, tzinfo=timezone.utc)
    item_low = NewsItem(title="Low", url="http://1", published_at=now, source="s", priority_score=10)
    item_high_older = NewsItem(title="High Old", url="http://2", published_at=now - timedelta(hours=5), source="s", priority_score=100)
    item_high_newer = NewsItem(title="High New", url="http://3", published_at=now - timedelta(hours=1), source="s", priority_score=100)

    classifier = PriorityClassifier()
    sorted_items = classifier.sort_items_by_priority([item_low, item_high_older, item_high_newer])

    assert [x.title for x in sorted_items] == ["High New", "High Old", "Low"]


def test_group_by_priority():
    """Verify grouping items by category."""
    rules = [
        PriorityRule(name="Major", priority=100, regex=r"Major"),
        PriorityRule(name="General", priority=10, regex=r".*"),
    ]
    classifier = PriorityClassifier(rules=rules)
    now = datetime.now(timezone.utc)

    item1 = classifier.classify_item(NewsItem(title="Major News", url="http://1", published_at=now, source="s"))
    item2 = classifier.classify_item(NewsItem(title="Other News", url="http://2", published_at=now, source="s"))

    grouped = classifier.group_by_priority([item1, item2])
    assert "Major" in grouped
    assert "General" in grouped
    assert len(grouped["Major"]) == 1
    assert len(grouped["General"]) == 1
