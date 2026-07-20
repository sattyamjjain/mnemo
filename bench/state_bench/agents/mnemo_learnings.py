"""mnemo-backed procedural-learnings store for STATE-Bench (no `state_bench` dep).

Pure mnemo logic — extraction, ingest, retrieval — so it is unit/smoke-testable
without STATE-Bench installed. `mnemo_memory_agent.py` wraps this in a
`StateBenchAgent` subclass; `build_learnings.py` drives the offline extraction.

Public API only: mnemo's `MnemoClient` (`remember` / `recall`). No `mnemo-core`
change.
"""

from __future__ import annotations

import json
import os
import threading
from pathlib import Path
from typing import Any

try:
    from mnemo import MnemoClient  # mnemo public Python SDK (PyO3)
except Exception as exc:  # pragma: no cover
    MnemoClient = None  # type: ignore[assignment]
    _MNEMO_IMPORT_ERROR: Exception | None = exc
else:
    _MNEMO_IMPORT_ERROR = None


def db_dir() -> Path:
    return Path(os.environ.get("MNEMO_STATEBENCH_DB_DIR", "outputs/mnemo_stores")).resolve()


def current_domain() -> str:
    return os.environ.get("MNEMO_STATEBENCH_DOMAIN", "customer_support")


def embed_key() -> str | None:
    return os.environ.get("MNEMO_STATEBENCH_EMBED_KEY") or os.environ.get("OPENAI_API_KEY")


def db_path_for(domain: str) -> Path:
    return db_dir() / f"{domain}.mnemo.db"


def recall_strategy() -> str:
    """Hybrid RRF (semantic + BM25) with a real embedder; lexical BM25 without.

    mnemo's semantic recall hard-errors under a no-op embedder (>= v0.5.13), so
    the no-key path must use lexical.
    """
    return "auto" if embed_key() else "lexical"


def make_client(db_path: Path) -> Any:
    if MnemoClient is None:
        raise RuntimeError(
            "mnemo is not importable — build the SDK first "
            "(`maturin develop -m python/Cargo.toml`). "
            f"Import error: {_MNEMO_IMPORT_ERROR}"
        )
    db_path.parent.mkdir(parents=True, exist_ok=True)
    key = embed_key()
    if key:
        return MnemoClient(
            db_path=str(db_path),
            agent_id="state-bench",
            openai_api_key=key,
            embedding_model=os.environ.get("MNEMO_STATEBENCH_EMBED_MODEL", "text-embedding-3-small"),
            dimensions=int(os.environ.get("MNEMO_STATEBENCH_EMBED_DIM", "1536")),
            with_full_text=True,
        )
    return MnemoClient(
        db_path=str(db_path),
        agent_id="state-bench",
        with_full_text=True,
        with_noop_embedding=True,
    )


_CLIENTS: dict[str, Any] = {}
_CLIENTS_LOCK = threading.Lock()


def client_for(domain: str) -> Any:
    """One client per (process, db) — avoids DuckDB write-lock contention under
    thread-pool workers; the engine serialises its own DB access."""
    key = str(db_path_for(domain))
    with _CLIENTS_LOCK:
        client = _CLIENTS.get(key)
        if client is None:
            client = make_client(db_path_for(domain))
            _CLIENTS[key] = client
        return client


def extract_learning(trajectory: dict[str, Any], task_id: str, domain: str) -> str:
    """One deterministic *procedural* note per train trajectory (no LLM).

    Task cue (first user turn) + the ordered tool sequence the successful
    trajectory used + the final assistant resolution.
    """
    convo = trajectory.get("conversation", [])
    first_user = next((m.get("content", "") for m in convo if m.get("role") == "user"), "")
    tool_seq: list[str] = []
    for m in convo:
        for tc in m.get("tool_calls", []) or []:
            name = tc.get("name")
            if isinstance(name, str):
                tool_seq.append(name)
    final_assistant = ""
    for m in reversed(convo):
        if m.get("role") == "assistant" and m.get("content"):
            final_assistant = m["content"]
            break

    cue = " ".join(first_user.split())[:280]
    resolution = " ".join(final_assistant.split())[:280]
    tools = " -> ".join(tool_seq) if tool_seq else "(no tool calls)"
    return f"[{domain}] Task {task_id}. Request: {cue} Procedure (tool order): {tools}. Resolution: {resolution}"


def build_learnings(train_dir: str | Path, domain: str, db_path: str | Path | None = None) -> int:
    """Extract one learning per `train_dir/*.json` and `remember` into mnemo.

    The mnemo DuckDB file *is* the learnings artifact. Returns the count written.
    """
    train_dir = Path(train_dir)
    db = Path(db_path) if db_path is not None else db_path_for(domain)
    client = make_client(db)
    count = 0
    for traj_file in sorted(train_dir.glob("*.json")):
        trajectory = json.loads(traj_file.read_text())
        client.remember(
            extract_learning(trajectory, traj_file.stem, domain),
            memory_type="procedural",
            tags=[f"domain:{domain}", "state-bench-learning"],
        )
        count += 1
    client.save_index()
    return count


def retrieve(query: str, top_k: int, domain: str | None = None) -> list[str]:
    """Inference-time retrieval hook body — returns up to `top_k` learning strings."""
    client = client_for(domain or current_domain())
    resp = client.recall(query=query, limit=top_k, strategy=recall_strategy())
    memories = resp.get("memories", []) if isinstance(resp, dict) else []
    out = [m["content"] for m in memories if isinstance(m.get("content"), str)]
    return out[:top_k]
