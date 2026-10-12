"""Unit tests for configuration models and loader."""

import pytest
from pathlib import Path

from daily_digest.config.loader import load_config, ConfigurationError
from daily_digest.config.models import (
    AppConfig,
    CleaningConfig,
    OutputConfig,
    SourceConfig,
    TimeWindowConfig,
)


def test_load_real_config():
    """Verify that the repository's config/config.yaml loads and validates successfully."""
    config_path = Path("config/config.yaml")
    assert config_path.exists()
    app_config = load_config(config_path)

    assert isinstance(app_config, AppConfig)
    assert app_config.time_window.default_hours > 0
    assert app_config.output.format in {"markdown", "text"}
    assert len(app_config.sources) > 0
    assert len(app_config.priorities) > 0


def test_load_missing_config(tmp_path: Path):
    """Verify that a non-existent file path raises ConfigurationError."""
    missing = tmp_path / "does_not_exist.yaml"
    with pytest.raises(ConfigurationError, match="Configuration file not found"):
        load_config(missing)


def test_load_invalid_yaml(tmp_path: Path):
    """Verify that malformed YAML content raises ConfigurationError."""
    bad_yaml = tmp_path / "bad.yaml"
    bad_yaml.write_text("invalid: [yaml: broken", encoding="utf-8")
    with pytest.raises(ConfigurationError, match="Failed to parse YAML file"):
        load_config(bad_yaml)


def test_output_config_format_validation():
    """Verify format validator accepts md/markdown and txt/text and normalizes them."""
    out_md = OutputConfig(format="md")
    assert out_md.format == "markdown"

    out_txt = OutputConfig(format="txt")
    assert out_txt.format == "text"

    with pytest.raises(ValueError, match="Unsupported output format"):
        OutputConfig(format="pdf")


def test_cleaning_config_defaults():
    """Verify CleaningConfig defaults are properly set."""
    cfg = CleaningConfig()
    assert cfg.max_characters == 1500
    assert cfg.strip_html_tags is True
    assert cfg.strip_images is True
    assert cfg.strip_links is True
    assert cfg.strip_scripts is True


def test_source_config_creation():
    """Verify SourceConfig creation and default values."""
    src = SourceConfig(name="Test Source", type="rss", url="https://example.com/rss")
    assert src.name == "Test Source"
    assert src.type == "rss"
    assert src.enabled is True
    assert src.max_results == 50


def test_server_config_defaults():
    """Verify ServerConfig defaults to 0.0.0.0 and port 8000 for Docker network openness."""
    from daily_digest.config.models import ServerConfig
    server_cfg = ServerConfig()
    assert server_cfg.host == "0.0.0.0"
    assert server_cfg.port == 8000


def test_config_env_overrides(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    """Verify 12-Factor environment variables override config values."""
    custom_dir = tmp_path / "env_digests"
    monkeypatch.setenv("DIGEST_OUTPUT_DIR", str(custom_dir))
    monkeypatch.setenv("DIGEST_OUTPUT_FORMAT", "text")
    monkeypatch.setenv("HOST", "0.0.0.0")
    monkeypatch.setenv("PORT", "9999")
    monkeypatch.setenv("DIGEST_DEFAULT_HOURS", "48")
    monkeypatch.setenv("RSSHUB_BASE_URL", "https://custom-rsshub.local")
    monkeypatch.setenv("MISTRAL_API_KEY", "env_test_key_123")

    cfg = load_config(Path("config/config.yaml"))
    assert cfg.output.directory == custom_dir
    assert cfg.output.format == "text"
    assert cfg.server.host == "0.0.0.0"
    assert cfg.server.port == 9999
    assert cfg.time_window.default_hours == 48
    assert cfg.rsshub.base_url == "https://custom-rsshub.local"
    assert cfg.mistral.api_key == "env_test_key_123"

