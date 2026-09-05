#!/usr/bin/env bash
#
# CITATION.cff must name the version this repo actually ships.
#
# GitHub renders CITATION.cff as the "Cite this repository" widget, so whatever
# `version:` says is what people paste into reference lists. A citation naming
# the wrong version is a wrong citation, and nothing else in the tree reads this
# file, so nothing else can notice when it rots.
#
# This is not hypothetical. The sibling agent-audit-kit repository carried the
# same two fields under a comment instructing a human to bump them each release.
# That instruction was followed until it wasn't, and the file sat at 0.3.83 while
# the repo shipped 0.3.93 — ten patch releases of silently wrong citation
# metadata. The fix there was to generate the fields and gate them. This is the
# gate for mnemo.
#
# Checked:
#   version:        == [workspace.package].version in Cargo.toml
#   date-released:  present and ISO-8601 (YYYY-MM-DD)
#   doi:            ABSENT. Mnemo has no Zenodo deposit. A placeholder DOI is a
#                   broken link that looks authoritative and gets copied into
#                   bibliographies before anyone resolves it. If a deposit is
#                   ever made, delete this check in the same commit that adds
#                   the real DOI — deliberately, not by accident.
#
# Usage: check_citation_version.sh [--self-test]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Overridable so --self-test can drive fixtures through the real check body.
CFF="${CFF:-$REPO_ROOT/CITATION.cff}"

cff_field() { # $1 = field name, $2 = file
  sed -n "s/^${1}:[[:space:]]*[\"']\{0,1\}\([^\"']*\)[\"']\{0,1\}[[:space:]]*$/\1/p" "$2" | head -1
}

# --- self-test: prove each assertion can actually fail ----------------------
# A guard that cannot fail is not coverage, it is confidence. Every check below
# is driven against a fixture that violates it.
if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  st=0
  probe() { # $1 = label, $2 = file body, $3 = expect (pass|fail)
    printf '%s\n' "$2" > "$tmp/CITATION.cff"
    if CFF="$tmp/CITATION.cff" SELFTEST_WS="0.5.29" bash "${BASH_SOURCE[0]}" --internal-check >/dev/null 2>&1; then
      got=pass; else got=fail; fi
    if [[ "$got" == "$3" ]]; then printf '  ok    %-42s -> %s\n' "$1" "$got"
    else printf '  FAIL  %-42s -> %s (expected %s)\n' "$1" "$got" "$3"; st=1; fi
  }
  good='cff-version: 1.2.0
version: "0.5.29"
date-released: "2026-09-04"'
  probe "matching version"                "$good" pass
  probe "version behind the workspace"    'cff-version: 1.2.0
version: "0.5.28"
date-released: "2026-09-04"'                     fail
  probe "date-released missing"           'cff-version: 1.2.0
version: "0.5.29"'                               fail
  probe "date-released not ISO-8601"      'cff-version: 1.2.0
version: "0.5.29"
date-released: "Sept 4 2026"'                    fail
  probe "a placeholder DOI was added"     'cff-version: 1.2.0
version: "0.5.29"
date-released: "2026-09-04"
doi: 10.5281/zenodo.0000000'                     fail
  [[ $st -ne 0 ]] && { echo "::error::check_citation_version.sh --self-test FAILED — a check no longer fires" >&2; exit 1; }
  echo "check_citation_version.sh --self-test OK (every assertion fails when violated)"
  exit 0
fi

# --- the checks -------------------------------------------------------------
[[ "${1:-}" == "--internal-check" ]] || true   # same body either way

if [[ ! -f "$CFF" ]]; then
  echo "::error::CITATION.cff is missing from the repo root" >&2; exit 1
fi

ws="${SELFTEST_WS:-}"
if [[ -z "$ws" ]]; then
  ws="$(awk '
    /^\[workspace\.package\]/ { in_wp = 1; next }
    /^\[/                     { in_wp = 0 }
    in_wp && /^[[:space:]]*version[[:space:]]*=/ { gsub(/.*=[[:space:]]*"|".*/, ""); print; exit }
  ' "$REPO_ROOT/Cargo.toml")"
fi
[[ -n "$ws" ]] || { echo "::error::could not read [workspace.package].version" >&2; exit 2; }

rc=0
cff_ver="$(cff_field version "$CFF")"
if [[ "$cff_ver" != "$ws" ]]; then
  echo "::error::CITATION.cff says version \"${cff_ver:-<absent>}\" but [workspace.package].version is \"${ws}\". GitHub renders this file as the 'Cite this repository' widget, so this is the version people will cite. Update CITATION.cff in the release commit." >&2
  rc=1
fi

cff_date="$(cff_field date-released "$CFF")"
if [[ ! "$cff_date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "::error::CITATION.cff date-released is \"${cff_date:-<absent>}\" — CFF 1.2.0 requires an ISO-8601 date (YYYY-MM-DD)." >&2
  rc=1
fi

if grep -qE '^doi:' "$CFF"; then
  echo "::error::CITATION.cff declares a doi:. Mnemo has no Zenodo deposit, and a placeholder DOI is a broken link that GitHub prints as authoritative. If a real deposit now exists, remove this check in the same commit that adds the DOI." >&2
  rc=1
fi

[[ $rc -eq 0 ]] && echo "CITATION.cff OK (version ${ws}, released ${cff_date}, no DOI claimed)"
exit $rc
