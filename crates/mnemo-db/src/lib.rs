//! # `mnemo-db` — name-reservation pointer (intentionally empty)
//!
//! **This crate ships no functionality.** `mnemo` — an MCP-native memory
//! database for AI agents — is published to crates.io as a set of focused
//! crates, not as a single `mnemo-db` crate. Install what you need:
//!
//! ```text
//! cargo add mnemo-core   # embeddable memory engine: storage, vector + full-text
//!                        # search, hash-chained tamper-evident audit log
//! cargo add mnemo-mcp    # MCP server exposing REMEMBER / RECALL / FORGET / SHARE
//! ```
//!
//! Also on crates.io: [`mnemo-compliance`](https://crates.io/crates/mnemo-compliance)
//! (regulatory mappings) and [`mnemo-attention-state`](https://crates.io/crates/mnemo-attention-state).
//!
//! Source, docs, and the full crate list:
//! <https://github.com/sattyamjjain/mnemo>
//!
//! ## Why this crate exists
//!
//! The unqualified `mnemo` name on crates.io is held by an unrelated crate, so
//! a Rust user might reach for `mnemo-db` by analogy. This crate reserves that
//! name and points at the real crates instead of leaving it to a squatter.
//!
//! ## Not the Python package
//!
//! The Python bindings are distributed **on PyPI** as `mnemo-db`
//! (`pip install mnemo-db`) — a separate registry and a real, functional
//! package. This crates.io Rust crate is only a pointer; do not confuse the two.

/// The crates that actually implement mnemo. See the crate-level documentation
/// for install commands.
pub const SEE_INSTEAD: &[&str] = &["mnemo-core", "mnemo-mcp", "mnemo-compliance"];

/// Canonical project URL.
pub const REPOSITORY: &str = "https://github.com/sattyamjjain/mnemo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_names_the_real_crates() {
        assert!(SEE_INSTEAD.contains(&"mnemo-core"));
        assert!(SEE_INSTEAD.contains(&"mnemo-mcp"));
        assert!(REPOSITORY.starts_with("https://github.com/sattyamjjain/mnemo"));
    }
}
