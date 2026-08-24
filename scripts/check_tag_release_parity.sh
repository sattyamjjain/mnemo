#!/usr/bin/env bash
# Every release tag must have a GitHub Release object.
#
# v0.5.23, v0.5.24 and v0.5.25 all shipped with no Release. The automation did
# not break — it never existed, and nothing anywhere went red about it. A tag
# with no Release is a version users cannot find release notes for, and the
# only signal was someone happening to look at the Releases page.
#
# `release-crate.yml` now creates the Release from the CHANGELOG section. This
# is the assertion that keeps it true: if that job is removed, disabled, or
# silently skipped for a tag, this goes red instead of nothing going red.
#
# Ported from ferrumdeck's `tag-has-release` guard, which does the same job in
# that repo after its release history fell three versions behind its tags.
#
#   bash scripts/check_tag_release_parity.sh
#   bash scripts/check_tag_release_parity.sh --self-test
set -euo pipefail

# Tags at or after this one must have a Release. Nine tags predate the
# release-crate.yml `github_release` job and were never given one:
#
#   v0.3.3 v0.3.4 v0.4.0-rc1 v0.4.2 v0.4.3 v0.5.0 v0.5.1 v0.5.2 v0.5.3
#
# They are listed here rather than skipped silently, and they are printed on
# every run so they stay visible. This is a dated boundary with a stated
# reason, not an open-ended allowlist: every tag from the cutoff forward is
# checked, so a NEW tag can never land in the exempt set.
CUTOFF="v0.5.4"

# Sorts semver-ish tags so `v0.5.9` < `v0.5.10`. `sort -V` handles this; the
# `-rc` suffixes sort before their release, which is what we want.
tag_lt() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$1" ] && [ "$1" != "$2" ]; }

check() {
  local tags="$1" releases="$2" missing="" skipped=""
  while IFS= read -r tag; do
    [ -z "$tag" ] && continue
    if grep -Fxq "$tag" <<<"$releases"; then
      continue
    fi
    # Only tags that ACTUALLY lack a Release are worth naming. Reporting every
    # pre-cutoff tag as "no Release" regardless would be false: v0.4.14 and
    # v0.4.15 predate the cutoff and do have one.
    if tag_lt "$tag" "$CUTOFF"; then
      skipped="$skipped $tag"
    else
      missing="$missing $tag"
    fi
  done <<<"$tags"

  if [ -n "$skipped" ]; then
    echo "  pre-$CUTOFF tags with no Release (known, predate the automation):$skipped"
  fi
  if [ -n "$missing" ]; then
    echo "::error::These tags at or after $CUTOFF have no GitHub Release:$missing"
    echo "      release-crate.yml's \`github_release\` job creates it from the"
    echo "      CHANGELOG section for the tag. If that job did not run, re-run the"
    echo "      release workflow for the tag, or create it by hand:"
    echo "        python3 scripts/extract_release_notes.py <version> > notes.md"
    echo "        gh release create <tag> --notes-file notes.md"
    return 1
  fi
  echo "  every tag at or after $CUTOFF has a GitHub Release."
  return 0
}

if [ "${1:-}" = "--self-test" ]; then
  fails=0
  expect() { # name expected_rc tags releases
    local name="$1" want="$2" out rc
    out="$(check "$3" "$4" 2>&1)" && rc=0 || rc=$?
    if [ "$rc" -eq "$want" ]; then echo "  ok   $name"; else
      echo "  FAIL $name (rc=$rc want=$want)"; printf '%s\n' "$out" | sed 's/^/       /'; fails=1
    fi
  }
  echo "self-test:"
  expect "all post-cutoff tags released" 0 \
    "$(printf 'v0.5.4\nv0.5.26\n')" "$(printf 'v0.5.4\nv0.5.26\n')"
  # THE regression: a tag shipped with no Release. This is v0.5.23/24/25.
  expect "a post-cutoff tag with no Release fails" 1 \
    "$(printf 'v0.5.4\nv0.5.25\n')" "$(printf 'v0.5.4\n')"
  expect "pre-cutoff tags are exempt, not failures" 0 \
    "$(printf 'v0.3.3\nv0.5.0\nv0.5.26\n')" "$(printf 'v0.5.26\n')"
  # The cutoff itself is INSIDE the checked set, not outside it.
  expect "the cutoff tag itself is checked" 1 \
    "$(printf 'v0.5.4\n')" ""
  # Numeric, not lexical: v0.5.10 must not sort below v0.5.4 and get exempted.
  expect "v0.5.10 is not exempted by string comparison" 1 \
    "$(printf 'v0.5.10\n')" ""
  expect "no tags at all is not a failure" 0 "" ""
  # A pre-cutoff tag that DOES have a Release must not be reported as missing
  # one; the exemption list is for tags genuinely without one.
  out="$(check "$(printf 'v0.4.14\nv0.5.26\n')" "$(printf 'v0.4.14\nv0.5.26\n')" 2>&1)"
  if grep -q "v0.4.14" <<<"$out"; then
    echo "  FAIL released pre-cutoff tag wrongly listed as missing"; echo "$out" | sed 's/^/       /'; fails=1
  else
    echo "  ok   released pre-cutoff tag is not listed as missing"
  fi
  [ "$fails" -eq 0 ] && { echo "self-test passed"; exit 0; }
  echo "self-test FAILED" >&2; exit 1
fi

command -v gh >/dev/null || { echo "gh CLI required"; exit 1; }
tags="$(git tag --list 'v*' | sort -V -u)"
if [ -z "$tags" ]; then
  echo "No tags — nothing to check."
  exit 0
fi
releases="$(gh release list --limit 200 --json tagName -q '.[].tagName' | sort -u)"
echo "tag/release parity (cutoff $CUTOFF):"
check "$tags" "$releases"
