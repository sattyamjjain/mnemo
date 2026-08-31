#!/usr/bin/env bash
# The README may not assert the release STATE by hand. The generated blocks own it.
#
# # The bug this exists for
#
# For four days after v0.5.28 published, README.md said both things at once. The
# generated block said:
#
#     Workspace [workspace.package].version (released): v0.5.28
#
# and forty lines below, a hand-written table said:
#
#     | all 21 published mnemo-* crates ... | v0.5.27 | v0.5.28 (unreleased) |
#       one patch, the open release window |
#
# The README even admitted it: "The summary table above is written by hand and is
# a narrative of one moment." A hand-maintained mirror of a generated table is
# guaranteed to drift, and this one drifted within a day of the release it
# described.
#
# # Why this is not "every version literal must equal the workspace"
#
# That rule is tempting and wrong. The README legitimately cites past versions
# outside the generated markers, and those are TRUE STATEMENTS ABOUT THE PAST
# that must not be rewritten when the workspace moves:
#
#     "the seven satellite crates ... are in the tag walk as of `v0.5.26`"
#     "`mnemo-mcp-server` had stranded at `v0.5.23`, skipping `v0.5.24`"
#     "the published 0.4.4 package was run against a server built from `v0.5.26`"
#
# `crates/mnemo-cli/tests/readme_crates_version_matches_workspace.rs` already
# pins BARE in-band literals and deliberately exempts `v`-prefixed citations for
# exactly that reason — and the stale table slipped through it, because every
# version in that table was `v`-prefixed.
#
# So this gate targets the thing that actually drifts: a PRESENT-TENSE CLAIM
# ABOUT RELEASE STATE, written by hand, outside the generated markers. Those
# phrases are only ever correct for as long as the sentence is young.
#
#   check_readme_version_claims.sh
#   check_readme_version_claims.sh --self-test
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
README="$REPO_ROOT/README.md"

# Present-tense release-state assertions. Each is a claim whose truth depends on
# what happened to have shipped on the day it was typed.
#
# Deliberately NOT "unreleased" on its own: the README says "one patch behind an
# unreleased workspace" when describing what check_version_drift.sh DOES, which
# is a durable statement about a mechanism rather than a claim about today. The
# parenthesised "(unreleased)" form is the table-cell annotation that drifted.
BANNED_PHRASES=(
  "cut but not yet published"
  "not yet published"
  "(unreleased)"
  "open release window"
)

scan() {  # scan <file> -> prints offences, returns 1 if any
  local file="$1" failed=0
  # Strip the generated regions first: inside them these phrases are produced by
  # the generator from live data, and are correct by construction.
  local stripped
  stripped="$(python3 - "$file" <<'PY'
import sys
gen = False
for i, line in enumerate(open(sys.argv[1]), 1):
    if "BEGIN generated:" in line:
        gen = True
    if gen:
        if "END generated:" in line:
            gen = False
        # Keep line numbering by emitting a blank placeholder.
        print(f"{i}\t")
        continue
    print(f"{i}\t{line.rstrip()}")
PY
)"

  local phrase hits
  for phrase in "${BANNED_PHRASES[@]}"; do
    # -F: fixed string. Not a regex — "(unreleased)" has parens in it, and an
    # ERE would read them as a group and match the bare word, which is the
    # legitimate usage this gate must not touch.
    hits="$(grep -Fn -- "$phrase" <<<"$stripped" | grep -v $'\t$' || true)"
    if [ -n "$hits" ]; then
      failed=1
      echo "  hand-written release-state claim: \"$phrase\""
      # The leading field is the real README line number.
      printf '%s\n' "$hits" | sed 's/^[0-9]*://' | awk -F'\t' '{printf "      README.md:%s  %s\n", $1, substr($2,1,88)}'
    fi
  done

  # A hand-written table row comparing a version against crates.io or the
  # workspace is the SHAPE of the thing that drifted, even if worded differently
  # next time.
  hits="$(awk -F'\t' '
    $2 ~ /^\|/ && $2 ~ /v?[0-9]+\.[0-9]+\.[0-9]+/ && ($2 ~ /crates\.io/ || $2 ~ /workspace/) {
      printf "      README.md:%s  %s\n", $1, substr($2,1,88)
    }' <<<"$stripped")"
  if [ -n "$hits" ]; then
    failed=1
    echo "  hand-written version-comparison table row (the generated block owns this):"
    printf '%s\n' "$hits"
  fi

  return $failed
}

if [ "${1:-}" = "--self-test" ]; then
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  pass=0; fail=0
  expect() { # expect <want:pass|fail> <label> <file>
    local want="$1" label="$2" f="$3" got
    if scan "$f" >/dev/null 2>&1; then got=pass; else got=fail; fi
    if [ "$got" = "$want" ]; then echo "  ok   $label (expected $want)"; pass=$((pass+1))
    else echo "  FAIL $label (expected $want, got $got)"; fail=$((fail+1)); fi
  }

  echo "self-test:"
  expect pass "the real README as committed" "$README"

  # THE regression, verbatim from the commit this gate was written for.
  cp "$README" "$tmp/stale-table.md"
  cat >> "$tmp/stale-table.md" <<'EOF'
| what | crates.io | workspace | gap |
|---|---|---|---|
| all **21** published `mnemo-*` crates | `v0.5.27` | `v0.5.28` (unreleased) | one patch, the open release window |
EOF
  expect fail "the v0.5.28 stale table, verbatim" "$tmp/stale-table.md"

  cp "$README" "$tmp/cut.md"
  echo '> **Current release: `0.5.28`, cut but not yet published.**' >> "$tmp/cut.md"
  expect fail "a 'cut but not yet published' claim" "$tmp/cut.md"

  # A comparison row that does not use any banned phrase — caught by shape.
  cp "$README" "$tmp/shape.md"
  echo '| every crate | crates.io `v0.5.20` | workspace `v0.5.28` | behind |' >> "$tmp/shape.md"
  expect fail "a re-worded version-comparison row" "$tmp/shape.md"

  # Must NOT fire on the legitimate historical citations the README relies on,
  # nor on the mechanism sentence containing the bare word "unreleased".
  cp "$README" "$tmp/legit.md"
  cat >> "$tmp/legit.md" <<'EOF'
The satellites are in the tag walk as of `v0.5.26`, and `mnemo-mcp-server` had
stranded at `v0.5.23`, skipping `v0.5.24` entirely.
check_version_drift.sh treats one patch behind an unreleased workspace as a
publish in flight.
EOF
  expect pass "historical citations and the 'unreleased workspace' mechanism line" "$tmp/legit.md"

  # Inside a generated block the phrases are produced from live data.
  cp "$README" "$tmp/ingen.md"
  cat >> "$tmp/ingen.md" <<'EOF'
<!-- BEGIN generated: fake -->
| all crates | crates.io `v0.5.1` | workspace `v0.5.28` | (unreleased) |
<!-- END generated: fake -->
EOF
  expect pass "the same text INSIDE a generated block" "$tmp/ingen.md"

  echo
  echo "self-test: $pass passed, $fail failed"
  [ "$fail" -eq 0 ]
  exit $?
fi

echo "README release-state claims (outside the generated markers):"
if scan "$README"; then
  echo "  none — the generated blocks are the only place release state is asserted."
else
  echo
  echo "FAIL: README.md asserts release state by hand, outside the generated markers."
  echo "      The generated blocks already carry this correctly and are regenerated"
  echo "      from the live registries. Delete the hand-written claim rather than"
  echo "      updating it: a hand-maintained mirror drifts again on the next release."
  echo "      Regenerate with: python3 scripts/gen_published_versions.py"
  exit 1
fi
