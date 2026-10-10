"""Data model representing a single news or paper item."""

from datetime import datetime
from typing import Optional
from pydantic import BaseModel, Field


class NewsItem(BaseModel):
    """Standardized representation of a news item across all sources."""

    title: str = Field(..., description="Headline or paper title")
    url: str = Field(..., description="Link to source article or paper")
    published_at: datetime = Field(..., description="Publication timestamp")
    source: str = Field(..., description="Source name (e.g., 'OpenAI Blog', 'arXiv')")
    raw_content: Optional[str] = Field(
        default=None,
        description="Original uncleaned content or abstract from source",
    )
    clean_snippet: Optional[str] = Field(
        default=None,
        description="Sanitized and truncated text preview (up to configured max characters)",
    )
    summary: Optional[str] = Field(
        default=None,
        description="AI-generated summary of the item",
    )
    priority_score: int = Field(
        default=10,
        description="Computed priority score for ordering (higher is more important)",
    )
    priority_group: Optional[str] = Field(
        default=None,
        description="Name of the matching priority category",
    )
