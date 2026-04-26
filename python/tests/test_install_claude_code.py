"""v0.4.0-rc3 (Task Q2) — Claude Code MCP installer tests."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from mnemo.install_claude_code import install_claude_code_mcp


def _read(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def test_install_into_empty_config(tmp_path: Path) -> None:
    cfg = tmp_path / "claude.json"
    result = install_claude_code_mcp(
        config_path=cfg,
        db_path="agent.db",
        agent_id="my-agent",
    )
    assert result.action == "added"
    data = _read(cfg)
    assert "mcpServers" in data
    entry = data["mcpServers"]["mnemo"]
    assert entry["args"][:4] == ["--db-path", "agent.db", "--agent-id", "my-agent"]
    assert entry["command"]


def test_install_preserves_other_servers(tmp_path: Path) -> None:
    cfg = tmp_path / "claude.json"
    cfg.write_text(
        json.dumps({"mcpServers": {"other": {"command": "x", "args": []}}})
    )
    install_claude_code_mcp(config_path=cfg)
    data = _read(cfg)
    assert "other" in data["mcpServers"]
    assert "mnemo" in data["mcpServers"]


def test_idempotent_install(tmp_path: Path) -> None:
    cfg = tmp_path / "claude.json"
    install_claude_code_mcp(config_path=cfg, db_path="agent.db")
    second = install_claude_code_mcp(config_path=cfg, db_path="agent.db")
    assert second.action == "unchanged"


def test_re_install_with_new_db_path_updates(tmp_path: Path) -> None:
    cfg = tmp_path / "claude.json"
    install_claude_code_mcp(config_path=cfg, db_path="old.db")
    second = install_claude_code_mcp(config_path=cfg, db_path="new.db")
    assert second.action == "updated"
    entry = _read(cfg)["mcpServers"]["mnemo"]
    assert "new.db" in entry["args"]


def test_hardened_manifest_switches_to_mcp_server_subcommand(tmp_path: Path) -> None:
    cfg = tmp_path / "claude.json"
    install_claude_code_mcp(
        config_path=cfg,
        hardened_manifest="/etc/mnemo/manifest.toml",
    )
    entry = _read(cfg)["mcpServers"]["mnemo"]
    assert entry["args"] == ["mcp-server", "--manifest", "/etc/mnemo/manifest.toml"]
    # Hardened mode never carries inherited env in the registered entry —
    # secrets must come from the manifest's keystore_path, not Claude.
    assert "env" not in entry


def test_refuses_to_overwrite_malformed_config(tmp_path: Path) -> None:
    cfg = tmp_path / "claude.json"
    cfg.write_text("{ this is not json")
    with pytest.raises(RuntimeError):
        install_claude_code_mcp(config_path=cfg)
