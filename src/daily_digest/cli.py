"""Command-line interface entrypoint for Daily Digest."""

import sys
from pathlib import Path
from .config.loader import load_config, ConfigurationError


def main() -> int:
    """Run Daily Digest CLI application."""
    config_path = Path("config/config.yaml")
    try:
        config = load_config(config_path)
        print("Daily Digest Configuration loaded successfully:")
        print(f" - Time window: {config.time_window.default_hours} hours")
        print(f" - Output format: {config.output.format} -> {config.output.directory}")
        print(f" - Active sources: {len([s for s in config.sources if s.enabled])} configured")
        print(f" - Mistral summarization: {'Enabled' if config.mistral.enabled else 'Disabled'}")
        return 0
    except ConfigurationError as err:
        print(f"Configuration Error: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
