#!/usr/bin/env bash
#
# Fail when ANY published workspace member has drifted more than one patch behind
# the workspace version on crates.io. This is the forcing function that stops the
# tree from accumulating unpublished releases.
#
# Originally this guarded only mnemo-core — which is exactly how mnemo-postgres /
# mnemo-rest / mnemo-grpc / mnemo-graph silently sat at 0.4.4/0.4.5 (2 months
# stale, missing the v0.5.7 real pgvector ANN and v0.5.18 async VectorIndex work)
# while mnemo-core moved to 0.5.x and the guard stayed green. It now checks every
# publishable member.
#
# Rule, per crate: let W = workspace [workspace.package] version, P = that crate's
# crates.io max_version.
#   * crate not on crates.io yet  -> SKIP (nothing published to drift; reported)
#   * W <= P                      -> OK
#   * W ahead, same major.minor   -> OK only if (W.patch - P.patch) <= 1
#   * W ahead by a minor or major -> FAIL (inherently > 1 patch ahead)
# The script fails if ANY crate fails. Publish the pending release to clear it.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="${1:-$REPO_ROOT/Cargo.toml}"
UA='mnemo-ci-version-drift (https://github.com/sattyamjjain/mnemo)'

# Workspace version from [workspace.package].version.
workspace_version="$(
  awk '
    /^\[workspace\.package\]/ { in_wp = 1; next }
    /^\[/                     { in_wp = 0 }
    in_wp && /^[[:space:]]*version[[:space:]]*=/ {
      gsub(/.*=[[:space:]]*"|".*/, ""); print; exit
    }
  ' "$CARGO_TOML"
)"
if [[ -z "$workspace_version" ]]; then
  echo "::error::could not read [workspace.package].version from $CARGO_TOML"
  exit 2
fi
echo "workspace version : $workspace_version"

# Collect publishable crate package names: every crates/*/Cargo.toml whose
# [package] does not set `publish = false`. (The golem WASM cdylibs and the
# PyO3 python crate live under crates/ too but carry publish=false / are not on
# crates.io; unpublished crates are skipped below regardless.)
crate_names=()
for manifest in "$REPO_ROOT"/crates/*/Cargo.toml; do
  [[ -f "$manifest" ]] || continue
  if grep -qE '^[[:space:]]*publish[[:space:]]*=[[:space:]]*false' "$manifest"; then
    continue
  fi
  name="$(awk -F'"' '/^\[package\]/{p=1} p&&/^[[:space:]]*name[[:space:]]*=/{print $2; exit}' "$manifest")"
  [[ -n "$name" ]] && crate_names+=("$name")
done

if [[ ${#crate_names[@]} -eq 0 ]]; then
  echo "::error::no publishable crates found under $REPO_ROOT/crates"
  exit 2
fi

fail=0
skipped=()
for crate in "${crate_names[@]}"; do
  # crates.io max_version, or empty if the crate is not published yet.
  published="$(
    curl -sSf -A "$UA" "https://crates.io/api/v1/crates/${crate}" 2>/dev/null \
      | python3 -c 'import sys,json
try:
    print(json.load(sys.stdin)["crate"]["max_version"])
except Exception:
    pass' 2>/dev/null || true
  )"
  if [[ -z "$published" ]]; then
    skipped+=("$crate")
    continue
  fi

  # Per-crate drift verdict.
  verdict="$(
    python3 - "$workspace_version" "$published" <<'PY'
import sys
def parse(v):
    core = v.split("+", 1)[0].split("-", 1)[0]
    parts = core.split(".")
    return tuple(int(x) for x in (parts + ["0", "0", "0"])[:3])
w = parse(sys.argv[1]); p = parse(sys.argv[2])
if w <= p:
    print("OK")
elif w[0] == p[0] and w[1] == p[1] and (w[2] - p[2]) <= 1:
    print("OK")
else:
    print("DRIFT")
PY
  )"
  if [[ "$verdict" == "OK" ]]; then
    printf "  OK    %-24s crates.io %s\n" "$crate" "$published"
  else
    printf "::error::DRIFT %-20s crates.io %s is >1 patch behind workspace %s — publish it.\n" \
      "$crate" "$published" "$workspace_version"
    fail=1
  fi
done

if [[ ${#skipped[@]} -gt 0 ]]; then
  echo "  (not yet on crates.io, skipped: ${skipped[*]})"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "::error::one or more published crates have drifted more than one patch behind the workspace version."
  exit 1
fi
echo "OK: all published crates are within one patch of workspace $workspace_version"
