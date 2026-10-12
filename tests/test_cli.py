"""Unit tests for CLI entrypoint."""

from pathlib import Path
from unittest.mock import patch, MagicMock

from daily_digest.cli import create_parser, main
from daily_digest.pipeline import PipelineResult


def test_cli_parser_defaults():
    """Verify default parser values."""
    parser = create_parser()
    args = parser.parse_args([])

    assert args.config == Path("config/config.yaml")
    assert args.hours is None
    assert args.format is None
    assert args.output is None
    assert args.no_summary is False
    assert args.verbose is False


def test_cli_parser_overrides():
    """Verify command-line argument overrides."""
    parser = create_parser()
    args = parser.parse_args([
        "--config", "custom.yaml",
        "--hours", "48",
        "--format", "text",
        "--output", "custom_dir",
        "--no-summary",
        "--verbose",
    ])

    assert args.config == Path("custom.yaml")
    assert args.hours == 48
    assert args.format == "text"
    assert args.output == Path("custom_dir")
    assert args.no_summary is True
    assert args.verbose is True


def test_cli_main_missing_config():
    """Verify main returns 1 on non-existent config path."""
    code = main(["--config", "non_existent_file_xyz.yaml"])
    assert code == 1


def test_cli_main_success(tmp_path: Path):
    """Verify main returns 0 on successful pipeline run."""
    mock_res = PipelineResult(
        total_collected=5,
        total_unique=5,
        time_window_hours=24,
        output_path=tmp_path / "digest.md",
    )

    with patch("daily_digest.cli.load_config"):
        with patch.object(
            __import__("daily_digest.pipeline", fromlist=["DigestPipeline"]).DigestPipeline,
            "run",
            return_value=mock_res,
        ):
            code = main(["--hours", "24"])
            assert code == 0


def test_cli_parser_server_options():
    """Verify --serve, --host, and --port CLI options."""
    parser = create_parser()
    args = parser.parse_args(["--serve", "--host", "0.0.0.0", "-p", "9000"])
    assert args.serve is True
    assert args.host == "0.0.0.0"
    assert args.port == 9000


def test_cli_main_serve():
    """Verify main dispatches to uvicorn when --serve is passed."""
    with patch("daily_digest.cli.load_config"):
        with patch("uvicorn.run") as mock_uvicorn:
            code = main(["--serve", "--host", "0.0.0.0", "--port", "8000"])
            assert code == 0
            assert mock_uvicorn.called

