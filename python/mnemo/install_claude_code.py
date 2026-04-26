"""v0.4.0-rc3 (Task Q2) — Claude Code MCP installer.

Idempotently registers Mnemo as an MCP server in Claude Code's
``~/.claude.json`` config. Safe to re-run: an existing ``mnemo`` entry
is updated in place; other entries are left alone.

Usage::

    python -m mnemo install claude-code
    python -m mnemo install claude-code --db-path ~/data/agent.db --hardened ~/etc/mnemo.toml

The ``--hardened`` flag points at a TOML manifest and uses the
``mnemo mcp-server`` subcommand introduced in v0.4.0-rc3 (Task B2),
so the registered launcher refuses inherited secrets / argv injection
/ untrusted parents at startup.
"""

from __future__ import annotations

import json
import os
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any


def _default_config_path() -> Path:
    """Resolve Claude Code's MCP config file.

    Honors ``CLAUDE_CONFIG_PATH`` for tests / non-default installs;
    falls back to ``~/.claude.json`` which is what the Claude Code
    distribution writes by default.
    """
    override = os.environ.get("CLAUDE_CONFIG_PATH")
    if override:
        return Path(override)
    return Path.home() / ".claude.json"


@dataclass
class InstallResult:
    config_path: Path
    server_name: str
    action: str  # "added" | "updated" | "unchanged"
    entry: dict[str, Any]


def install_claude_code_mcp(
    *,
    server_name: str = "mnemo",
    db_path: str = "mnemo.db",
    agent_id: str = "default",
    openai_api_key: str | None = None,
    hardened_manifest: str | None = None,
    binary: str | None = None,
    config_path: Path | None = None,
) -> InstallResult:
    """Add or update the Mnemo MCP server entry in Claude Code's config.

    Parameters
    ----------
    server_name:
        Key under ``mcpServers`` in the Claude config. Defaults to
        ``"mnemo"``; override when registering multiple Mnemo
        deployments.
    db_path:
        Database file the launcher should open. Defaults to
        ``mnemo.db`` in the current directory.
    agent_id:
        Default ``--agent-id`` for recall/remember calls.
    openai_api_key:
        OpenAI key for embeddings. ``None`` falls back to
        ``$OPENAI_API_KEY`` at run-time, which is the right move when
        registering on a multi-user box.
    hardened_manifest:
        Path to a B2 manifest. If set, the registered launcher uses
        ``mnemo mcp-server --manifest <path>`` and inherits the
        safe-spawn gauntlet.
    binary:
        Override the launcher binary. Defaults to the first ``mnemo``
        on ``$PATH``; falls back to the literal string ``"mnemo"``
        which Claude Code resolves on its own when launching.
    config_path:
        Override the config file. Honors ``CLAUDE_CONFIG_PATH``
        otherwise, then ``~/.claude.json``.
    """
    cfg_path = config_path or _default_config_path()
    cfg_path.parent.mkdir(parents=True, exist_ok=True)

    # Load existing config (or start fresh) — preserve every other
    # MCP entry the user has registered.
    if cfg_path.exists():
        with cfg_path.open("r", encoding="utf-8") as f:
            try:
                config = json.load(f)
            except json.JSONDecodeError as e:
                raise RuntimeError(
                    f"refusing to overwrite malformed config at {cfg_path}: {e}"
                ) from e
    else:
        config = {}
    if not isinstance(config, dict):
        raise RuntimeError(
            f"refusing to overwrite non-object config at {cfg_path}"
        )

    servers = config.setdefault("mcpServers", {})
    if not isinstance(servers, dict):
        raise RuntimeError("mcpServers in config is not a JSON object")

    launcher = binary or shutil.which("mnemo") or "mnemo"
    entry = _build_entry(
        launcher=launcher,
        db_path=db_path,
        agent_id=agent_id,
        openai_api_key=openai_api_key,
        hardened_manifest=hardened_manifest,
    )

    prior = servers.get(server_name)
    if prior == entry:
        return InstallResult(
            config_path=cfg_path,
            server_name=server_name,
            action="unchanged",
            entry=entry,
        )
    action = "updated" if prior is not None else "added"
    servers[server_name] = entry

    # Write atomically via a tmp + rename so a partial write never
    # corrupts the config.
    tmp = cfg_path.with_suffix(cfg_path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8") as f:
        json.dump(config, f, indent=2)
        f.write("\n")
    os.replace(tmp, cfg_path)

    return InstallResult(
        config_path=cfg_path,
        server_name=server_name,
        action=action,
        entry=entry,
    )


def _build_entry(
    *,
    launcher: str,
    db_path: str,
    agent_id: str,
    openai_api_key: str | None,
    hardened_manifest: str | None,
) -> dict[str, Any]:
    if hardened_manifest is not None:
        # B2 hardened mode: every privileged knob comes from the
        # manifest, so argv stays minimal.
        args: list[str] = ["mcp-server", "--manifest", hardened_manifest]
        env: dict[str, str] = {}
    else:
        args = ["--db-path", db_path, "--agent-id", agent_id]
        env = {}
        if openai_api_key:
            env["OPENAI_API_KEY"] = openai_api_key

    entry: dict[str, Any] = {"command": launcher, "args": args}
    if env:
        entry["env"] = env
    return entry


__all__ = [
    "InstallResult",
    "install_claude_code_mcp",
]
