//! Evaluation harnesses that exercise the engine in shapes the
//! integration test suite + bench bins reuse.
//!
//! Today this module hosts a single harness, [`memfail`], which
//! decomposes each end-to-end recall into the three operation seams
//! mnemo actually exposes (store / summarize / retrieve) so that an
//! observed failure can be attributed to one stage instead of the
//! whole pipeline.

pub mod memfail;
