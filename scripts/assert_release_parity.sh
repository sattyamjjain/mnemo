#!/usr/bin/env bash
#
# Release-parity assertion — HARD, no baseline.
#
# After a publish, every crate in the release closure MUST be at the workspace
# version on crates.io. The bug this exists to catch: mnemo-mcp-server sat
# stranded at 0.4.4 for MULTIPLE releases while the libraries moved to 0.5.x, and
# the release job reported success the whole time because nothing asserted that
# what the walk was supposed to publish actually landed. A partial publish that
# reports success is invisible; this makes it loud.
#
# Unlike scripts/check_version_drift.sh (baselined, so a known standing gap stays
# green and only NEW drift is red), this is deliberately un-baselined: it is meant
# to run AFTER a publish and fail the release if any closure crate did not reach
# the workspace version. Do not baseline it.
#
# Usage:
#   scripts/assert_release_parity.sh                 # checks the default closure
#   scripts/assert_release_parity.sh <crate> ...     # checks the given crates
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UA='mnemo-release-parity (https://github.com/sattyamjjain/mnemo)'

# Workspace version from [workspace.package].version — the single source.
ws="$(
  awk '
    /^\[workspace\.package\]/ { in_wp = 1; next }
    /^\[/                     { in_wp = 0 }
    in_wp && /^[[:space:]]*version[[:space:]]*=/ {
      gsub(/.*=[[:space:]]*"|".*/, ""); print; exit
    }
  ' "$REPO_ROOT/Cargo.toml"
)"
if [[ -z "$ws" ]]; then
  echo "::error::could not read [workspace.package].version from Cargo.toml"
  exit 2
fi

# Default to the release closure release-crate.yml publishes (topological order);
# a caller (the workflow) may pass an explicit list instead.
if [[ $# -gt 0 ]]; then
  crates=("$@")
else
  crates=(
    mnemo-core mnemo-graph mnemo-attention-state mnemo-compliance mnemo-mcp
    mnemo-postgres mnemo-rest mnemo-grpc mnemo-db mnemo-embeddings-bench
    mnemo-mcp-server
  )
fi

lag=()
printf 'release parity check — workspace = %s\n' "$ws"
for c in "${crates[@]}"; do
  v="$(curl -sSf -A "$UA" "https://crates.io/api/v1/crates/${c}" 2>/dev/null \
    | python3 -c 'import sys,json
try: print(json.load(sys.stdin)["crate"]["max_version"])
except Exception: print("")' 2>/dev/null || echo "")"
  if [[ "$v" == "$ws" ]]; then
    printf '  %-26s %s  ok\n' "$c" "$v"
  else
    printf '  %-26s %s  LAGS (workspace %s)\n' "$c" "${v:-absent}" "$ws"
    lag+=("${c}: crates.io=${v:-absent} != workspace=${ws}")
  fi
done

# ---------------------------------------------------------------------------
# Git tag parity (HARD). The tag being released must name the workspace version.
# In the release job the tag is $GITHUB_REF_NAME (e.g. v0.5.23); locally, fall
# back to the newest tag. A tag that names a different version than the workspace
# means the release is mislabeled. If neither is available, skip with a note.
# ---------------------------------------------------------------------------
tag_raw="${GITHUB_REF_NAME:-$(git -C "$REPO_ROOT" describe --tags --abbrev=0 2>/dev/null || echo "")}"
tag="${tag_raw#v}"
if [[ -z "$tag_raw" ]]; then
  printf '  %-26s %s\n' "git tag" "(none in context — skipped)"
elif [[ "$tag" == "$ws" ]]; then
  printf '  %-26s %s  ok\n' "git tag" "$tag_raw"
else
  printf '  %-26s %s  MISMATCH (workspace %s)\n' "git tag" "$tag_raw" "$ws"
  lag+=("git tag=${tag_raw} != workspace=${ws}")
fi

# ---------------------------------------------------------------------------
# Independently-versioned SDK artifacts — the PyPI `mnemo-db` (Python SDK) and
# the npm `@mndfreek/mnemo-sdk` (TypeScript SDK). Per python/pyproject.toml these
# version INDEPENDENTLY of the Rust workspace, so we do NOT force them equal to
# ${ws}. Instead assert the documented relationship — the repo's own manifest
# version for each artifact must agree with what is actually published for THAT
# artifact — and print all three numbers so the independence is visible.
#
#   manifest == registry   -> ok (published state matches the repo)
#   manifest >  registry    -> pending (an unpublished bump; WARN, do not fail —
#                              this is the normal state between a bump and its
#                              publish, exactly like the Rust workspace sitting
#                              ahead of crates.io before a release)
#   manifest <  registry    -> DRIFT (the registry is AHEAD of the repo: a
#                              publish happened that main never recorded, or the
#                              manifest regressed) -> FAIL
#
# `cmp_semver` prints -1/0/1 for manifest vs registry using dotted-numeric
# comparison (release lines here are plain x.y.z), tolerating a missing value.
warn=()
cmp_semver() {
  python3 - "$1" "$2" <<'PY'
import sys
def parts(v):
    out=[]
    for p in (v or "").split("."):
        n=""
        for ch in p:
            if ch.isdigit(): n+=ch
            else: break
        out.append(int(n) if n else 0)
    return out
a,b=parts(sys.argv[1]),parts(sys.argv[2])
n=max(len(a),len(b)); a+=[0]*(n-len(a)); b+=[0]*(n-len(b))
print(-1 if a<b else (1 if a>b else 0))
PY
}

check_sdk_artifact() {
  # $1 label  $2 manifest_version  $3 registry_version(or "absent")
  local label="$1" manifest="$2" registry="$3"
  if [[ -z "$registry" || "$registry" == "absent" ]]; then
    printf '  %-26s manifest=%s registry=absent  DRIFT (never published)\n' "$label" "$manifest"
    lag+=("${label}: manifest=${manifest} but registry has no release")
    return
  fi
  local rel; rel="$(cmp_semver "$manifest" "$registry")"
  if [[ "$rel" == "0" ]]; then
    printf '  %-26s manifest=%s == registry=%s  ok (workspace %s, independent)\n' "$label" "$manifest" "$registry" "$ws"
  elif [[ "$rel" == "1" ]]; then
    printf '  %-26s manifest=%s > registry=%s  PENDING publish (workspace %s)\n' "$label" "$manifest" "$registry" "$ws"
    warn+=("${label}: manifest ${manifest} is ahead of the published ${registry} — an unpublished bump; publish it to reconcile.")
  else
    printf '  %-26s manifest=%s < registry=%s  DRIFT (registry ahead)\n' "$label" "$manifest" "$registry"
    lag+=("${label}: registry ${registry} is AHEAD of manifest ${manifest} — a publish main never recorded.")
  fi
}

# PyPI: mnemo-db (Python SDK). Manifest = python/pyproject.toml [project].version.
pyproject_ver="$(
  awk '
    /^\[project\]/ { in_p = 1; next }
    /^\[/          { in_p = 0 }
    in_p && /^[[:space:]]*version[[:space:]]*=/ {
      gsub(/.*=[[:space:]]*"|".*/, ""); print; exit
    }
  ' "$REPO_ROOT/python/pyproject.toml"
)"
pypi_ver="$(curl -sSf "https://pypi.org/pypi/mnemo-db/json" 2>/dev/null \
  | python3 -c 'import sys,json
try: print(json.load(sys.stdin)["info"]["version"])
except Exception: print("")' 2>/dev/null || echo "")"
check_sdk_artifact "mnemo-db (PyPI)" "${pyproject_ver:-unknown}" "${pypi_ver:-absent}"

# npm: @mndfreek/mnemo-sdk (TypeScript SDK). Manifest = sdks/typescript/package.json.
npm_manifest="$(python3 -c 'import json;print(json.load(open("'"$REPO_ROOT"'/sdks/typescript/package.json"))["version"])' 2>/dev/null || echo "")"
npm_ver="$(curl -sSf "https://registry.npmjs.org/@mndfreek%2Fmnemo-sdk" 2>/dev/null \
  | python3 -c 'import sys,json
try: print(json.load(sys.stdin)["dist-tags"]["latest"])
except Exception: print("")' 2>/dev/null || echo "")"
check_sdk_artifact "@mndfreek/mnemo-sdk (npm)" "${npm_manifest:-unknown}" "${npm_ver:-absent}"

# ---------------------------------------------------------------------------
# Verdict.
# ---------------------------------------------------------------------------
if [[ ${#warn[@]} -gt 0 ]]; then
  for w in "${warn[@]}"; do echo "::warning::${w}"; done
fi

if [[ ${#lag[@]} -gt 0 ]]; then
  echo "::error::release parity FAILED — ${#lag[@]} artifact(s) drift from the release. A publish that leaves a member behind (or a registry ahead of main) is the exact silent failure that stranded mnemo-mcp-server at 0.4.4; reconcile every artifact."
  for l in "${lag[@]}"; do echo "::error::  ${l}"; done
  {
    echo "## ❌ Release parity — drift detected (workspace \`${ws}\`)"
    echo
    for l in "${lag[@]}"; do echo "- ❌ \`${l}\`"; done
    for w in "${warn[@]:-}"; do [[ -n "$w" ]] && echo "- ⚠️ ${w}"; done
    echo
    echo "The Rust closure crates and the git tag must reach \`${ws}\` on crates.io. The independently-versioned SDK artifacts (PyPI \`mnemo-db\`, npm \`@mndfreek/mnemo-sdk\`) must match their OWN published version — a registry ahead of the repo manifest is drift; an unpublished bump is only a warning."
  } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit 1
fi

echo "release parity OK: ${#crates[@]} crates.io closure crate(s) + git tag at ${ws}; SDK artifacts consistent with their own registries (${#warn[@]} pending-publish warning(s))."
