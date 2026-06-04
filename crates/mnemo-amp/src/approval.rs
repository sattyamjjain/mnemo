//! Human-in-the-loop (HITL) diff-and-approve hook.
//!
//! AMP gates **long-term writes** (`semantic` / `procedural` memory
//! types) behind an optional approval step: before the write commits,
//! a [`WriteDiff`] describing what would change is handed to an
//! [`ApprovalHook`], which returns [`Approval::Approve`] or
//! [`Approval::Reject`]. On approval the store emits a
//! `Decision` audit event through mnemo's existing
//! hash-chained event log, so the approve trail is tamper-evident and
//! replayable alongside the write it authorized.
//!
//! Short-term tiers (`episodic` / `working`) bypass approval — they
//! are high-churn and not worth a human gate.

use crate::wire::AmpMemoryType;

/// What a pending long-term write would change.
///
/// For a fresh write `before` is `None`; for a `merge` it carries the
/// concatenated source content so the reviewer sees what is being
/// folded together.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteDiff {
    pub agent_id: String,
    pub memory_type: AmpMemoryType,
    /// Existing content being replaced/folded, if any.
    pub before: Option<String>,
    /// Proposed content.
    pub after: String,
    pub tags: Vec<String>,
}

impl WriteDiff {
    /// A compact, deterministic textual diff suitable for hashing into
    /// the audit trail or showing a reviewer. Stable across runs (no
    /// timestamps / addresses).
    pub fn render(&self) -> String {
        match &self.before {
            Some(b) => format!(
                "[{}] tags={:?}\n- {}\n+ {}",
                self.memory_type.as_str(),
                self.tags,
                b,
                self.after
            ),
            None => format!(
                "[{}] tags={:?}\n+ {}",
                self.memory_type.as_str(),
                self.tags,
                self.after
            ),
        }
    }
}

/// The outcome of a HITL review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    Approve,
    Reject(String),
}

impl Approval {
    pub fn is_approved(&self) -> bool {
        matches!(self, Approval::Approve)
    }
}

/// A pluggable approval gate consulted before long-term writes.
///
/// Implementors are `Send + Sync` so a router can hold one behind an
/// `Arc` and share it across async tasks. The default
/// [`AutoApprove`] approves everything (no human in the loop); wire a
/// real reviewer by implementing this trait or using
/// [`ClosureApprove`].
pub trait ApprovalHook: Send + Sync {
    fn review(&self, diff: &WriteDiff) -> Approval;
    fn name(&self) -> &str;
}

/// No-op hook: approves every write. The default when no HITL gate is
/// configured.
#[derive(Debug, Clone, Default)]
pub struct AutoApprove;

impl ApprovalHook for AutoApprove {
    fn review(&self, _diff: &WriteDiff) -> Approval {
        Approval::Approve
    }
    fn name(&self) -> &str {
        "auto_approve"
    }
}

/// Hook backed by an injectable closure, so tests (and real
/// integrations that bridge to an out-of-band review UI) can supply a
/// deterministic decision without defining a new type.
pub struct ClosureApprove {
    f: Box<dyn Fn(&WriteDiff) -> Approval + Send + Sync>,
}

impl ClosureApprove {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&WriteDiff) -> Approval + Send + Sync + 'static,
    {
        Self { f: Box::new(f) }
    }
}

impl ApprovalHook for ClosureApprove {
    fn review(&self, diff: &WriteDiff) -> Approval {
        (self.f)(diff)
    }
    fn name(&self) -> &str {
        "closure_approve"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff() -> WriteDiff {
        WriteDiff {
            agent_id: "a".into(),
            memory_type: AmpMemoryType::Semantic,
            before: None,
            after: "Paris is the capital of France".into(),
            tags: vec!["geo".into()],
        }
    }

    #[test]
    fn auto_approve_always_approves() {
        assert_eq!(AutoApprove.review(&diff()), Approval::Approve);
    }

    #[test]
    fn closure_hook_is_honoured() {
        let hook = ClosureApprove::new(|d| {
            if d.after.contains("France") {
                Approval::Approve
            } else {
                Approval::Reject("off-topic".into())
            }
        });
        assert!(hook.review(&diff()).is_approved());

        let mut other = diff();
        other.after = "unrelated".into();
        assert_eq!(hook.review(&other), Approval::Reject("off-topic".into()));
    }

    #[test]
    fn diff_render_is_deterministic() {
        let d = diff();
        assert_eq!(d.render(), d.render());
        assert!(d.render().contains("+ Paris is the capital of France"));
    }
}
