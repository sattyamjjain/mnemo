# Pending workflow files

The `.yml.txt` files in this directory are GitHub Actions workflows
that need a token with `workflow` scope to land. The default
`sattyamjjain` token lacks that scope, so they get parked here
until a maintainer with `workflow` scope moves them across.

The v0.3.3 cohort (`benchmarks-nightly` + `security`) was already
moved out of this directory in PR #43; the steady-state list lives
below.

## Activation drill (one-time per workflow)

```bash
git checkout -b chore/activate-<name>
mkdir -p .github/workflows
git mv .github/workflows.pending/<name>.yml.txt \
       .github/workflows/<name>.yml
git commit -m "chore(ci): activate <name> workflow"
git push -u origin chore/activate-<name>
gh pr create --base main --head chore/activate-<name> \
    --title "chore(ci): activate <name>" \
    --body "Move from workflows.pending/. See the file's top-of-file docs for prerequisites."
gh pr merge --admin --merge <pr-number>
```

A token with `workflow` scope is required for the push step. Either
refresh the personal token via `gh auth refresh -s workflow`, or do
the move-and-push from the work account.

---

## Currently parked

### `cargo-publish.yml.txt` — crates.io publish

Publishes every public mnemo crate in dependency order on every
`v*` tag push. Also supports `workflow_dispatch` with a tag input
and a dry-run flag.

**Before activating:**

1. `CARGO_REGISTRY_TOKEN` repo secret — generate at
   <https://crates.io/me> → API Tokens, scope to publish-new +
   publish-update for every crate name. First publish reserves the
   name; subsequent publishes update.
2. The tag's `Cargo.toml` must have `version` specifiers next to
   `path` on every internal `[workspace.dependencies]` entry. v0.4.0
   onwards has this; pre-v0.4.0 tags can't be published without
   re-tagging from a Cargo.toml that does.
3. Local sanity-check before the first activation:
   ```bash
   cargo publish -p mnemo-core --dry-run
   ```
   This fails fast on naming conflicts or registry issues without
   actually publishing.

### `pypi-publish.yml.txt` — PyPI publish (OIDC trusted publisher)

Builds maturin wheels for `python/mnemo` (linux + macOS, Python
3.10–3.13) plus an sdist; publishes via OIDC trusted publishing
(no API token in repo).

**Before activating:**

1. Register `mnemo` on PyPI with a trusted publisher pointing at
   this repo + the workflow file `pypi-publish.yml` + environment
   `pypi`:
   <https://pypi.org/manage/account/publishing/>
2. Create a GitHub repo environment named `pypi` (Settings →
   Environments → New environment). No secrets needed inside it —
   the OIDC token is issued at runtime.
3. Confirm `python/pyproject.toml` `version` is PyPI-compatible.
   v0.3.4 and v0.4.0rc1 both work (PyPI normalises `v0.4.0-rc1`
   to `0.4.0rc1`).

Windows wheels are deferred — DuckDB + PyO3 + Windows is a known
sharp edge, and shipping a wheel that fails at runtime is worse
than not shipping one. Add after the first PyPI release succeeds.

### `npm-publish.yml.txt` — npm publish (`@mnemo/sdk`)

Publishes the TypeScript SDK with npm provenance attestation.

**Before activating:**

1. The `@mnemo` npm scope must exist and be owned by the publishing
   account: <https://www.npmjs.com/org/create>
2. `NPM_TOKEN` repo secret — automation token with publish scope
   for `@mnemo/sdk`:
   <https://www.npmjs.com/settings/sattyamjjain/tokens>
3. Create a GitHub repo environment named `npm`.

npm's full-OIDC trusted publisher is supported but less battle-
tested than PyPI's. Token + `--provenance` is the safer default
for now. Reassess in a release or two.

---

## How to fire a publish run

After all three workflows are activated and their respective
prerequisites are configured, a single `git push origin v0.4.0`
(or whatever the next tag is) triggers all three in parallel.

To publish a tag that already exists (e.g. backfill `v0.4.0` after
this rewrite lands), use `workflow_dispatch` from the Actions UI
with the tag name as input. The workflows checkout that tag and
build from its committed `Cargo.toml` / `pyproject.toml` / npm
`package.json`.
