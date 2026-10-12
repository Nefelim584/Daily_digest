"""Pydantic models representing configuration for Daily Digest."""

from pathlib import Path
from typing import List, Optional
from pydantic import BaseModel, Field, field_validator


class TimeWindowConfig(BaseModel):
    """Configuration for collection time window."""
    default_hours: int = Field(default=24, ge=1, description="Default time window in hours")


class OutputConfig(BaseModel):
    """Configuration for digest output generation."""
    format: str = Field(default="markdown", description="Format: markdown or text")
    directory: Path = Field(default=Path("data/digests"), description="Destination directory")
    filename_pattern: str = Field(
        default="digest_{date}.{ext}",
        description="Filename template pattern for generated digests",
    )

    @field_validator("format")
    @classmethod
    def validate_format(cls, value: str) -> str:
        cleaned = value.strip().lower()
        if cleaned not in {"markdown", "text", "md", "txt"}:
            raise ValueError(f"Unsupported output format '{value}'. Choose 'markdown' or 'text'.")
        return "markdown" if cleaned in {"markdown", "md"} else "text"


class CleaningConfig(BaseModel):
    """Configuration for regex-based text cleaning and truncation."""
    max_characters: int = Field(default=1500, ge=100, description="Max snippet length")
    strip_html_tags: bool = Field(default=True, description="Remove general HTML tags")
    strip_images: bool = Field(default=True, description="Remove img and media tags")
    strip_links: bool = Field(default=True, description="Remove link tags and URLs")
    strip_scripts: bool = Field(default=True, description="Remove script and style blocks")


class RSSHubConfig(BaseModel):
    """Configuration for RSSHub integration."""
    base_url: str = Field(default="https://rsshub.app", description="RSSHub instance base URL")
    timeout_seconds: int = Field(default=15, ge=1, description="Request timeout in seconds")
    access_key: str = Field(default="", description="Optional access token/key for RSSHub")

    @field_validator("base_url")
    @classmethod
    def sanitize_base_url(cls, value: str) -> str:
        return value.strip().rstrip("/")


class MistralConfig(BaseModel):
    """Configuration for Mistral AI summarization."""
    enabled: bool = Field(default=True, description="Enable Mistral AI summarization")
    env_file: str = Field(default=".env", description="Path to .env file")
    api_key_env: str = Field(default="MISTRAL_API_KEY", description="Environment variable name for API key")
    api_key: Optional[str] = Field(default=None, description="Resolved API key value at runtime")
    model: str = Field(default="mistral-small-latest", description="Mistral model name")
    temperature: float = Field(default=0.2, ge=0.0, le=1.0, description="Sampling temperature")
    max_tokens: int = Field(default=300, ge=50, description="Max response tokens per summary")
    prompt_template: str = Field(
        default="Summarize the news item concisely in 2-3 bullet points focusing on key contributions.",
        description="Prompt template used for summarization",
    )


class PriorityRule(BaseModel):
    """Regex-based rule for assigning category priorities."""
    name: str = Field(description="Human readable category name")
    priority: int = Field(description="Priority weight (higher values get placed first)")
    regex: str = Field(description="Regular expression to match against news titles")


class SourceConfig(BaseModel):
    """Configuration for an individual news source."""
    name: str = Field(description="Name of the source")
    type: str = Field(description="Collector adapter type, e.g. arxiv_api, huggingface_papers_api, rss, rsshub")
    enabled: bool = Field(default=True, description="Whether this source is active")
    url: Optional[str] = Field(default=None, description="Direct RSS/Atom feed URL")
    endpoint: Optional[str] = Field(default=None, description="API endpoint URL")
    routes: List[str] = Field(default_factory=list, description="RSSHub route list")
    categories: List[str] = Field(default_factory=list, description="arXiv category list")
    max_results: int = Field(default=50, ge=1, description="Max items to retrieve from this source")


class ServerConfig(BaseModel):
    """Configuration for HTTP server."""
    host: str = Field(default="0.0.0.0", description="Network host interface to listen on")
    port: int = Field(default=8000, ge=1, le=65535, description="Port to listen on")


class AppConfig(BaseModel):
    """Root application configuration."""
    server: ServerConfig = Field(default_factory=ServerConfig)
    time_window: TimeWindowConfig = Field(default_factory=TimeWindowConfig)
    output: OutputConfig = Field(default_factory=OutputConfig)
    cleaning: CleaningConfig = Field(default_factory=CleaningConfig)
    rsshub: RSSHubConfig = Field(default_factory=RSSHubConfig)
    mistral: MistralConfig = Field(default_factory=MistralConfig)
    priorities: List[PriorityRule] = Field(default_factory=list)
    sources: List[SourceConfig] = Field(default_factory=list)

