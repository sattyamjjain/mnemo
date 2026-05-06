//! v0.4.4 (U2) — every `## [Unreleased]` block must carry a
//! `### Landing trace` sub-heading whose body references at least one
//! hex commit-sha-prefix (7-40 hex chars). This forces every future
//! docs-only PR landing inside `[Unreleased]` to pin its on-`main`
//! commit pointer in the CHANGELOG, so an operator reading the
//! `[Unreleased]` block can verify the rows are not in a local-only
//! state.
//!
//! When v0.4.4 cuts, the `[Unreleased]` block becomes `## [0.4.4]` and
//! the landing-trace sub-block carries inside it; a fresh
//! `[Unreleased]` opens above. This test continues to pass as long as
//! the new `[Unreleased]` also gains a Landing-trace sub-block on the
//! cut commit.

use std::path::Path;

#[test]
fn unreleased_block_carries_landing_trace_with_commit_sha() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("CHANGELOG.md");
    let body = std::fs::read_to_string(&path).expect("CHANGELOG.md readable");

    let unreleased_idx = body
        .find("## [Unreleased]")
        .expect("CHANGELOG.md must carry a `## [Unreleased]` heading");
    let after_unreleased = &body[unreleased_idx..];
    // Bound the search to the next top-level heading (`## [`) so the
    // landing-trace assertion only inspects the [Unreleased] block.
    let next_release_relative = after_unreleased[1..]
        .find("\n## [")
        .map(|i| i + 1)
        .unwrap_or(after_unreleased.len());
    let unreleased_block = &after_unreleased[..next_release_relative];

    assert!(
        unreleased_block.contains("### Landing trace"),
        "`## [Unreleased]` block must carry a `### Landing trace` \
         sub-heading. Every docs-only PR landing inside `[Unreleased]` \
         is required to pin its on-`main` commit pointer here so a \
         future operator can verify the rows are not local-only."
    );

    // Look for a hex commit-sha-prefix anywhere in the block.
    // Pattern: 7 to 40 lowercase hex chars, surrounded by non-hex
    // characters (so we don't false-match on a longer hex run).
    let has_sha = unreleased_block
        .split(|c: char| !c.is_ascii_hexdigit())
        .any(|chunk| {
            let len = chunk.len();
            (7..=40).contains(&len) && chunk.chars().all(|c| c.is_ascii_hexdigit())
        });
    assert!(
        has_sha,
        "`### Landing trace` sub-block must reference at least one \
         hex commit-sha-prefix (7-40 hex chars) so the on-`main` \
         pointer is concrete. Use the form ``[`abc1234`](https://github.com/sattyamjjain/mnemo/commit/<full-sha>)``."
    );
}
