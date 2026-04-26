"""`python -m mnemo` — availability + Claude Code installer entry point.

Examples::

    python -m mnemo doctor
    python -m mnemo install claude-code
    python -m mnemo install claude-code --hardened ~/etc/mnemo.toml
"""

from __future__ import annotations

import argparse
import sys

from mnemo.availability import doctor
from mnemo.install_claude_code import install_claude_code_mcp


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m mnemo")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser(
        "doctor",
        help="Print an availability report for the native extension + adapters.",
    )

    install = sub.add_parser(
        "install", help="Register Mnemo with a host (e.g. Claude Code)."
    )
    install_sub = install.add_subparsers(dest="target", required=True)
    cc = install_sub.add_parser(
        "claude-code",
        help="Add/update the Mnemo MCP server entry in ~/.claude.json.",
    )
    cc.add_argument("--server-name", default="mnemo")
    cc.add_argument("--db-path", default="mnemo.db")
    cc.add_argument("--agent-id", default="default")
    cc.add_argument(
        "--openai-api-key",
        default=None,
        help="OpenAI key for embeddings; defaults to $OPENAI_API_KEY at run-time.",
    )
    cc.add_argument(
        "--hardened",
        dest="hardened_manifest",
        default=None,
        help="Path to a B2 TOML manifest. Switches the launcher to "
        "'mnemo mcp-server --manifest <path>' (refuses inherited secrets, "
        "JSON-injection argv, untrusted parents).",
    )
    cc.add_argument("--binary", default=None)
    cc.add_argument(
        "--config-path",
        default=None,
        help="Override Claude config path (honors $CLAUDE_CONFIG_PATH otherwise).",
    )

    args = parser.parse_args(argv)
    if args.command == "doctor":
        return doctor()
    if args.command == "install" and args.target == "claude-code":
        from pathlib import Path

        result = install_claude_code_mcp(
            server_name=args.server_name,
            db_path=args.db_path,
            agent_id=args.agent_id,
            openai_api_key=args.openai_api_key,
            hardened_manifest=args.hardened_manifest,
            binary=args.binary,
            config_path=Path(args.config_path) if args.config_path else None,
        )
        action_word = {
            "added": "Added",
            "updated": "Updated",
            "unchanged": "Already up-to-date:",
        }[result.action]
        print(
            f"{action_word} MCP server {result.server_name!r} in {result.config_path}"
        )
        return 0
    parser.print_help()
    return 2


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
