# Pending workflow files

The `.yml.txt` files in this directory are GitHub Actions workflows
that need a token with `workflow` scope to land. The default
`sattyamjjain` token lacks that scope, so they get parked here
until a maintainer with `workflow` scope moves them across.

## Activation drill

```bash
git checkout -b chore/activate-publish-workflows
mkdir -p .github/workflows
for f in cargo-publish pypi-publish npm-publish; do
  git mv ".github/workflows.pending/${f}.yml.txt" \
         ".github/workflows/${f}.yml"
done
git commit -m "chore(ci): activate auto-publish workflows"
git push -u origin chore/activate-publish-workflows
gh pr create --base main --head chore/activate-publish-workflows \
    --title "chore(ci): activate auto-publish workflows" \
    --body "Move from workflows.pending/. See README in that directory."
gh pr merge --admin --merge "$(gh pr list --head chore/activate-publish-workflows --json number -q '.[0].number')"
```

A token with `workflow` scope is required for the push step.
Either run `gh auth refresh -s workflow` once, or use a fresh PAT.

---

## Trigger model — auto-deploy on push to main

All three workflows trigger on **push to main** (plus
`workflow_dispatch` for manual reruns). Each runs a
**version-changed precheck** before any actual publish:

* `cargo-publish.yml` — for each of the 9 internal crates, check
  `crates.io/api/v1/crates/<crate>/<workspace-version>`. Crates
  whose current workspace version is already on crates.io get
  skipped; the rest get queued and published in dependency order.
* `pypi-publish.yml` — check `pypi.org/pypi/mnemo-db/<version>/json`.
  Skip if 200, build wheels + publish if 404.
* `npm-publish.yml` — `npm view @mndfreek/mnemo-sdk@<version>`.
  Skip if it returns the version, publish otherwise.

What this gives you

* **Doc-only commits don't republish.** No version bump → no
  publish attempt → no spurious failure.
* **Version-bump commits publish exactly the channels whose
  manifest changed.** Bump only `Cargo.toml`? Only crates.io
  fires. Bump `pyproject.toml` too? PyPI joins. All three? Three
  parallel publishes.
* **Idempotent.** Re-running on the same commit is always safe.

What this does NOT give you

* **Auto-version-bumping.** You still edit the version field in
  `Cargo.toml` / `pyproject.toml` / `package.json` before pushing.
  No commit-message-driven bump (no semantic-release / changesets).
* **Automatic CHANGELOG generation.** Manual today.

---

## Currently parked

### `cargo-publish.yml.txt` — crates.io

**Prerequisites**

1. `CARGO_REGISTRY_TOKEN` repo secret. **Set 2026-04-25.** Rotate
   per normal cadence.
2. `[workspace.dependencies]` entries in `Cargo.toml` carry
   `version` alongside `path`. **Already present from PR #56.**

**What it publishes**

`mnemo-core` first, then in dep order: `mnemo-graph`, `mnemo-mcp`,
`mnemo-postgres`, `mnemo-rest`, `mnemo-admin`, `mnemo-pgwire`,
`mnemo-grpc`, `mnemo-compliance`, then finally `mnemo-mcp-server`
(the umbrella binary). Each new-crate publish is spaced 12 minutes
apart to stay under crates.io's free-tier rate limit (5 new crates
per hour).

### `pypi-publish.yml.txt` — PyPI

**Prerequisites**

1. PyPI trusted publisher rule (`mnemo-db` → this repo +
   `pypi-publish.yml` + env `pypi`). **Already registered** at
   <https://pypi.org/manage/account/publishing/>.
2. GitHub repo environment named `pypi`. **Already created.**

Linux + macOS wheels for Python 3.10–3.13 + sdist. Windows
wheels deferred — DuckDB+PyO3+Windows is a known sharp edge.

### `npm-publish.yml.txt` — npm

**Prerequisites**

1. `@mndfreek` npm scope owned by the publishing account.
   **Already active** (predates our session).
2. `NPM_TOKEN` repo secret (granular access token with
   `@mndfreek/*` read+write and **2FA-bypass enabled**).
   **Set 2026-04-25.**
3. GitHub repo environment named `npm`. **Already created.**

Publishes with `--provenance` attestation. Prerelease versions
(those with a `-` in the version, e.g. `0.4.0-rc2`) ship to the
`rc` dist-tag so `npm install @mndfreek/mnemo-sdk` keeps resolving
to the latest stable.

---

## Releasing once activated

Edit the version fields in all relevant manifests, commit, push to
main:

```bash
# Edit:
#   Cargo.toml workspace.package.version + [workspace.dependencies] internal `version`
#   python/pyproject.toml [project] version
#   python/mnemo/__init__.py __version__
#   sdks/typescript/package.json version
git commit -am "chore: release 0.4.0"
git push origin main
```

The three workflows fire in parallel. Each one prechecks: only the
channels whose manifest version actually changed will publish.

To re-run after a partial failure (e.g. crates.io rate-limited 4 of 9
crates), open **Actions → cargo-publish → Run workflow** on the same
commit. The precheck queues only the unpublished crates.
