#!/usr/bin/env bash
#
# Fail when the workspace version has drifted MORE THAN ONE PATCH ahead of the
# newest version published to crates.io. This is the forcing function that stops
# the tree from accumulating unpublished releases (the exact state that left
# v0.5.17 / v0.5.18 tagged-but-unpublished while crates.io sat at 0.5.16).
#
# Rule: let W = workspace [workspace.package] version, P = crates.io max_version
# of the canonical crate (mnemo-core).
#   * W <= P                      -> OK  (behind or in sync)
#   * W ahead, same major.minor   -> OK only if (W.patch - P.patch) <= 1
#   * W ahead by a minor or major -> FAIL (inherently > 1 patch ahead)
#
# Publish mnemo-core (and the rest of the release) to clear a failure.
set -euo pipefail

CANONICAL_CRATE="mnemo-core"
CARGO_TOML="${1:-Cargo.toml}"

# Workspace version from [workspace.package].version (awk: first `version =`
# line that appears after the [workspace.package] header).
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

# Newest published version on crates.io.
published_version="$(
  curl -sSf "https://crates.io/api/v1/crates/${CANONICAL_CRATE}" \
    -H 'User-Agent: mnemo-ci-version-drift (https://github.com/sattyamjjain/mnemo)' \
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["crate"]["max_version"])'
)"

if [[ -z "$published_version" ]]; then
  echo "::error::could not read crates.io max_version for ${CANONICAL_CRATE}"
  exit 2
fi

echo "workspace version : $workspace_version"
echo "crates.io ($CANONICAL_CRATE) : $published_version"

python3 - "$workspace_version" "$published_version" <<'PY'
import sys

def parse(v):
    core = v.split("+", 1)[0].split("-", 1)[0]
    parts = core.split(".")
    return tuple(int(x) for x in (parts + ["0", "0", "0"])[:3])

w = parse(sys.argv[1])
p = parse(sys.argv[2])

if w <= p:
    print(f"OK: workspace {sys.argv[1]} is in sync with / behind crates.io {sys.argv[2]}")
    sys.exit(0)

# w is ahead of p.
if w[0] == p[0] and w[1] == p[1]:
    delta = w[2] - p[2]
    if delta <= 1:
        print(f"OK: workspace {sys.argv[1]} is {delta} patch ahead of crates.io {sys.argv[2]}")
        sys.exit(0)
    print(f"::error::workspace {sys.argv[1]} is {delta} patches ahead of the newest "
          f"published mnemo-core {sys.argv[2]} (limit: 1). Publish the pending "
          f"release to crates.io, or roll the workspace version back.")
    sys.exit(1)

print(f"::error::workspace {sys.argv[1]} is a minor/major ahead of the newest "
      f"published mnemo-core {sys.argv[2]} — more than one patch of drift. Publish "
      f"the pending release to crates.io, or roll the workspace version back.")
sys.exit(1)
PY
