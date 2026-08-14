#!/usr/bin/env bash
#
# Registry parity — the SINGLE implementation of "what does crates.io actually
# have, and does it agree with this repo".
#
# ---------------------------------------------------------------------------
# Why this file exists (issue #140)
# ---------------------------------------------------------------------------
# `mnemo-mcp-server` — the crate a stranger installs to get the `mnemo` binary —
# sat at 0.4.4 on crates.io for 87 days (published 2026-05-18) while the library
# crates advanced to 0.5.22. Every publish run in that window reported SUCCESS.
#
# It reported success because the publish walk *enumerated* crates and *skipped*
# the ones it could not publish, and a skip was indistinguishable from a no-op.
# That is a CI design bug independent of the rejected CARGO_REGISTRY_TOKEN that
# caused it: a release pipeline whose failure mode is silence will strand a crate
# again the next time anything goes wrong, for any reason.
#
# So: before publishing anything, print the full triple for every publishable
# crate, and refuse to proceed on a lag nobody is fixing. After publishing,
# assert every crate in the walk actually landed.
#
# ---------------------------------------------------------------------------
# Modes
# ---------------------------------------------------------------------------
#   --mode preflight   Run BEFORE the publish walk. Prints, for every publishable
#                      crate, the triple (workspace version, newest git tag,
#                      crates.io version). FAILS when a crate lags the workspace
#                      by more than one patch release AND is not in the walk that
#                      is about to run (--walk). See "the deadlock" below.
#
#   --mode assert      Run AFTER the publish walk. Every crate named must now be
#                      AT the workspace version on crates.io. A crate still
#                      behind means the walk skipped it — the exact silent
#                      failure this file exists to make loud. Hard, no baseline.
#
# ---------------------------------------------------------------------------
# The deadlock, and why preflight is walk-aware
# ---------------------------------------------------------------------------
# A naive "fail if any crate lags" preflight is unshippable: mnemo-mcp-server
# lags at 0.4.4 RIGHT NOW, so such a gate would block the very publish that
# repairs it, forever. The gate is therefore walk-aware:
#
#   crate lags AND is in the walk      -> loud REPAIRING line, allowed to proceed
#   crate lags AND is NOT in the walk  -> FAIL, named, with both versions
#
# That keeps the property the issue actually wants ("a lag can never be silent")
# without making the repair impossible. With no --walk (a bare local run) every
# lag is reported and the run fails — which is the right answer for a human
# asking "is the registry in sync?".
#
# ---------------------------------------------------------------------------
# Division of labour with the other version guards (do not merge these)
# ---------------------------------------------------------------------------
#   scripts/registry_parity.sh          ONLINE. repo vs crates.io/npm/PyPI.
#   crates/mnemo-cli/tests/
#     workspace_version_fence.rs        OFFLINE. Cargo.toml vs git tag vs
#                                       CHANGELOG, every crate. Runs in
#                                       `cargo test`, so a mismatch cannot merge.
#   scripts/check_version_drift.sh      ONLINE, BASELINED. Watches the standing
#                                       drift for getting WORSE without going red
#                                       on every push while #140 is open.
#
# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------
#   scripts/registry_parity.sh --mode preflight
#   scripts/registry_parity.sh --mode preflight --walk "mnemo-core mnemo-mcp"
#   scripts/registry_parity.sh --mode assert mnemo-core mnemo-mcp-server
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UA='mnemo-registry-parity (https://github.com/sattyamjjain/mnemo)'

MODE=""
WALK=""
ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="${2:-}"; shift 2 ;;
    --walk) WALK="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,80p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) ARGS+=("$1"); shift ;;
  esac
done
if [[ "$MODE" != "preflight" && "$MODE" != "assert" ]]; then
  echo "::error::registry_parity.sh: --mode must be 'preflight' or 'assert' (got '${MODE:-}')" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Workspace version — [workspace.package].version, the single source.
# ---------------------------------------------------------------------------
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
  echo "::error::could not read [workspace.package].version from Cargo.toml" >&2
  exit 2
fi

# Newest git tag (the third leg of the triple). In a tag-triggered workflow run
# GITHUB_REF_NAME is the tag being released; locally, fall back to the newest
# reachable tag. Empty in a shallow checkout with no tags — reported as such
# rather than silently treated as agreement.
newest_tag="${GITHUB_REF_NAME:-}"
if [[ "$newest_tag" != v* ]]; then
  newest_tag="$(git -C "$REPO_ROOT" describe --tags --abbrev=0 2>/dev/null || echo "")"
fi
: "${newest_tag:=none}"

# ---------------------------------------------------------------------------
# Publishable crate enumeration.
#
# Publishable = a Cargo.toml under crates/ or bench/ that does NOT set
# `publish = false`, minus a documented deny-list of crates that are real
# members but are deliberately never crates.io artifacts:
#
#   mnemo-python     PyO3 cdylib; built by maturin and published to PyPI as
#                    `mnemo-db`, never to crates.io.
#   mnemo-golem-wit  cdylib WASM component; in the root `[workspace] exclude`,
#                    built standalone with cargo-component. Not a crates.io
#                    artifact. (Its version is fenced OFFLINE instead — see
#                    workspace_version_fence.rs, which is what caught it sitting
#                    at 0.5.21 while the workspace was 0.5.23.)
#   mnemo-golem-host wasmtime runner for the above; same story.
# ---------------------------------------------------------------------------
DENY=" mnemo-python mnemo-golem-wit mnemo-golem-host "

enumerate_publishable() {
  local manifest name
  for manifest in "$REPO_ROOT"/crates/*/Cargo.toml "$REPO_ROOT"/bench/*/Cargo.toml; do
    [[ -f "$manifest" ]] || continue
    grep -qE '^[[:space:]]*publish[[:space:]]*=[[:space:]]*false' "$manifest" && continue
    name="$(awk -F'"' '/^\[package\]/{p=1} p&&/^[[:space:]]*name[[:space:]]*=/{print $2; exit}' "$manifest")"
    [[ -n "$name" ]] || continue
    case "$DENY" in *" $name "*) continue ;; esac
    echo "$name"
  done
}

# crates.io max_version for a crate, or "absent". Retries transient answers so a
# registry blip is never mistaken for "this crate is missing" — mistaking a blip
# for absence is how a guard starts lying.
registry_version() {
  local crate="$1" json="" attempt
  for attempt in 1 2 3; do
    if json="$(curl -sS -A "$UA" --max-time 20 \
        "https://crates.io/api/v1/crates/${crate}" 2>/dev/null)"; then
      local v
      v="$(printf '%s' "$json" | python3 -c 'import sys,json
try:
    d = json.load(sys.stdin)
    if "errors" in d: print("absent")
    else: print(d["crate"]["max_version"])
except Exception:
    print("")' 2>/dev/null || echo "")"
      [[ -n "$v" ]] && { echo "$v"; return 0; }
    fi
    sleep $((attempt * 3))
  done
  echo "unreachable"
}

# semver compare: prints -1 / 0 / 1 for $1 vs $2.
cmp_semver() {
  python3 - "$1" "$2" <<'PY'
import sys
def parts(v):
    core = (v or "0").split("+", 1)[0].split("-", 1)[0]
    out = []
    for p in core.split("."):
        n = ""
        for ch in p:
            if ch.isdigit(): n += ch
            else: break
        out.append(int(n) if n else 0)
    return (out + [0, 0, 0])[:3]
a, b = parts(sys.argv[1]), parts(sys.argv[2])
print(-1 if a < b else (1 if a > b else 0))
PY
}

# True when `published` is MORE than one patch behind `workspace`. One patch of
# slack is deliberate: between a bump and its publish the repo is legitimately
# one ahead, and failing on that would make the gate red on every release PR.
more_than_one_patch_behind() {
  python3 - "$1" "$2" <<'PY'
import sys
def parts(v):
    core = (v or "0").split("+", 1)[0].split("-", 1)[0]
    out = []
    for p in core.split("."):
        n = ""
        for ch in p:
            if ch.isdigit(): n += ch
            else: break
        out.append(int(n) if n else 0)
    return (out + [0, 0, 0])[:3]
w, p = parts(sys.argv[1]), parts(sys.argv[2])
if p >= w:                      sys.exit(1)   # not behind
if w[0] == p[0] and w[1] == p[1] and (w[2] - p[2]) <= 1: sys.exit(1)
sys.exit(0)                     # behind by more than one patch
PY
}

in_walk() { case " $WALK " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
if [[ ${#ARGS[@]} -gt 0 ]]; then
  crates=("${ARGS[@]}")
else
  mapfile -t crates < <(enumerate_publishable)
fi
if [[ ${#crates[@]} -eq 0 ]]; then
  echo "::error::no publishable crates enumerated — the enumeration is broken, not the registry" >&2
  exit 2
fi

echo "registry parity — mode=${MODE}"
echo "  workspace version : ${ws}"
echo "  newest git tag    : ${newest_tag}"
[[ -n "$WALK" ]] && echo "  publish walk      : ${WALK}"
echo
printf '  %-30s %-11s %-11s %-11s %s\n' "crate" "workspace" "git tag" "crates.io" "status"
printf '  %-30s %-11s %-11s %-11s %s\n' "------------------------------" "-----------" "-----------" "-----------" "------"

fail=()      # hard failures
repairing=() # lagging crates that this walk is about to fix
orphans=()   # publishable, never published, and in no walk -> can never ship
unreachable=0

for c in "${crates[@]}"; do
  rv="$(registry_version "$c")"
  status=""
  case "$rv" in
    unreachable)
      status="UNREACHABLE (crates.io did not answer)"
      unreachable=1
      fail+=("${c}: crates.io unreachable after retries — refusing to treat an unknown registry state as agreement")
      ;;
    absent)
      if [[ "$MODE" == "assert" ]]; then
        status="ABSENT — never published"
        fail+=("${c}: absent from crates.io after the publish walk (workspace ${ws})")
      elif in_walk "$c"; then
        status="absent — this walk CREATES it (needs publish-new)"
        repairing+=("${c}: absent -> ${ws}")
      else
        # Publishable, never published, and in no publish walk. Not a hard
        # failure (a freshly added crate legitimately looks like this for one
        # release), but it is the #140 shape: nothing will ever ship it, and
        # nothing would ever say so. Warn by name.
        status="absent, in NO walk — will never publish"
        orphans+=("$c")
      fi
      ;;
    *)
      if [[ "$(cmp_semver "$rv" "$ws")" == "0" ]]; then
        status="ok"
      elif [[ "$MODE" == "assert" ]]; then
        status="LAGS — still ${rv} after the walk"
        fail+=("${c}: crates.io=${rv} != workspace=${ws} after the publish walk")
      elif more_than_one_patch_behind "$ws" "$rv"; then
        if in_walk "$c"; then
          status="LAGS ${rv} -> this walk repairs it"
          repairing+=("${c}: ${rv} -> ${ws}")
        else
          status="LAGS ${rv} — NOT in this walk"
          fail+=("${c}: crates.io=${rv} is more than one patch behind workspace=${ws}, and this walk does not publish it")
        fi
      else
        status="behind by <=1 patch (pending publish)"
      fi
      ;;
  esac
  printf '  %-30s %-11s %-11s %-11s %s\n' "$c" "$ws" "$newest_tag" "$rv" "$status"
done

# ---------------------------------------------------------------------------
# Independently-versioned SDK artifacts. These do NOT track the Rust workspace
# version (python/pyproject.toml and sdks/typescript/package.json say so), so
# they are reported for visibility and checked only against THEIR OWN registry:
# a registry AHEAD of the repo manifest is drift (a publish main never recorded);
# a manifest ahead of the registry is just an unpublished bump.
# ---------------------------------------------------------------------------
echo
printf '  %-30s %-13s %-13s %s\n' "sdk artifact" "manifest" "registry" "status"
printf '  %-30s %-13s %-13s %s\n' "------------------------------" "-------------" "-------------" "------"

check_sdk() {
  local label="$1" manifest="$2" registry="$3"
  if [[ -z "$registry" || "$registry" == "absent" ]]; then
    printf '  %-30s %-13s %-13s %s\n' "$label" "$manifest" "absent" "never published"
    return
  fi
  local rel; rel="$(cmp_semver "$manifest" "$registry")"
  if [[ "$rel" == "0" ]]; then
    printf '  %-30s %-13s %-13s %s\n' "$label" "$manifest" "$registry" "ok (independent of workspace ${ws})"
  elif [[ "$rel" == "1" ]]; then
    printf '  %-30s %-13s %-13s %s\n' "$label" "$manifest" "$registry" "pending publish (warn only)"
  else
    printf '  %-30s %-13s %-13s %s\n' "$label" "$manifest" "$registry" "DRIFT — registry ahead of repo"
    fail+=("${label}: registry ${registry} is AHEAD of manifest ${manifest} — a publish main never recorded")
  fi
}

py_manifest="$(awk '
  /^\[project\]/ { in_p = 1; next }
  /^\[/          { in_p = 0 }
  in_p && /^[[:space:]]*version[[:space:]]*=/ { gsub(/.*=[[:space:]]*"|".*/, ""); print; exit }
' "$REPO_ROOT/python/pyproject.toml" 2>/dev/null || echo "")"
py_registry="$(curl -sS --max-time 20 "https://pypi.org/pypi/mnemo-db/json" 2>/dev/null \
  | python3 -c 'import sys,json
try: print(json.load(sys.stdin)["info"]["version"])
except Exception: print("absent")' 2>/dev/null || echo "absent")"
check_sdk "mnemo-db (PyPI)" "${py_manifest:-unknown}" "${py_registry}"

npm_name="$(python3 -c 'import json;print(json.load(open("'"$REPO_ROOT"'/sdks/typescript/package.json"))["name"])' 2>/dev/null || echo "")"
npm_manifest="$(python3 -c 'import json;print(json.load(open("'"$REPO_ROOT"'/sdks/typescript/package.json"))["version"])' 2>/dev/null || echo "")"
npm_registry="$(curl -sS --max-time 20 "https://registry.npmjs.org/${npm_name//\//%2F}" 2>/dev/null \
  | python3 -c 'import sys,json
try: print(json.load(sys.stdin)["dist-tags"]["latest"])
except Exception: print("absent")' 2>/dev/null || echo "absent")"
check_sdk "${npm_name:-@mndfreek/mnemo-sdk} (npm)" "${npm_manifest:-unknown}" "${npm_registry}"

# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------
echo
if [[ ${#repairing[@]} -gt 0 ]]; then
  echo "This walk REPAIRS ${#repairing[@]} lagging artifact(s):"
  for r in "${repairing[@]}"; do echo "  - ${r}"; done
  {
    echo "### Registry parity — ${#repairing[@]} lagging artifact(s) queued for repair"
    echo
    for r in "${repairing[@]}"; do echo "- \`${r}\`"; done
  } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
fi

if [[ ${#orphans[@]} -gt 0 && "$MODE" == "preflight" ]]; then
  echo "::warning::${#orphans[@]} publishable crate(s) are absent from crates.io and appear in NO publish walk, so no release will ever ship them: ${orphans[*]}. Either add them to the walk in release-crate.yml, or set publish = false so the intent is explicit. This is the same shape as issue #140 — a crate nobody publishes and nobody notices."
  {
    echo "### ⚠️ Orphaned publishable crates (in no walk)"
    echo
    for o in "${orphans[@]}"; do echo "- \`${o}\` — publishable, absent from crates.io, in no publish walk"; done
    echo
    echo "Add to the walk in \`release-crate.yml\`, or set \`publish = false\` to make the intent explicit."
  } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
fi

if [[ ${#fail[@]} -gt 0 ]]; then
  if [[ "$MODE" == "assert" ]]; then
    echo "::error::release parity FAILED — ${#fail[@]} artifact(s) did not reach ${ws}. A publish that reports success while leaving a member behind is the exact silent failure that stranded mnemo-mcp-server at 0.4.4 for 87 days (issue #140)."
  else
    echo "::error::publish preflight FAILED — ${#fail[@]} artifact(s) lag the workspace and this walk does not repair them. Add them to the walk or fix the registry state before publishing (issue #140)."
  fi
  for f in "${fail[@]}"; do echo "::error::  ${f}"; done
  {
    echo "## ❌ Registry parity (${MODE}) — ${#fail[@]} problem(s), workspace \`${ws}\`"
    echo
    for f in "${fail[@]}"; do echo "- ❌ \`${f}\`"; done
    echo
    if [[ $unreachable -eq 1 ]]; then
      echo "At least one failure is an UNREACHABLE registry, not a version lag. Re-run before concluding anything about the published state."
    fi
    echo "Rotating \`CARGO_REGISTRY_TOKEN\` is an **operator action** and cannot be done from CI — see [issue #140](https://github.com/sattyamjjain/mnemo/issues/140)."
  } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit 1
fi

echo "registry parity OK (${MODE}): ${#crates[@]} crate(s) checked against workspace ${ws}; SDK artifacts consistent with their own registries."
