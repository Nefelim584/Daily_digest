"""Command-line interface entrypoint for Daily Digest."""

import argparse
import logging
import sys
from pathlib import Path
from typing import List, Optional

from .config.loader import load_config, ConfigurationError
from .pipeline import DigestPipeline


def create_parser() -> argparse.ArgumentParser:
    """Create command-line argument parser."""
    parser = argparse.ArgumentParser(
        prog="daily-digest",
        description="Daily AI & Tech News Digest Collector - Aggregates, prioritizes, and summarizes AI research.",
    )
    parser.add_argument(
        "-c",
        "--config",
        type=Path,
        default=Path("config/config.yaml"),
        help="Path to YAML configuration file (default: config/config.yaml)",
    )
    parser.add_argument(
        "-H",
        "--hours",
        type=int,
        default=None,
        help="Collection time window in hours (overrides config default)",
    )
    parser.add_argument(
        "-f",
        "--format",
        type=str,
        choices=["markdown", "text", "md", "txt"],
        default=None,
        help="Digest output format: markdown or text (overrides config default)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help="Destination directory for generated digest files (overrides config default)",
    )
    parser.add_argument(
        "--no-summary",
        action="store_true",
        default=False,
        help="Disable Mistral AI summarization even if enabled in config",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        default=False,
        help="Enable verbose debug logging output",
    )
    return parser


def main(argv: Optional[List[str]] = None) -> int:
    """Parse arguments and execute digest pipeline."""
    parser = create_parser()
    args = parser.parse_args(argv)

    log_level = logging.DEBUG if args.verbose else logging.INFO
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s [%(levelname)s] %(message)s",
        datefmt="%H:%M:%S",
    )

    try:
        config = load_config(args.config)
    except ConfigurationError as err:
        print(f"Error: {err}", file=sys.stderr)
        return 1

    pipeline = DigestPipeline(config)

    try:
        result = pipeline.run(
            hours=args.hours,
            output_format=args.format,
            output_dir=args.output,
            enable_summarization=False if args.no_summary else None,
        )

        print("\nDaily Digest Generated Successfully:")
        print(f" - Time window:  {result.time_window_hours} hours")
        print(f" - Total items:  {result.total_collected} retrieved ({result.total_unique} unique)")
        print(f" - Digest file:  {result.output_path.resolve()}\n")
        return 0

    except Exception as err:
        logging.exception("Pipeline execution failed: %s", err)
        return 1


if __name__ == "__main__":
    sys.exit(main())
