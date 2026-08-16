# Registry-token runbook — five minutes, no context rediscovery

Read this when a release fails on crates.io authentication, or when
`registry_parity.sh` reports a crate behind the workspace. It exists because the
same diagnosis was re-derived four times during the
[#140](https://github.com/sattyamjjain/mnemo/issues/140) drift, and three of
those derivations were **wrong**.

> **Status 2026-08-16: nothing is broken.** Every publishable crate is at
> `0.5.23`, matching the workspace, including `mnemo-mcp-server`. This is a
> runbook for the next failure, not a description of the current one.

## The corrected diagnosis, stated first so it is not re-derived

`mnemo-mcp-server` sat at 0.4.4 for 87 days. The long-held explanation was "the
`CARGO_REGISTRY_TOKEN` is rejected and needs rotating." **That was wrong.** The
token carried both `publish-update` and `publish-new` the whole time — it created
`mnemo-embeddings-bench` on the first real attempt.

Two things actually caused it, and neither is a token:

1. **A packaging bug.** `mnemo-mcp-server` depends on `mnemo-embeddings-bench`,
   which was `publish = false` and path-only. `cargo publish` refuses a published
   crate that depends on an unpublished path crate, so the walk skipped the
   server — and **a skip was indistinguishable from a no-op**, so every run
   reported success. Fixed in #139.
2. **Procedural gates.** v0.5.23 needed a `v0.5.23` git tag and a `## [0.5.23]`
   CHANGELOG heading, and the new-crate gate needed explicit confirmation.

The `HTTP 403` on `/api/v1/me` that anchored the wrong diagnosis was a **granular
token lacking account-read scope**. It is advisory. A token can fail `/me` and
publish perfectly well. The preflight therefore only hard-fails on a *missing*
secret, never on a `/me` 403.

**So: do not rotate the token as a first move.** Work the list below in order.

## Triage in order

### 1. Is anything actually behind?

```bash
scripts/registry_parity.sh --mode preflight
```

Prints the triple (workspace version, newest git tag, crates.io version) for
every publishable crate. Read the `status` column before doing anything else.

### 2. Is the crate in a publish walk at all?

This is the failure mode that produced #140 and the one people miss. A crate in
no walk is published by nothing, and before the severity floor existed it
produced a `::warning::` on every run, forever.

- Release walk: `WALK:` in [`.github/workflows/release-crate.yml`](../../.github/workflows/release-crate.yml)
- Library walk: the queue in [`.github/workflows/cargo-publish.yml`](../../.github/workflows/cargo-publish.yml)

If a crate is missing from the release walk, **add it there** — or set
`publish = false` in its manifest so the intent is explicit. Those are the only
two correct answers.

### 3. Are the procedural gates satisfied?

A release will not publish without all three:

| gate | requirement |
|---|---|
| git tag | a `v<version>` tag exists **on the cut commit**, not on the feature merge |
| CHANGELOG | a `## [X.Y.Z]` heading exists (not just `## [Unreleased]`) |
| `[Unreleased]` | still carries a `### Landing trace` section with a SHA — `changelog_has_landing_trace_section.rs` enforces this |

### 4. Does the walk create a NEW crate?

If so, the run **aborts early by design** unless publish-new is confirmed. The
repo variable `RELEASE_TOKEN_HAS_PUBLISH_NEW` is **not set**, so a bare tag push
will abort. Dispatch it explicitly instead:

```bash
gh workflow run release-crate.yml --ref vX.Y.Z -f confirm_publish_new=true
```

The early abort is deliberate: dying mid-walk strands every crate after the new
one, which is how the server ended up behind `mnemo-embeddings-bench`.

## Only now: rotating the token

Do this if, and only if, steps 1–4 are clean and crates.io still rejects the
credential.

1. Generate a token at <https://crates.io/settings/tokens> with **both**
   `publish-new` and `publish-update`, and a long or absent expiry. There is
   **no API to verify a token's scopes ahead of time** — this is why the
   confirmation input in step 4 exists.
2. Update the `CARGO_REGISTRY_TOKEN` repo secret: Settings → Secrets and
   variables → Actions.
3. Re-run the release: `gh workflow run release-crate.yml --ref vX.Y.Z -f confirm_publish_new=true`.
4. **Smoke-test the artifact, not the exit code.** A green run has meant nothing
   here before:

   ```bash
   cargo install mnemo-mcp-server --force && mnemo --version   # must print X.Y.Z
   ```

Never paste a token into a chat, a commit, an issue, or a CI log. Set it in the
GitHub secrets UI directly.

## What now fails loudly instead of warning

The severity floor, added 2026-08-16 and passed only by `release-crate.yml`:

```bash
scripts/registry_parity.sh --mode preflight --walk "$WALK" --fail-on-minor-lag
```

An out-of-walk crate that is a whole minor behind, or absent from crates.io
entirely, **fails the release** rather than emitting a warning nobody reads.

The threshold is **any minor-level lag**, not "more than one minor" — #140 was
0.4.4 against a 0.5.22 workspace, which is exactly one minor, so the obvious
spelling would have missed the incident it was written for. A patch-level lag
still passes, because between a bump and its publish the repo is legitimately one
ahead. `scripts/registry_parity.sh --self-test` asserts that table offline in CI.

The flag is **not** set on `cargo-publish.yml`. That path publishes libraries on
push-to-main and cannot publish `mnemo-mcp-server` by design; failing it over a
crate only the tag path can ship would block good library releases on a repair it
is incapable of making.

## The four guards, and why they are not merged

| guard | when | what it watches |
|---|---|---|
| [`scripts/registry_parity.sh`](../../scripts/registry_parity.sh) `--mode preflight` | before the walk | online; the full triple, plus the severity floor on the release path |
| [`scripts/registry_parity.sh`](../../scripts/registry_parity.sh) `--mode assert` | after the walk | online; every crate the walk owned actually landed |
| `crates/mnemo-cli/tests/workspace_version_fence.rs` | `cargo test` | offline; Cargo.toml vs git tag vs CHANGELOG, every crate — a mismatch cannot merge |
| [`scripts/check_version_drift.sh`](../../scripts/check_version_drift.sh) | CI | online, baselined; watches standing drift for getting worse |

They cover different failure modes at different times. Merging them would lose
the offline one (which runs on every PR) or the post-publish one (which is the
only thing that catches a silent skip).
