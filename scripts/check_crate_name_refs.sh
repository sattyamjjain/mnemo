#!/usr/bin/env bash
#
# check_crate_name_refs.sh — fail if the docs tell anyone to install a crate
# this project does not own.
#
# Two crates.io names collide with this project and belong to other people:
#
#   mnemo      -> github.com/aayushadhikari7/mnemo  ("personal knowledge vault")
#   mnemo-cli  -> github.com/watzon/mnemo           ("Mnemo LLM memory proxy")
#
# Neither is ours. This workspace publishes only under the `mnemo-*` prefix, and
# the server binary — whose crate *directory* is `crates/mnemo-cli` — publishes
# as `mnemo-mcp-server`. That mismatch is the trap: a contributor reading the
# tree writes `cargo install mnemo-cli` in a doc, it resolves, it installs
# somebody else's program, and nothing anywhere goes red.
#
# So the guard is on the docs, not on the manifests: `cargo install mnemo` and
# `cargo add mnemo-cli` are perfectly valid commands. They are just the wrong
# ones, and a reader has no way to tell.
#
# Usage
#
#   scripts/check_crate_name_refs.sh              # scan the docs, exit 1 on a hit
#   scripts/check_crate_name_refs.sh --self-test  # prove the matcher offline
#
# The `--self-test` mode exists for the same reason `registry_parity.sh` has one:
# a guard that has never been shown to fire is indistinguishable from a guard
# that cannot fire. It runs the matcher over a table of strings that MUST match
# and strings that MUST NOT, and fails if either column is wrong.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Crate names that are NOT ours. A doc may mention them (this guard's own
# rationale, and the README "Naming" table, both do) but must never put them
# after `cargo install` / `cargo add`.
#
# The trailing boundary is what makes this precise: `mnemo-mcp-server` and
# `mnemo-core` must NOT match, but a bare `mnemo` or `mnemo-cli` must. `[a-z-]`
# in the lookahead position would swallow the hyphen, so the boundary is
# "not followed by another name character".
FOREIGN_RE='cargo (install|add) (mnemo|mnemo-cli)([^a-zA-Z0-9_-]|$)'

# Files to scan: every tracked markdown file, minus the changelog.
#
# CHANGELOG.md is excluded deliberately. It is an append-only historical record
# and it *quotes* the wrong command on purpose — the 2026-08-12 entry says
# "Verified no doc tells a user to `cargo add mnemo`". Rewriting history to
# appease a linter would be the wrong repair.
#
# `git ls-files` keeps this to tracked files, so a build artifact under
# docs/book/ cannot fail the build.
scan_targets() {
  git -C "$REPO_ROOT" ls-files -- '*.md' 2>/dev/null \
    | grep -v '^CHANGELOG\.md$' \
    | sort -u
}

# Report only commands inside a fenced code block.
#
# This is the line between an *instruction* and *prose about an instruction*.
# The README "Naming" section has to be able to write `cargo install mnemo` in a
# sentence in order to tell you not to run it; a fenced block is a thing readers
# copy. Flagging both would make the guard unusable and it would be switched off,
# which is the usual way a check like this dies.
scan_file() {
  local f="$1"
  awk -v re="$FOREIGN_RE" -v fname="$f" '
    /^[[:space:]]*```/ { fence = !fence; next }
    fence && $0 ~ re   { printf "%s:%d:%s\n", fname, NR, $0 }
  ' "$REPO_ROOT/$f"
}

self_test() {
  local st_fail=0

  # MUST match — these install the wrong crate.
  local -a should_match=(
    'cargo install mnemo'
    'cargo add mnemo'
    'cargo install mnemo-cli'
    'cargo add mnemo-cli'
    'run `cargo install mnemo` to get started'
    'cargo add mnemo --features foo'
    '$ cargo install mnemo-cli --force'
  )
  # MUST NOT match — these are ours, or are prose.
  local -a should_not_match=(
    'cargo install mnemo-mcp-server'
    'cargo add mnemo-core mnemo-compliance'
    'cargo add mnemo-mcp'
    'cargo add mnemo-core'
    'cargo add mnemo-compliance'
    'cargo install mnemo-mcp-server --force && mnemo --version'
    'cargo build --release -p mnemo-mcp-server'
    'the mnemo crate on crates.io is unrelated'
    'cargo add mnemo-clippy-thing'
  )

  local s
  for s in "${should_match[@]}"; do
    if printf '%s\n' "$s" | grep -qE "$FOREIGN_RE"; then
      printf '  ok    MATCH     %s\n' "$s"
    else
      printf '  FAIL  NO-MATCH  %s  (should have been caught)\n' "$s"
      st_fail=1
    fi
  done
  for s in "${should_not_match[@]}"; do
    if printf '%s\n' "$s" | grep -qE "$FOREIGN_RE"; then
      printf '  FAIL  MATCH     %s  (false positive — this is ours)\n' "$s"
      st_fail=1
    else
      printf '  ok    NO-MATCH  %s\n' "$s"
    fi
  done

  # Fence awareness: the same string must be caught inside a fenced block and
  # ignored in prose. Without this case the two halves of the rule could drift
  # apart and the guard would either cry wolf or go quiet.
  local tmp
  tmp="$(mktemp -t crate-name-refs-selftest.XXXXXX)"
  trap 'rm -f "$tmp"' RETURN
  cat > "$tmp" <<'FIXTURE'
Prose may name the trap: do not run `cargo install mnemo`, it is not ours.

```bash
cargo install mnemo
```
FIXTURE
  # scan_file resolves against REPO_ROOT, so hand it a path relative to that.
  local rel="${tmp#"$REPO_ROOT"/}"
  local found
  if [[ "$rel" == "$tmp" ]]; then
    # mktemp landed outside the repo (the normal case) — scan it directly.
    found="$(awk -v re="$FOREIGN_RE" -v fname="fixture" '
      /^[[:space:]]*```/ { fence = !fence; next }
      fence && $0 ~ re   { printf "%s:%d\n", fname, NR }
    ' "$tmp")"
  else
    found="$(scan_file "$rel")"
  fi
  local n
  n="$(printf '%s' "$found" | grep -c . || true)"
  if [[ "$n" -eq 1 ]]; then
    printf '  ok    FENCE     1 hit in the fenced block, 0 in the prose line\n'
  else
    printf '  FAIL  FENCE     expected exactly 1 fenced hit, got %s\n' "$n"
    st_fail=1
  fi

  if [[ $st_fail -ne 0 ]]; then
    echo "::error::check_crate_name_refs.sh --self-test FAILED: the matcher no longer separates the crates we publish from the two we do not own, or lost the fenced-vs-prose distinction. A guard that cannot fire is worse than no guard, because it reads as coverage." >&2
    exit 1
  fi
  echo "check_crate_name_refs.sh --self-test OK (matcher catches bare mnemo / mnemo-cli in fenced commands, passes every mnemo-* crate we publish and all prose)"
  exit 0
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
fi
if [[ -n "${1:-}" ]]; then
  echo "::error::check_crate_name_refs.sh: unknown argument '${1}' (expected no args, or --self-test)" >&2
  exit 2
fi

hits=0
while IFS= read -r f; do
  [[ -n "$f" ]] || continue
  [[ -f "$REPO_ROOT/$f" ]] || continue
  while IFS= read -r line; do
    printf '%s\n' "$line"
    hits=$((hits + 1))
  done < <(scan_file "$f")
done < <(scan_targets)

if [[ $hits -gt 0 ]]; then
  cat >&2 <<'EOF'
::error::check_crate_name_refs.sh: the docs above tell a reader to install a crate this project does not own.

  `mnemo`      on crates.io is github.com/aayushadhikari7/mnemo
  `mnemo-cli`  on crates.io is github.com/watzon/mnemo

Neither is this project. The server binary publishes as `mnemo-mcp-server`
(its crate directory is `crates/mnemo-cli`, which is what makes this an easy
mistake to make). Use one of:

  cargo install mnemo-mcp-server
  cargo add mnemo-core mnemo-compliance
  cargo add mnemo-mcp

See the "Naming" section in README.md.
EOF
  exit 1
fi

echo "check_crate_name_refs.sh OK (no doc installs a crate we do not own)"
