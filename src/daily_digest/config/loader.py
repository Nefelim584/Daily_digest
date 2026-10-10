"""Configuration loader and validator for Daily Digest."""

import os
from pathlib import Path
from typing import Union
import yaml
from dotenv import load_dotenv

from .models import AppConfig


class ConfigurationError(Exception):
    """Raised when configuration file is invalid or missing."""
    pass


def load_config(config_path: Union[str, Path] = "config/config.yaml") -> AppConfig:
    """Load, validate, and enrich application configuration from a YAML file.

    Args:
        config_path: Path to the YAML configuration file.

    Returns:
        Validated AppConfig instance with resolved environment secrets.

    Raises:
        ConfigurationError: If the file does not exist or contains invalid structure.
    """
    path = Path(config_path)
    if not path.is_file():
        raise ConfigurationError(f"Configuration file not found at: {path.resolve()}")

    try:
        with open(path, "r", encoding="utf-8") as f:
            raw_data = yaml.safe_load(f)
    except yaml.YAMLError as exc:
        raise ConfigurationError(f"Failed to parse YAML file at {path}: {exc}") from exc

    if not isinstance(raw_data, dict):
        raise ConfigurationError(f"Configuration file must contain a top-level dictionary, got: {type(raw_data).__name__}")

    try:
        config = AppConfig.model_validate(raw_data)
    except Exception as exc:
        raise ConfigurationError(f"Configuration validation failed: {exc}") from exc

    # Load environment variables (e.g. from .env file specified in mistral config)
    env_file_path = Path(config.mistral.env_file)
    if env_file_path.is_file():
        load_dotenv(dotenv_path=env_file_path)
    else:
        # Fall back to default .env in current directory or parent
        load_dotenv()

    # Resolve Mistral API key from environment
    api_key_env_var = config.mistral.api_key_env
    resolved_key = os.getenv(api_key_env_var)
    if resolved_key:
        config.mistral.api_key = resolved_key.strip()
    elif config.mistral.enabled:
        # Note: Do not hard-crash on import/load if key is missing,
        # but mark key as None so warning/degradation can occur during Phase 4.
        config.mistral.api_key = None

    return config
