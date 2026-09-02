//! Per-chain mutual exclusion for hash-chain appends.
//!
//! A hash chain is built by reading the current head and writing a record that
//! names it. That is a read-modify-write, and it is only a chain if no other
//! writer slips between the two halves. Without this, concurrent appends read
//! the same head and each writes itself as a head: the log keeps its per-record
//! tamper-evidence and loses its ordering, which is the only reason to chain
//! records rather than hash them individually.
//!
//! # Why sharded, and not a map of per-key locks
//!
//! A `HashMap<Key, Arc<Mutex<()>>>` gives perfect isolation between keys, at the
//! cost of growing without bound — one entry per `(agent_id, thread_id)` ever
//! seen — and of an eviction protocol that has to prove nobody holds the lock it
//! is about to drop. That is a real source of bugs for a benefit that does not
//! matter here.
//!
//! A fixed shard array has bounded memory, no eviction and no lifecycle. Its
//! cost is that two unrelated keys can hash to the same shard and briefly queue
//! behind each other: a fairness cost, not a correctness one, and with 64
//! slots a rare one. A single global lock — the thing this
//! deliberately is not — would serialise every agent in the process, which is a
//! worse product than the defect it fixes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

/// Number of independent locks. Sized so a few hundred concurrently active keys
/// rarely collide, while the array stays irrelevant to memory.
const SHARDS: usize = 64;

/// Which chain a key names. The two advance independently, so an append to one
/// must not block an append to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chain {
    /// The `memories` chain.
    Memory,
    /// The `agent_events` chain.
    Event,
}

/// A set of locks covering every chain key, sharded by hash.
#[derive(Debug)]
pub struct ChainLocks {
    shards: Vec<Arc<Mutex<()>>>,
}

impl Default for ChainLocks {
    fn default() -> Self {
        Self::new()
    }
}

impl ChainLocks {
    pub fn new() -> Self {
        Self {
            shards: (0..SHARDS).map(|_| Arc::new(Mutex::new(()))).collect(),
        }
    }

    /// Map a key onto a shard.
    ///
    /// # The event lock is keyed on the agent alone, deliberately
    ///
    /// A lock is only sound if its key is at least as coarse as the scope of the
    /// widest query it protects. `get_latest_event_hash(agent, None)` is scoped
    /// to the whole agent — it carries no `thread_id IS NULL` predicate, unlike
    /// its memories counterpart — so an append under thread `t` inserts a row a
    /// concurrent unthreaded append would have read. Keying the event lock on
    /// `(agent, thread)` would put those two on different locks and leave the
    /// race standing for exactly the mixed-thread case.
    ///
    /// So event appends serialise per agent. That is coarser than it would need
    /// to be if the head query were thread-scoped, and narrowing that query is a
    /// change to what already-written logs mean:
    /// `verify_event_integrity(agent, None)` walks every event for the agent
    /// across threads, so today's unscoped lookup is what makes that
    /// verification pass. Widening the lock fixes the race without rewriting
    /// that meaning.
    ///
    /// Memory appends are genuinely per-`(agent, thread)` — the head lookup and
    /// `list_memories_by_agent_ordered` carry the same predicate — so the finer
    /// key is sound there.
    fn shard_for(&self, chain: Chain, agent_id: &str, thread_id: Option<&str>) -> usize {
        let mut h = DefaultHasher::new();
        chain.hash(&mut h);
        agent_id.hash(&mut h);
        if chain == Chain::Memory {
            // Hash the discriminant as well as the value, so an absent thread
            // and a thread literally named "None" do not share a lock.
            thread_id.is_some().hash(&mut h);
            thread_id.unwrap_or("").hash(&mut h);
        }
        (h.finish() % SHARDS as u64) as usize
    }

    /// Acquire the lock covering `(chain, agent_id, thread_id)`. Hold the guard
    /// across **both** the head read and the insert; releasing it between the
    /// two reintroduces the race in full.
    pub async fn lock(
        &self,
        chain: Chain,
        agent_id: &str,
        thread_id: Option<&str>,
    ) -> MutexGuard<'_, ()> {
        self.shards[self.shard_for(chain, agent_id, thread_id)]
            .lock()
            .await
    }
}

/// The `chain_heads.thread_key` for a chain key.
///
/// Encodes `Option<&str>` as a non-null string, because NULL in a primary key is
/// not comparable with `=` and the two backends disagree about whether it is
/// even allowed. `-` is the absent thread; `t:<id>` a present one, so a thread
/// literally named `-` stays distinct from no thread.
///
/// # Events have one chain per agent
///
/// [`Chain::Event`] ignores `thread_id` entirely — every event an agent writes
/// belongs to one chain. That is what the only verification path in the tree
/// checks: `verify_event_integrity(agent, None)` walks `list_events`, which
/// returns every event for the agent regardless of thread. A per-thread event
/// chain would make that walk fail on any agent that used more than one thread.
///
/// Memory chains are genuinely per-`(agent, thread)`: `get_latest_memory_hash`
/// and `list_memories_by_agent_ordered` both carry the `thread_id` predicate, so
/// the write and the verification agree.
pub fn thread_key(chain: Chain, thread_id: Option<&str>) -> String {
    match (chain, thread_id) {
        (Chain::Event, _) | (_, None) => "-".to_string(),
        (Chain::Memory, Some(t)) => format!("t:{t}"),
    }
}

/// The `chain_heads.chain` discriminant.
pub fn chain_name(chain: Chain) -> &'static str {
    match chain {
        Chain::Memory => "memory",
        Chain::Event => "event",
    }
}

/// A stable 64-bit name for a chain, for backends whose mutual exclusion lives
/// outside this process — a PostgreSQL advisory lock takes a `bigint`.
///
/// Stability is the entire requirement. `DefaultHasher` is explicitly not stable
/// across Rust releases, and two mnemo processes deriving different keys for one
/// chain would take different locks and serialise nothing — a silent no-op that
/// is indistinguishable from a working lock. SHA-256 truncated to its leading
/// eight bytes is stable by specification, and the literals below pin it.
///
/// The scoping rule matches `ChainLocks::shard_for`: event keys ignore
/// `thread_id`, memory keys do not.
pub fn advisory_key(chain: Chain, agent_id: &str, thread_id: Option<&str>) -> i64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(match chain {
        Chain::Memory => b"mnemo:chain:memory\x00".as_slice(),
        Chain::Event => b"mnemo:chain:event\x00".as_slice(),
    });
    h.update(agent_id.as_bytes());
    h.update([0u8]);
    if chain == Chain::Memory {
        match thread_id {
            Some(t) => {
                h.update([1u8]);
                h.update(t.as_bytes());
            }
            None => h.update([0u8]),
        }
    }
    let d = h.finalize();
    i64::from_be_bytes(d[..8].try_into().expect("sha256 yields 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two mnemo builds must agree on these or they take different PostgreSQL
    /// advisory locks. A self-consistency test cannot catch that — every hash is
    /// consistent with itself — so the values are pinned as literals.
    const PINNED_EVENT: i64 = -516657226202830256;
    const PINNED_MEMORY: i64 = -7096874915296593842;

    #[test]
    fn the_two_chains_do_not_share_a_lock_for_the_same_key() {
        let locks = ChainLocks::new();
        // A performance property rather than a correctness one, but worth
        // pinning: appending an event must not queue behind appending a memory.
        assert_ne!(
            locks.shard_for(Chain::Memory, "a", None),
            locks.shard_for(Chain::Event, "a", None),
        );
    }

    #[test]
    fn a_thread_named_none_is_not_the_absent_memory_thread() {
        let locks = ChainLocks::new();
        assert_ne!(
            locks.shard_for(Chain::Memory, "a", None),
            locks.shard_for(Chain::Memory, "a", Some("None")),
        );
    }

    /// Pins the scope decision documented on `shard_for`: narrowing the event
    /// lock to `(agent, thread)` without also narrowing `get_latest_event_hash`
    /// silently restores the mixed-thread race. This fails first.
    #[test]
    fn event_appends_for_one_agent_share_a_lock_across_threads() {
        let locks = ChainLocks::new();
        let base = locks.shard_for(Chain::Event, "a", None);
        assert_eq!(base, locks.shard_for(Chain::Event, "a", Some("t1")));
        assert_eq!(base, locks.shard_for(Chain::Event, "a", Some("t2")));
    }

    #[test]
    fn the_advisory_key_is_pinned_to_a_literal() {
        assert_eq!(
            advisory_key(Chain::Event, "agent-a", None),
            advisory_key(Chain::Event, "agent-a", Some("ignored")),
        );
        assert_ne!(
            advisory_key(Chain::Memory, "agent-a", None),
            advisory_key(Chain::Memory, "agent-a", Some("t")),
        );
        assert_ne!(
            advisory_key(Chain::Memory, "agent-a", None),
            advisory_key(Chain::Event, "agent-a", None),
        );
        assert_eq!(advisory_key(Chain::Event, "agent-a", None), PINNED_EVENT);
        assert_eq!(advisory_key(Chain::Memory, "agent-a", None), PINNED_MEMORY);
    }

    #[test]
    fn thread_keys_are_unambiguous() {
        assert_eq!(thread_key(Chain::Memory, None), "-");
        assert_eq!(thread_key(Chain::Memory, Some("a")), "t:a");
        // A thread literally named "-" must not collide with the absent thread.
        assert_ne!(
            thread_key(Chain::Memory, Some("-")),
            thread_key(Chain::Memory, None)
        );
        // Events: one chain per agent, whatever the thread.
        assert_eq!(thread_key(Chain::Event, Some("anything")), "-");
        assert_eq!(thread_key(Chain::Event, None), "-");
    }

    #[tokio::test]
    async fn the_same_key_serialises_and_releasing_frees_it() {
        let locks = ChainLocks::new();
        let held = locks.lock(Chain::Event, "agent-a", None).await;
        let shard = locks.shard_for(Chain::Event, "agent-a", None);
        // `try_lock` rather than a second `lock().await`, so a regression fails
        // the test instead of hanging it.
        assert!(locks.shards[shard].try_lock().is_err());
        drop(held);
        assert!(locks.shards[shard].try_lock().is_ok());
    }
}
