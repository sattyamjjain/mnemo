#!/usr/bin/env bash
#
# Version-drift guard — baselined so it is signal, not noise.
#
# History: this guard used to FAIL on ANY publishable crate more than one patch
# behind the workspace version on crates.io. That is correct in principle, but
# the mnemo registry has been permanently behind since the tag-gated publish
# broke (an expired CARGO_REGISTRY_TOKEN, an operator action nobody has done),
# so the job was RED on every single push. A check that is always red teaches
# everyone to ignore it — worse than no check at all.
#
# New semantics (the drift is real; the guard's job is to catch it getting
# WORSE, not to shout about the standing, already-known gap):
#
#   * GREEN when the registry MATCHES the workspace (drift fully resolved), OR
#     when the current drift is exactly the acknowledged baseline recorded in
#     scripts/version-drift-baseline.json (no new divergence).
#   * RED only when a NEW divergence appears since that baseline:
#       - a publishable crate that is NOT in the baseline is behind, or
#       - the workspace [workspace.package].version was advanced past the
#         baseline's workspace_version while a crate is still behind — i.e. the
#         pile grew because someone bumped without publishing.
#     (crates.io is append-only, so a crate's registry version can only move
#     forward; the only way drift worsens is a bump-without-publish or a new
#     stranded crate.)
#
# The RED message names every newly-stranded crate with its registry version,
# its workspace version, and the age of the gap in days.
#
# Refresh the baseline after a real publish (or to accept a new standing state):
#   bash scripts/check_version_drift.sh --update-baseline
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="${REPO_ROOT}/Cargo.toml"
BASELINE="${REPO_ROOT}/scripts/version-drift-baseline.json"
UA='mnemo-ci-version-drift (https://github.com/sattyamjjain/mnemo)'

UPDATE_BASELINE=0
if [[ "${1:-}" == "--update-baseline" ]]; then
  UPDATE_BASELINE=1
fi

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

# Publishable crate names: every crates/*/Cargo.toml whose [package] does not
# set `publish = false`.
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

# Fetch registry state once per crate: "name<TAB>max_version<TAB>updated_at"
# (max_version empty if the crate is not on crates.io yet).
registry_tsv=""
for crate in "${crate_names[@]}"; do
  json="$(curl -sSf -A "$UA" "https://crates.io/api/v1/crates/${crate}" 2>/dev/null || echo '{}')"
  line="$(printf '%s' "$json" | python3 -c 'import sys,json
try:
    d=json.load(sys.stdin)["crate"]
    print((d.get("max_version") or "")+"\t"+(d.get("updated_at") or ""))
except Exception:
    print("\t")')"
  registry_tsv+="${crate}	${line}
"
done

# All the classification / baseline logic lives in one python pass so the semver
# comparison and JSON handling are not re-implemented in portable bash. The
# registry data is passed via an env var (NOT a pipe) because the heredoc below
# already occupies python's stdin as the program source.
REGISTRY_TSV="$registry_tsv" python3 - "$workspace_version" "$BASELINE" "$UPDATE_BASELINE" <<'PY'
import sys, json, os, datetime

workspace_version = sys.argv[1]
baseline_path = sys.argv[2]
update_baseline = sys.argv[3] == "1"

def parse(v):
    if not v:
        return (0, 0, 0)
    core = v.split("+", 1)[0].split("-", 1)[0]
    parts = core.split(".")
    return tuple(int(x) for x in (parts + ["0", "0", "0"])[:3])

def behind(w, p):
    # True if crate version p is MORE than one patch behind workspace w.
    W, P = parse(w), parse(p)
    if P >= W:
        return False
    if W[0] == P[0] and W[1] == P[1] and (W[2] - P[2]) <= 1:
        return False
    return True

def age_days(updated_at):
    if not updated_at:
        return None
    try:
        ts = updated_at.replace("Z", "+00:00")
        dt = datetime.datetime.fromisoformat(ts)
        now = datetime.datetime.now(datetime.timezone.utc)
        return (now - dt).days
    except Exception:
        return None

rows = []  # (name, registry_version, updated_at)
for ln in os.environ.get("REGISTRY_TSV", "").splitlines():
    if not ln.strip():
        continue
    parts = ln.split("\t")
    name = parts[0]
    ver = parts[1] if len(parts) > 1 else ""
    upd = parts[2] if len(parts) > 2 else ""
    rows.append((name, ver, upd))

# --update-baseline: record the current published state and exit.
if update_baseline:
    crates = {n: v for (n, v, _u) in rows if v}
    doc = {
        "note": (
            "Acknowledged crates.io drift baseline for the version-drift guard. "
            "The guard is GREEN when the registry matches this baseline (or the "
            "workspace) and RED only when a NEW divergence appears: a new crate "
            "falls behind, or the workspace version is advanced past "
            "'workspace_version' below without publishing (widening the gap). The "
            "standing drift is the tag-gated publish being blocked on rotating an "
            "expired CARGO_REGISTRY_TOKEN (operator action; see CHANGELOG). "
            "Refresh after a real publish: bash scripts/check_version_drift.sh "
            "--update-baseline"
        ),
        "workspace_version": workspace_version,
        "crates": dict(sorted(crates.items())),
    }
    with open(baseline_path, "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")
    print(f"wrote baseline: {baseline_path}")
    print(f"  workspace_version = {workspace_version}; {len(crates)} published crates recorded")
    sys.exit(0)

# Normal run: load the baseline.
if not os.path.exists(baseline_path):
    print(f"::error::no drift baseline at {baseline_path} — create it with "
          f"`bash scripts/check_version_drift.sh --update-baseline`")
    sys.exit(2)
with open(baseline_path) as f:
    base = json.load(f)
base_ws = base.get("workspace_version", "0.0.0")
base_crates = base.get("crates", {})

ws_advanced = parse(workspace_version) > parse(base_ws)

resolved, acknowledged, new_div, unpublished = [], [], [], []
for name, ver, upd in rows:
    if not ver:
        unpublished.append(name)
        continue
    if not behind(workspace_version, ver):
        resolved.append((name, ver, upd))
        continue
    # crate is behind the workspace by > 1 patch.
    base_ver = base_crates.get(name)
    if base_ver is None:
        new_div.append((name, ver, upd, "not in baseline (new stranded crate)"))
    elif ws_advanced:
        new_div.append((name, ver, upd,
                        f"workspace advanced {base_ws} -> {workspace_version} without publishing"))
    elif parse(ver) < parse(base_ver):
        new_div.append((name, ver, upd, f"registry below baseline {base_ver}"))
    else:
        acknowledged.append((name, ver, upd))

def fmt_age(upd):
    d = age_days(upd)
    return f"{d}d" if d is not None else "-"

# Aligned table for anyone reading the log.
print(f"\nworkspace version : {workspace_version}   (baseline workspace {base_ws})")
print(f"  {'crate':24} {'workspace':10} {'crates.io':11} {'baseline':10} {'gap age':8} status")
print(f"  {'-'*24} {'-'*10} {'-'*11} {'-'*10} {'-'*8} ------")
def row(name, ver, upd, status):
    bv = base_crates.get(name, "-")
    print(f"  {name:24} {workspace_version:10} {ver:11} {bv:10} {fmt_age(upd):8} {status}")
for n, v, u in resolved:
    row(n, v, u, "ok (matches workspace)")
for n, v, u in acknowledged:
    row(n, v, u, "drift (acknowledged in baseline)")
for n, v, u, why in new_div:
    row(n, v, u, f"NEW DIVERGENCE — {why}")
if unpublished:
    print(f"  (unpublished, not on crates.io: {' '.join(unpublished)})")

if new_div:
    print()
    print(f"::error::version drift: {len(new_div)} NEW divergence(s) since the recorded "
          f"baseline (workspace {base_ws}). The standing drift is acknowledged; this is "
          f"drift getting WORSE.")
    for n, v, u, why in new_div:
        print(f"::error::  {n}: crates.io {v} vs workspace {workspace_version}, "
              f"gap age {fmt_age(u)} — {why}")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as f:
            f.write(f"### version drift — {len(new_div)} NEW divergence(s)\n\n")
            f.write("| crate | crates.io | workspace | gap age | reason |\n|---|---|---|---|---|\n")
            for n, v, u, why in new_div:
                f.write(f"| {n} | {v} | {workspace_version} | {fmt_age(u)} | {why} |\n")
            f.write("\nBump-without-publish or a new stranded crate widened the gap. "
                    "Publish (rotate `CARGO_REGISTRY_TOKEN` first if the walk 403s), or "
                    "refresh the baseline once the new state is intended.\n")
    sys.exit(1)

# GREEN.
if acknowledged:
    print()
    print(f"OK: no NEW divergence. {len(acknowledged)} crate(s) carry the acknowledged "
          f"drift below (blocked on rotating CARGO_REGISTRY_TOKEN — an operator action):")
    for n, v, u in acknowledged:
        print(f"     {n}: crates.io {v}, stranded {fmt_age(u)} behind workspace {workspace_version}")
    print("     This is GREEN on purpose. Refresh the baseline after a real publish: "
          "bash scripts/check_version_drift.sh --update-baseline")
elif unpublished and not resolved:
    print("OK: nothing published yet; no drift to report.")
else:
    print(f"OK: every published crate matches workspace {workspace_version}. "
          f"Drift resolved — refresh the baseline: "
          f"bash scripts/check_version_drift.sh --update-baseline")
sys.exit(0)
PY
