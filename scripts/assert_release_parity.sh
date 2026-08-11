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

if [[ ${#lag[@]} -gt 0 ]]; then
  echo "::error::release parity FAILED — ${#lag[@]} closure crate(s) do not match workspace ${ws}. A publish that leaves a member behind is the exact silent failure that stranded mnemo-mcp-server; fix the publish (or the token scope) so every closure crate reaches ${ws}."
  for l in "${lag[@]}"; do echo "::error::  ${l}"; done
  {
    echo "## ❌ Release parity — closure members lag the workspace \`${ws}\`"
    echo
    for l in "${lag[@]}"; do echo "- \`${l}\`"; done
    echo
    echo "Every crate the release publishes must reach the workspace version. A lagging member means the publish did not fully land."
  } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit 1
fi

echo "release parity OK: all ${#crates[@]} closure crate(s) are at ${ws} on crates.io."
