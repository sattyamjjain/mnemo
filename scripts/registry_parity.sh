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
#                      crates.io version), and reports every lag by name. It is
#                      deliberately hard to fail — see "where the teeth are" —
#                      UNLESS --fail-on-minor-lag is passed.
#
#   --fail-on-minor-lag  Preflight only. Adds a SEVERITY FLOOR: a crate that is
#                      behind at MINOR level (or absent entirely) and is NOT in
#                      this walk becomes a hard failure instead of a warning.
#                      Used by release-crate.yml, NOT by cargo-publish.yml —
#                      see "the severity floor" below for why the split matters.
#
#   --mode assert      Run AFTER the publish walk. Every crate named must now be
#                      AT the workspace version on crates.io. A crate still
#                      behind means the walk skipped it — the exact silent
#                      failure this file exists to make loud. Hard, no baseline.
#
# ---------------------------------------------------------------------------
# Where the teeth are, and two deadlocks that had to be avoided
# ---------------------------------------------------------------------------
# A naive "preflight fails if any crate lags" is unshippable twice over:
#
#   1. mnemo-mcp-server lags RIGHT NOW, so such a gate blocks the very publish
#      that would repair it — forever.
#   2. cargo-publish.yml (push-to-main, the LIBRARY path) deliberately excludes
#      mnemo-mcp-server; only release-crate.yml (the tag path) can publish it.
#      Failing the library path over that crate blocks the 0.5.23 libraries on a
#      crate they are not responsible for, which makes the registry WORSE and
#      couples two release paths that are independent by design. This is not
#      hypothetical: it happened on the first real 0.5.23 run.
#
# So preflight classifies rather than vetoes:
#
#   lags AND in the walk      -> loud REPAIRING line, proceed (it is being fixed)
#   lags AND not in the walk  -> loud ::warning:: by name, proceed (not ours)
#   absent AND in no walk     -> loud ::warning:: by name (nothing will ship it)
#   registry AHEAD of repo    -> FAIL (a publish main never recorded)
#   registry unreachable      -> FAIL (never treat an unknown state as agreement)
#
# The teeth live in `--mode assert`, which runs AFTER the walk and hard-fails if
# a crate the walk WAS responsible for did not land. That is the check that
# actually catches #140's silent skip. A pre-publish veto on an out-of-scope
# crate only blocks good releases; a post-publish assertion catches the bad ones.
#
# ---------------------------------------------------------------------------
# The severity floor (--fail-on-minor-lag)
# ---------------------------------------------------------------------------
# `--mode assert` closes the "walk skipped a crate it owned" hole. It does NOT
# close the hole that actually produced #140: a crate that is in NO walk. Nothing
# asserts over it, because no walk claimed it, so it warns forever and 87 days
# pass. `mnemo-embeddings-bench` was absent from every walk; `mnemo-mcp-server`
# was stranded behind it. Both only ever produced ::warning:: lines.
#
# So: pass --fail-on-minor-lag on the RELEASE path and an out-of-walk crate that
# is behind at minor level, or absent entirely, is a hard failure.
#
# THE THRESHOLD, AND WHY IT IS NOT "more than one minor version".
#
# The obvious spelling of this gate is "fail if a crate drifts by more than one
# minor version". Check it against the case it exists for: #140 was
# mnemo-mcp-server at 0.4.4 against a 0.5.22 workspace. That is exactly ONE minor
# behind. A `> 1 minor` gate would have watched the whole 87 days go by in
# silence — a guard that misses the incident it was written for is worse than no
# guard, because it also supplies confidence.
#
# The floor is therefore ANY minor-level lag: a crate behind by a whole minor (or
# a major) fails. Patch-level lag keeps the existing one-patch slack, because
# between a version bump and its publish the repo is legitimately ahead and a
# gate that reddens on every release PR gets disabled within a week.
#
#   workspace 0.5.24, registry 0.5.23  -> pass (one patch, publish in flight)
#   workspace 0.5.24, registry 0.5.22  -> pass at the floor (still patch-level;
#                                         the existing >1-patch warning fires)
#   workspace 0.5.22, registry 0.4.4   -> FAIL  (#140's exact shape)
#   workspace 0.5.22, absent           -> FAIL  (nothing will ever ship it)
#
# WHY ONLY THE RELEASE PATH. cargo-publish.yml (push-to-main, libraries) does not
# and must not publish mnemo-mcp-server; only the tag path does. Applying the
# floor there would block library releases on a crate that path cannot repair —
# the exact coupling documented above as deadlock #2, which already broke a real
# 0.5.23 run once. release-crate.yml's walk covers every publishable crate, so
# there is no crate it is blamed for and cannot fix, and an in-walk lag is still
# classified REPAIRING and proceeds. The floor can only fire on a crate that
# genuinely nothing will publish.
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
#   scripts/registry_parity.sh --mode preflight --walk "$WALK" --fail-on-minor-lag
#   scripts/registry_parity.sh --mode assert mnemo-core mnemo-mcp-server
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UA='mnemo-registry-parity (https://github.com/sattyamjjain/mnemo)'

MODE=""
WALK=""
FLOOR=0
SELFTEST=0
ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="${2:-}"; shift 2 ;;
    --walk) WALK="${2:-}"; shift 2 ;;
    --fail-on-minor-lag) FLOOR=1; shift ;;
    --self-test) SELFTEST=1; shift ;;
    -h|--help) sed -n '2,90p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) ARGS+=("$1"); shift ;;
  esac
done
# --self-test needs no --mode; it runs offline against the threshold table and
# exits. The block itself lives below, after minor_level_behind is defined.
if [[ "${SELFTEST:-0}" -eq 0 ]] && [[ "$MODE" != "preflight" && "$MODE" != "assert" ]]; then
  echo "::error::registry_parity.sh: --mode must be 'preflight' or 'assert' (got '${MODE:-}')" >&2
  exit 2
fi
# The floor is a preflight concept. `assert` is already unconditionally hard on
# every crate in the walk, so accepting the flag there would imply it loosens or
# changes something. Reject it rather than silently ignore it.
if [[ $FLOOR -eq 1 && "$MODE" != "preflight" ]]; then
  echo "::error::registry_parity.sh: --fail-on-minor-lag applies to --mode preflight only (--mode assert is already hard-failing)" >&2
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

# True when `published` is behind `workspace` at MINOR level or worse — a whole
# minor version, or a major. A patch-only lag is NOT minor-level, however many
# patches wide it is; that case keeps the softer >1-patch warning above.
#
# This is the severity floor's predicate. See "the severity floor" in the header
# for why it is not spelled "more than one minor" — #140 was exactly one minor
# behind, so a `> 1` threshold would have missed it entirely.
minor_level_behind() {
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
if p >= w:            sys.exit(1)   # level or ahead: not behind at all
if p[0] < w[0]:       sys.exit(0)   # a whole major behind
if p[1] < w[1]:       sys.exit(0)   # a whole minor behind  <- #140's shape
sys.exit(1)                         # same major+minor: patch-level only
PY
}

if [[ $SELFTEST -eq 1 ]]; then
  # Offline assertion of the severity floor's threshold, run in CI.
  #
  # A version-comparison predicate is exactly the kind of code that is written
  # once, is never exercised because the healthy path never trips it, and is
  # wrong when it finally matters. The rows below ARE the table in the header —
  # if someone "simplifies" the threshold to `> 1 minor`, the #140 row goes red
  # here instead of going quiet for 87 days in production.
  st_fail=0
  # fields: workspace  published  expect(yes = minor-level behind)  label
  while read -r w p expect label; do
    [[ -z "${w:-}" || "$w" == \#* ]] && continue
    if minor_level_behind "$w" "$p"; then got=yes; else got=no; fi
    if [[ "$got" == "$expect" ]]; then
      printf '  ok    %-8s vs %-8s -> %-3s  %s\n' "$w" "$p" "$got" "$label"
    else
      printf '  FAIL  %-8s vs %-8s -> %-3s (expected %s)  %s\n' "$w" "$p" "$got" "$expect" "$label"
      st_fail=1
    fi
  done <<'CASES'
0.5.24 0.5.23 no one patch behind, publish in flight
0.5.24 0.5.22 no two patches behind, still patch-level (the softer warning owns this)
0.5.23 0.5.23 no at parity
0.5.23 0.5.24 no registry ahead (the DRIFT check owns this)
0.5.22 0.4.4 yes #140 EXACTLY - one whole minor behind, must fail
0.6.0 0.5.23 yes one whole minor behind at a minor bump
1.0.0 0.5.23 yes a whole major behind
0.5.23 0.4.0 yes a whole minor behind at .0
CASES
  if [[ $st_fail -ne 0 ]]; then
    echo "::error::registry_parity.sh --self-test FAILED: the severity-floor threshold no longer matches its documented table. #140 was exactly ONE minor behind, so a '> 1 minor' threshold silently misses the incident this gate exists for." >&2
    exit 1
  fi
  echo "registry_parity.sh --self-test OK (severity-floor threshold matches its documented table)"
  exit 0
fi

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
if [[ $FLOOR -eq 1 ]]; then
  echo "  severity floor    : ON — an out-of-walk crate a whole minor behind (or absent) FAILS this release"
else
  echo "  severity floor    : off — lags outside this walk warn only"
fi
echo
printf '  %-30s %-11s %-11s %-11s %s\n' "crate" "workspace" "git tag" "crates.io" "status"
printf '  %-30s %-11s %-11s %-11s %s\n' "------------------------------" "-----------" "-----------" "-----------" "------"

fail=()      # hard failures
repairing=() # lagging crates that this walk is about to fix
orphans=()   # publishable, never published, and in no walk -> can never ship
stranded=()  # lagging, but not this walk's responsibility -> warn, never fail
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
      elif [[ $FLOOR -eq 1 ]]; then
        # Severity floor engaged (release path). Absent AND in no walk is the
        # strictly worst state there is: nothing will ever ship it. On the
        # release path that is not a warning, it is a broken release.
        status="ABSENT and in NO walk — severity floor"
        fail+=("${c}: absent from crates.io and in no publish walk — nothing will ever ship it (workspace ${ws})")
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
        elif [[ $FLOOR -eq 1 ]] && minor_level_behind "$ws" "$rv"; then
          # Severity floor engaged (release path). A crate a whole minor behind,
          # that this walk does not repair, is not a "warning" — it is the #140
          # state, and the only reason it lasted 87 days is that it produced a
          # warning every time and a failure never. Stop the release.
          status="LAGS ${rv} — MINOR-level, in NO walk — severity floor"
          fail+=("${c}: crates.io=${rv} is a whole minor behind workspace=${ws} and this walk does not repair it — this is issue #140's exact shape (mnemo-mcp-server sat one minor behind for 87 days while every run reported success)")
        else
          # Lagging, and this walk is not responsible for it.
          #
          # This WARNS, it does not fail. Failing here was a real bug: it made
          # the push-to-main library path (cargo-publish.yml, which deliberately
          # excludes mnemo-mcp-server — see its own comment) responsible for a
          # crate only the tag path can publish. The result was that the 0.5.23
          # libraries could not ship because a DIFFERENT crate was stranded,
          # which makes the registry worse, not better, and couples two release
          # paths that are independent by design.
          #
          # The teeth live in `--mode assert`, which runs after the walk and
          # hard-fails if a crate the walk WAS responsible for did not land.
          # That is the check that actually catches #140's silent skip; a
          # pre-publish veto on an out-of-scope crate only blocks good releases.
          status="LAGS ${rv} — not this walk's responsibility"
          stranded+=("${c}: crates.io=${rv} vs workspace=${ws}")
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

if [[ ${#stranded[@]} -gt 0 && "$MODE" == "preflight" ]]; then
  echo "::warning::${#stranded[@]} crate(s) lag the workspace but are NOT in this walk, so this run cannot repair them: ${stranded[*]}. Not a failure — this walk is not responsible for them (e.g. cargo-publish.yml deliberately excludes mnemo-mcp-server; only the tag path in release-crate.yml publishes it). Tracked by issue #140."
  {
    echo "### ⚠️ Lagging, but out of this walk's scope"
    echo
    for s in "${stranded[@]}"; do echo "- \`${s}\`"; done
    echo
    echo "This run is not responsible for these. The tag path (\`release-crate.yml\`) publishes them; see [issue #140](https://github.com/sattyamjjain/mnemo/issues/140)."
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
    if [[ $FLOOR -eq 1 ]]; then
      echo "::error::publish preflight FAILED (severity floor ON) — ${#fail[@]} artifact(s) are a whole minor behind or absent, and this walk does not repair them. Add each to the WALK in release-crate.yml, or set publish = false so the intent is explicit. This gate exists because #140's 87-day drift only ever produced warnings."
    else
      echo "::error::publish preflight FAILED — ${#fail[@]} artifact(s) lag the workspace and this walk does not repair them. Add them to the walk or fix the registry state before publishing (issue #140)."
    fi
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
    echo "**Triage in order: [\`docs/release/registry-token-runbook.md\`](https://github.com/sattyamjjain/mnemo/blob/main/docs/release/registry-token-runbook.md).** Do NOT rotate the token first — during [#140](https://github.com/sattyamjjain/mnemo/issues/140) the token was never the blocker, and the \`/api/v1/me\` 403 that anchored that diagnosis was advisory. Check walk membership and the tag/CHANGELOG gates before touching credentials."
  } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit 1
fi

echo "registry parity OK (${MODE}): ${#crates[@]} crate(s) checked against workspace ${ws}; SDK artifacts consistent with their own registries."
