//! v0.4.0 (P2-6) — Markdown+Git working-set adapter.
//!
//! Wuphf (Show HN, 2026-04-27) popularized the Karpathy-style
//! "agent wiki": a git repo of Markdown notes that the agent reads
//! and writes directly. The complaint Mnemo gets is that operators
//! who already have such a wiki don't want to re-platform onto a
//! database — they want recall + provenance over the files they
//! already have. This crate solves that by syncing a git-tracked
//! Markdown directory into Mnemo's hot tier:
//!
//! * Each `.md` file's frontmatter (`mnemo_id`, `tags`, `expires_at`)
//!   maps to a Mnemo memory record.
//! * Heading + body content lands in `MemoryRecord.content`.
//! * Edits round-trip: changing a file produces a new memory version;
//!   appending a memory in Mnemo can be flushed back to disk.
//!
//! This crate ships the **contract layer** (parser, spec, tests).
//! The notify-based watcher and the gix-backed commit-on-flush are
//! follow-ups that swap in heavier deps without changing this API.

pub mod parser;
pub mod spec;

pub use parser::{ParseError, ParsedMarkdown, parse_markdown};
pub use spec::{MdSyncSpec, SyncFlushPolicy};
