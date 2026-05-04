//! v0.4.2 (U2) — README marketing-phrase lint.
//!
//! The "Why mnemo when Cloudflare Agent Memory exists?" section is
//! deliberately framed as an honest concession + differentiation pitch.
//! It explicitly cedes edge-recall p50 to Cloudflare and positions
//! mnemo's axis on provenance + chain replay + sovereignty.
//!
//! This test fails the build if any of three marketing phrases shows
//! up in `README.md`. They are the phrases an operator skim-reading
//! the README would translate to "the mnemo authors think they beat
//! Cloudflare on perf" — which would be wrong, and which the v0.4.2
//! prompt explicitly called out as the marketing risk to lint against.

use std::path::Path;

const BANNED_PHRASES: &[&str] = &[
    // v0.4.2 (U2) — Cloudflare-comparison banned framing.
    "beat Cloudflare",
    "faster than Cloudflare",
    "Cloudflare killer",
    // v0.4.3 (A1) — extended canonical banned-phrases ledger. The
    // operator's running policy ("ship one row honestly over five rows
    // aspirationally") rules out every form of viral / breakthrough /
    // game-changing framing in the README. Per-claim primary-source
    // links, honest concessions, and bench numbers are the substrates
    // we promote.
    "blow up",
    "viral",
    "game-changing",
    "game changing",
    "revolutionary",
    "wild",
    "mind-blowing",
    "mind blowing",
];

#[test]
fn readme_does_not_carry_banned_marketing_phrases() {
    let readme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    let body =
        std::fs::read_to_string(&readme_path).expect("README.md must be readable from repo root");

    let lower = body.to_lowercase();
    let mut hits: Vec<&'static str> = Vec::new();
    for phrase in BANNED_PHRASES {
        if lower.contains(&phrase.to_lowercase()) {
            hits.push(phrase);
        }
    }

    assert!(
        hits.is_empty(),
        "README.md carries banned marketing phrase(s): {hits:?}. \
         The v0.4.2 Cloudflare-differentiation section must concede \
         edge-recall perf, not claim parity or superiority. See \
         docs/comparisons/cloudflare-agent-memory.md for the framing."
    );
}

#[test]
fn readme_carries_required_cloudflare_section_anchor() {
    // Inverse: confirm the differentiation section is actually present
    // so a future blanket README rewrite can't silently delete it.
    let readme_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    let body = std::fs::read_to_string(&readme_path).expect("README.md must be readable");
    assert!(
        body.contains("Why mnemo when Cloudflare Agent Memory exists?"),
        "README.md must keep the Cloudflare differentiation H2 from v0.4.2 (U2)."
    );
    assert!(
        body.contains("docs/comparisons/cloudflare-agent-memory.md"),
        "README.md must link to docs/comparisons/cloudflare-agent-memory.md."
    );
}
