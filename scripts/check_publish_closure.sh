#!/usr/bin/env bash
# Assert the publish closure covers every crate that is supposed to ship.
#
# WHY THIS EXISTS
#
# Two releases in a row died on the same shape of bug: the list of crates to
# publish was written down in more than one place, and the copies drifted.
#   - v0.5.24 stranded mnemo-admin/mnemo-pgwire (the dry-run kept an old copy).
#   - v0.5.25 failed twice with `failed to select a version for the requirement
#     mnemo-admin` for the same reason, one copy further along.
# Both were fixed by hand, and the fix was verified by eye.
#
# Then a third variant surfaced with no failure at all: seven crates published
# only on the push-to-main lane, that lane stopped partway through v0.5.25, and
# they silently sat a patch behind the workspace. Nothing was red. The drift
# guard had to be taught to *describe* the split rather than prevent it.
#
# The general form of all three is: a crate that should ship is not in the list
# that ships crates. This script is that assertion, run in CI, so the next new
# crate cannot be orphaned by being forgotten.
#
# WHAT IT CHECKS
#   1. Every publishable workspace member appears in release-crate.yml's WALK,
#      or is explicitly exempt with a recorded reason.
#   2. Every name in WALK is a real publishable member (catches typos and
#      crates removed from the workspace but left in the list).
#   3. Everything the push-to-main library lane publishes is also in WALK, so
#      the tag lane can never ship less than the library lane.
#
# Usage: check_publish_closure.sh [--self-test]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_WF="$REPO_ROOT/.github/workflows/release-crate.yml"
LIBRARY_WF="$REPO_ROOT/.github/workflows/cargo-publish.yml"

# Crates that are publishable in cargo metadata but deliberately never go to
# crates.io. Each MUST carry a reason: an exemption without one is how a real
# orphan gets parked here and forgotten.
exemption_reason() {
  case "$1" in
    mnemo-python)
      echo "PyO3 extension built by maturin and published to PyPI as \`mnemo-db\`, never to crates.io" ;;
    mnemo-golem-host)
      echo "excluded from the CI workspace build (rust-lld rejects the generated \`cabi_post_mnemo:golem-vector/...\` symbol), so CI cannot build it, let alone publish it; unpublished on crates.io" ;;
    *) return 1 ;;
  esac
}

# ---- extraction -------------------------------------------------------------

publishable_members() {
  cargo metadata --no-deps --format-version 1 --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null \
  | python3 -c '
import json, sys
m = json.load(sys.stdin)
ids = set(m["workspace_members"])
for p in m["packages"]:
    if p["id"] in ids and p.get("publish") != []:
        print(p["name"])
' | sort -u
}

# Fields every published crate must declare, and why each one is not optional.
#
# A crates.io page with no repository, no homepage and no keywords is a dead end:
# a reader cannot get to the source, and nobody searching "mcp memory" finds it.
# `mnemo-mcp-server` — the crate the README tells people to `cargo install`,
# twice — shipped through v0.5.27 with ALL of these null. Nothing was red,
# because nothing was looking.
#
# `documentation` is included because cargo does NOT default it to docs.rs in the
# published metadata; an absent value is an absent link.
#
# Metadata only takes effect on a NEW version. Editing the manifest without
# publishing changes nothing a user can see, which is exactly how this stayed
# broken across 27 releases.
REQUIRED_METADATA="repository homepage documentation keywords categories"

crate_metadata_gaps() {
  # Emits "<crate> <missing-field>..." per offending publishable member.
  cargo metadata --no-deps --format-version 1 --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null \
  | python3 -c '
import json, sys, tomllib
required = sys.argv[1].split()
m = json.load(sys.stdin)
ids = set(m["workspace_members"])
for p in sorted(m["packages"], key=lambda x: x["name"]):
    if p["id"] not in ids or p.get("publish") == []:
        continue
    try:
        raw = tomllib.load(open(p["manifest_path"], "rb")).get("package", {})
    except Exception as e:
        print(p["name"], "UNREADABLE:" + str(e).replace(" ", "_"))
        continue
    # An inherited value parses as {"workspace": True}; an empty list is as
    # absent as a missing key, which is how `keywords = []` reads on crates.io.
    missing = [k for k in required if not raw.get(k)]
    if missing:
        print(p["name"], " ".join(missing))
' "$REQUIRED_METADATA"
}

walk_crates() {
  # The single WALK definition, as the workflow expands it.
  sed -n 's/^  WALK: "\(.*\)"$/\1/p' "$1" | tr ' ' '\n' | grep -E '^mnemo-' | sort -u
}

library_lane_crates() {
  # The `for crate in ... ; do` list in the push-to-main lane. Bounded to that
  # loop so unrelated crate names in comments are not picked up.
  python3 - "$1" <<'PY'
import re, sys
s = open(sys.argv[1]).read()
m = re.search(r'for crate in\s+(.*?);\s*do', s, re.S)
if not m:
    sys.exit("could not find the `for crate in ...; do` publish loop")
body = m.group(1).replace("\\\n", " ").replace("\\", " ")
names = sorted(set(re.findall(r'\bmnemo-[a-z0-9-]+', body)))
print("\n".join(names))
PY
}

# ---- the check --------------------------------------------------------------

run_check() {
  local release_wf="$1" library_wf="$2"
  local failed=0

  local members walk lib
  members="$(publishable_members)"
  walk="$(walk_crates "$release_wf")"
  lib="$(library_lane_crates "$library_wf")"

  echo "publishable workspace members: $(echo "$members" | wc -l | tr -d ' ')"
  echo "crates in release WALK:        $(echo "$walk" | wc -l | tr -d ' ')"
  echo "crates in library lane:        $(echo "$lib" | wc -l | tr -d ' ')"
  echo

  # 1. every publishable member is in WALK, or exempt with a reason
  local missing=""
  while read -r c; do
    [ -z "$c" ] && continue
    if ! grep -qx "$c" <<<"$walk"; then
      if reason="$(exemption_reason "$c")"; then
        echo "  exempt  $c - $reason"
      else
        missing="$missing $c"
      fi
    fi
  done <<<"$members"

  if [ -n "$missing" ]; then
    failed=1
    echo
    echo "FAIL: publishable crate(s) missing from the release WALK:$missing"
    echo "      Nothing ships them, so they will silently fall behind the workspace."
    echo "      Add them to WALK in .github/workflows/release-crate.yml (after any"
    echo "      crate they depend on), or add an exemption WITH A REASON to"
    echo "      exemption_reason() in this script."
  fi

  # 3. every publishable member carries the crates.io metadata a reader needs.
  #    Checked here rather than in a separate guard because this script already
  #    knows which crates actually ship, and that is precisely the set that has
  #    a crates.io page to be blank.
  # NOT `|| true`. The first version of this swallowed a SyntaxError in the
  # embedded python and then printed "every publishable member declares ..."
  # over a checker that had produced nothing at all. A checker that crashed must
  # not be indistinguishable from a checker that found no problems.
  local gaps rc
  gaps="$(crate_metadata_gaps)" && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    failed=1
    echo
    echo "FAIL: the crates.io metadata check itself failed (exit $rc). That is a"
    echo "      broken guard, not a clean repository."
    printf '%s\n' "$gaps" | sed 's/^/        /'
  elif [ -n "$gaps" ]; then
    failed=1
    echo
    echo "FAIL: publishable crate(s) missing crates.io metadata:"
    printf '%s\n' "$gaps" | sed 's/^/        /'
    echo "      Required: $REQUIRED_METADATA"
    echo "      Add the inheritable ones as \`<field>.workspace = true\` (the root"
    echo "      [workspace.package] already defines repository/homepage/documentation),"
    echo "      and give each crate its own keywords (max 5, max 20 chars) and"
    echo "      categories (slugs must exist on crates.io or the publish is rejected)."
    echo "      NOTE: metadata only lands on a NEW version — bump and publish."
  else
    echo "  every publishable member declares: $REQUIRED_METADATA"
  fi

  # 2. every WALK entry is a real publishable member
  local unknown=""
  while read -r c; do
    [ -z "$c" ] && continue
    grep -qx "$c" <<<"$members" || unknown="$unknown $c"
  done <<<"$walk"
  if [ -n "$unknown" ]; then
    failed=1
    echo
    echo "FAIL: WALK names crate(s) that are not publishable workspace members:$unknown"
    echo "      A typo here makes the publish loop fail mid-release, after earlier"
    echo "      crates have already uploaded and cannot be taken back."
  fi

  # 3. the library lane must not exceed WALK
  local extra=""
  while read -r c; do
    [ -z "$c" ] && continue
    grep -qx "$c" <<<"$walk" || extra="$extra $c"
  done <<<"$lib"
  if [ -n "$extra" ]; then
    failed=1
    echo
    echo "FAIL: the push-to-main library lane publishes crate(s) the tag lane does not:$extra"
    echo "      That is the two-lane split that left seven crates a patch behind at"
    echo "      v0.5.25. Whatever one lane ships, the tag lane must ship too."
  fi

  if [ "$failed" -eq 0 ]; then
    echo "OK: the release WALK covers every publishable crate, names only real ones,"
    echo "    and is a superset of the library lane."
  fi
  return "$failed"
}

# ---- self-test --------------------------------------------------------------
#
# A guard is only worth having if it fails when it should. These fixtures mutate
# a copy of the real workflow and assert the check goes red.

self_test() {
  local tmp status pass=0 fail=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN

  expect() { # expect <expected: pass|fail> <label> <release_wf> <library_wf>
    local want="$1" label="$2" rwf="$3" lwf="$4"
    if run_check "$rwf" "$lwf" >/dev/null 2>&1; then status=pass; else status=fail; fi
    if [ "$status" = "$want" ]; then
      echo "  ok   $label (expected $want)"; pass=$((pass+1))
    else
      echo "  FAIL $label (expected $want, got $status)"; fail=$((fail+1))
    fi
  }

  # baseline: the real files must pass
  expect pass "real workflows as committed" "$RELEASE_WF" "$LIBRARY_WF"

  # a publishable crate dropped from WALK
  sed 's/ mnemo-baseline//' "$RELEASE_WF" > "$tmp/drop.yml"
  expect fail "publishable crate dropped from WALK" "$tmp/drop.yml" "$LIBRARY_WF"

  # the exact v0.5.24 regression: mnemo-admin missing
  sed 's/ mnemo-admin//' "$RELEASE_WF" > "$tmp/admin.yml"
  expect fail "mnemo-admin missing (the v0.5.24/v0.5.25 failure)" "$tmp/admin.yml" "$LIBRARY_WF"

  # a typo in WALK
  sed 's/mnemo-pgwire/mnemo-pgwyre/' "$RELEASE_WF" > "$tmp/typo.yml"
  expect fail "typo'd crate name in WALK" "$tmp/typo.yml" "$LIBRARY_WF"

  # the two-lane split that motivated this script: library lane ships more
  sed 's/ mnemo-letta mnemo-mesh mnemo-codemode mnemo-deal mnemo-md-sync mnemo-cma mnemo-baseline//' \
    "$RELEASE_WF" > "$tmp/split.yml"
  expect fail "library lane ships crates the tag lane does not" "$tmp/split.yml" "$LIBRARY_WF"

  # --- the crates.io metadata assertion ------------------------------------
  # Driven through REQUIRED_METADATA rather than workflow fixtures, because that
  # check reads real cargo metadata and has no workflow input to mutate.
  local out

  # A field every crate really does have (license is inherited workspace-wide)
  # must produce no gaps. If this reports gaps the detector is over-firing.
  out="$(REQUIRED_METADATA="license" crate_metadata_gaps)"
  if [ -z "$out" ]; then
    echo "  ok   metadata check is quiet on a field every crate has"; pass=$((pass+1))
  else
    echo "  FAIL metadata check reports gaps for 'license': $out"; fail=$((fail+1))
  fi

  # A field NO crate has must name crates. This is the non-vacuity test: a
  # checker that can never report anything is not coverage.
  out="$(REQUIRED_METADATA="definitely-not-a-cargo-field" crate_metadata_gaps)"
  if grep -q "mnemo-mcp-server definitely-not-a-cargo-field" <<<"$out"; then
    echo "  ok   metadata check names a crate missing a required field"; pass=$((pass+1))
  else
    echo "  FAIL metadata check found nothing for an impossible field"; fail=$((fail+1))
  fi

  # The real requirement must be satisfied right now — this is the assertion
  # that would have caught mnemo-mcp-server shipping 27 releases with a blank
  # crates.io page.
  out="$(crate_metadata_gaps)"
  if [ -z "$out" ]; then
    echo "  ok   every publishable crate satisfies REQUIRED_METADATA today"; pass=$((pass+1))
  else
    echo "  FAIL crates missing required metadata:"; printf '%s\n' "$out" | sed 's/^/       /'
    fail=$((fail+1))
  fi

  echo
  echo "self-test: $pass passed, $fail failed"
  [ "$fail" -eq 0 ]
}

case "${1:-}" in
  --self-test) self_test ;;
  "")          run_check "$RELEASE_WF" "$LIBRARY_WF" ;;
  *)           echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac
