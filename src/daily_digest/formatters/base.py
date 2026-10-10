"""Base formatter interface."""

from abc import ABC, abstractmethod
from datetime import datetime, timezone
from typing import Optional, Sequence
from ..models import NewsItem


class BaseFormatter(ABC):
    """Abstract base class for all digest formatters."""

    @abstractmethod
    def format(
        self,
        items: Sequence[NewsItem],
        time_window_hours: int,
        generated_at: Optional[datetime] = None,
    ) -> str:
        """Format a collection of news items into a string digest."""
        pass

    def get_generated_time(self, generated_at: Optional[datetime] = None) -> datetime:
        """Return timezone-aware UTC datetime for digest generation time."""
        if generated_at is None:
            return datetime.now(timezone.utc)
        if generated_at.tzinfo is None:
            return generated_at.replace(tzinfo=timezone.utc)
        return generated_at.astimezone(timezone.utc)
