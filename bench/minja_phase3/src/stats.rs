//! Re-export of the shared Wilson-95 helper.
//!
//! Deliberately a re-export rather than a second implementation: every rate in
//! this repo is supposed to come from the same interval formula, and a bench
//! that quietly grew its own would be free to disagree with the others by a
//! percentage point nobody could account for.
pub use mnemo_locomo_bench::stats::wilson_95;
