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
drift_versions=()        # registry versions of drifting crates, for the grouped tally
drift_rows=()            # "crate|registry" for the drift-only recap

# Collect EVERY crate's verdict first (never exit on the first mismatch), then
# print one aligned table so an operator reading a red job sees repo vs registry
# side by side for all crates without cloning anything.
printf "\n  %-24s %-9s %-11s %s\n" "crate" "repo" "crates.io" "status"
printf "  %-24s %-9s %-11s %s\n" "------------------------" "---------" "-----------" "------"
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
    printf "  %-24s %-9s %-11s unpublished\n" "$crate" "$workspace_version" "-"
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
    printf "  %-24s %-9s %-11s ok\n" "$crate" "$workspace_version" "$published"
  else
    printf "  %-24s %-9s %-11s DRIFT >1 patch behind\n" "$crate" "$workspace_version" "$published"
    drift_versions+=("$published")
    drift_rows+=("${crate}|${published}")
    fail=1
  fi
done

if [[ ${#skipped[@]} -gt 0 ]]; then
  echo "  (unpublished, not on crates.io: ${skipped[*]})"
fi

if [[ "$fail" -ne 0 ]]; then
  echo ""
  echo "::error::version drift: published crates are more than one patch behind workspace ${workspace_version}."
  # Grouped tally — one line per stuck registry version, e.g.
  # "13 crate(s) stranded at 0.4.4". Portable (no associative arrays), so it
  # runs on macOS bash 3.2 as well as the CI ubuntu bash.
  echo "  stranded, grouped by the version they are stuck on:"
  printf '%s\n' "${drift_versions[@]}" | sort -rV | uniq -c | while read -r count ver; do
    printf "::error::  %s crate(s) stranded at %s (workspace is %s)\n" "$count" "$ver" "$workspace_version"
  done
  # Also drop a readable summary into the GitHub job page when running in CI.
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      echo "### ❌ Version drift — ${#drift_rows[@]} crate(s) behind workspace ${workspace_version}"
      echo
      echo "| crate | crates.io |"
      echo "|---|---|"
      for row in "${drift_rows[@]}"; do echo "| ${row%%|*} | ${row##*|} |"; done
      echo
      echo "Cut and publish the release (tag \`v${workspace_version}\`) to clear this. If the walk fails on the token, rotate \`CARGO_REGISTRY_TOKEN\`."
    } >> "$GITHUB_STEP_SUMMARY"
  fi
  exit 1
fi
echo "OK: all published crates are within one patch of workspace $workspace_version"
