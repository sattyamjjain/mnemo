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

# npm registry state for the TypeScript SDK (sdks/typescript). crates.io was the
# only registry this guard watched, so a package.json bumped past what is on npm
# went unnoticed — exactly today's state: package.json is ahead while npm's
# @mndfreek/mnemo-sdk has trailed since the npm publish started failing on a bad
# NPM_TOKEN (operator action; see .github/workflows/npm-publish.yml). npm is
# guarded the SAME baselined way as crates.io: GREEN on the acknowledged standing
# gap, RED only on a NEW divergence (package.json bumped past the baseline
# without an npm publish).
ts_pkg="${REPO_ROOT}/sdks/typescript/package.json"
npm_name=""; npm_src=""; npm_reg=""
if [[ -f "$ts_pkg" ]]; then
  read -r npm_name npm_src < <(python3 -c 'import json,sys
d = json.load(open(sys.argv[1]))
print(d.get("name", ""), d.get("version", ""))' "$ts_pkg" 2>/dev/null || echo " ")
  if [[ -n "$npm_name" ]]; then
    npm_reg="$(curl -sSf -A "$UA" "https://registry.npmjs.org/${npm_name}" 2>/dev/null | python3 -c 'import sys,json
try:
    print(json.load(sys.stdin).get("dist-tags", {}).get("latest", ""))
except Exception:
    print("")' || echo "")"
  fi
fi

# All the classification / baseline logic lives in one python pass so the semver
# comparison and JSON handling are not re-implemented in portable bash. The
# registry data is passed via env vars (NOT a pipe) because the heredoc below
# already occupies python's stdin as the program source.
REGISTRY_TSV="$registry_tsv" NPM_NAME="$npm_name" NPM_SRC="$npm_src" NPM_REG="$npm_reg" \
  python3 - "$workspace_version" "$BASELINE" "$UPDATE_BASELINE" <<'PY'
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
            "expired CARGO_REGISTRY_TOKEN (operator action; see CHANGELOG). The 'npm' "
            "block guards the TypeScript SDK (sdks/typescript/package.json vs the npm "
            "registry) the same baselined way. "
            "Refresh after a real publish: bash scripts/check_version_drift.sh "
            "--update-baseline"
        ),
        "workspace_version": workspace_version,
        "crates": dict(sorted(crates.items())),
        "npm": (
            {
                "package": os.environ.get("NPM_NAME", ""),
                "source_version": os.environ.get("NPM_SRC", ""),
                "registry_version": os.environ.get("NPM_REG", ""),
            }
            if os.environ.get("NPM_NAME")
            else {}
        ),
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

# Binary-vs-libraries parity. The published mnemo-mcp-server (the `mnemo` binary a
# stranger installs) must not trail the published mnemo-core by more than one patch.
# It sat at 0.4.4 for 80 days while the libraries reached 0.5.21, because it was
# excluded from the publish walk and no guard watched the binary-vs-library gap
# specifically. This is a HARD check, NOT baselined: a binary that trails its own
# libraries is a bug a stranger hits on `cargo install`, so it fails even when the
# workspace-vs-registry drift is an acknowledged, blocked-on-token state.
reg = {n: v for (n, v, _u) in rows}
parity_fail = None
core_v, mcp_v = reg.get("mnemo-core", ""), reg.get("mnemo-mcp-server", "")
if core_v and mcp_v and behind(core_v, mcp_v):
    parity_fail = (mcp_v, core_v)

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

# --- npm drift: same baselined semantics as crates.io, for the one TS SDK ------
npm_name = os.environ.get("NPM_NAME", "")
npm_src = os.environ.get("NPM_SRC", "")
npm_reg = os.environ.get("NPM_REG", "")
npm_new_div = None  # reason string when a NEW npm divergence appears
npm_status = None   # human-readable status for the table
base_npm = base.get("npm") if isinstance(base.get("npm"), dict) else {}
if npm_name and npm_src:
    if not npm_reg:
        npm_status = "not on npm yet"
    elif not behind(npm_src, npm_reg):
        npm_status = "ok (matches package.json)"
    else:
        b_src = base_npm.get("source_version")
        b_reg = base_npm.get("registry_version")
        if b_reg is None:
            # Baseline predates npm tracking — acknowledge the standing gap
            # rather than fail on day one; refresh the baseline to record it.
            npm_status = "drift (baseline predates npm tracking)"
        elif parse(npm_src) > parse(b_src or "0.0.0"):
            npm_new_div = f"package.json advanced {b_src} -> {npm_src} without publishing to npm"
            npm_status = f"NEW DIVERGENCE — {npm_new_div}"
        elif parse(npm_reg) < parse(b_reg):
            npm_new_div = f"npm registry below baseline {b_reg}"
            npm_status = f"NEW DIVERGENCE — {npm_new_div}"
        else:
            npm_status = "drift (acknowledged in baseline)"

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
    # `behind()` deliberately tolerates <= 1 patch as "a release in flight", so
    # this bucket is NOT all exact matches. Saying "matches workspace" for a
    # crate that is a patch behind is a false statement in a guard's own output:
    # a reader concludes `cargo add <crate>` gives the workspace version when it
    # does not. Distinguish the two rather than flatten them.
    row(n, v, u, "ok (matches workspace)" if parse(v) >= parse(workspace_version)
        else f"ok (one patch behind {workspace_version}, publish in flight)")
for n, v, u in acknowledged:
    row(n, v, u, "drift (acknowledged in baseline)")
for n, v, u, why in new_div:
    row(n, v, u, f"NEW DIVERGENCE — {why}")
if unpublished:
    print(f"  (unpublished, not on crates.io: {' '.join(unpublished)})")

if npm_name:
    print(f"\n  {'npm package':28} {'package.json':12} {'npm latest':11} status")
    print(f"  {'-' * 28} {'-' * 12} {'-' * 11} ------")
    print(f"  {npm_name:28} {npm_src:12} {(npm_reg or '-'):11} {npm_status}")

if new_div or parity_fail or npm_new_div:
    print()
    if new_div:
        print(f"::error::version drift: {len(new_div)} NEW divergence(s) since the recorded "
              f"baseline (workspace {base_ws}). The standing drift is acknowledged; this is "
              f"drift getting WORSE.")
        for n, v, u, why in new_div:
            print(f"::error::  {n}: crates.io {v} vs workspace {workspace_version}, "
                  f"gap age {fmt_age(u)} — {why}")
    if parity_fail:
        mcp_v, core_v = parity_fail
        print(f"::error::binary-vs-libraries drift: mnemo-mcp-server is {mcp_v} on crates.io "
              f"while mnemo-core is {core_v}. The `mnemo` binary a stranger installs trails "
              f"its own libraries by more than one patch. Publish mnemo-mcp-server so "
              f"`cargo install mnemo-mcp-server` matches the libraries.")
    if npm_new_div:
        print(f"::error::npm drift: {npm_name} is {npm_reg or 'absent'} on npm while "
              f"package.json is {npm_src} — {npm_new_div}. Publish the SDK to npm (fix the "
              f"NPM_TOKEN first if the publish 404s) or refresh the baseline once the new "
              f"state is intended.")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as f:
            if new_div:
                f.write(f"### version drift — {len(new_div)} NEW divergence(s)\n\n")
                f.write("| crate | crates.io | workspace | gap age | reason |\n|---|---|---|---|---|\n")
                for n, v, u, why in new_div:
                    f.write(f"| {n} | {v} | {workspace_version} | {fmt_age(u)} | {why} |\n")
                f.write("\nBump-without-publish or a new stranded crate widened the gap. "
                        "Publish (rotate `CARGO_REGISTRY_TOKEN` first if the walk 403s), or "
                        "refresh the baseline once the new state is intended.\n")
            if parity_fail:
                mcp_v, core_v = parity_fail
                f.write(f"### binary-vs-libraries drift\n\nmnemo-mcp-server `{mcp_v}` trails "
                        f"mnemo-core `{core_v}` by more than one patch. Publish the server "
                        f"binary so `cargo install mnemo-mcp-server` matches the libraries.\n")
            if npm_new_div:
                f.write(f"### npm drift\n\n`{npm_name}` is `{npm_reg or 'absent'}` on npm while "
                        f"package.json is `{npm_src}` — {npm_new_div}. Publish the SDK "
                        f"(fix `NPM_TOKEN` first if the publish 404s) or refresh the baseline.\n")
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
    trailing = [(n, v) for n, v, _ in resolved
                if parse(v) < parse(workspace_version)]
    if trailing:
        # Green, but do not claim parity that does not exist.
        print(f"OK: no crate is more than one patch behind workspace "
              f"{workspace_version}, so nothing is stranded. "
              f"{len(trailing)} crate(s) are NOT yet at {workspace_version} and "
              f"resolve one patch lower today:")
        for n, v in trailing:
            print(f"     {n}: crates.io {v}")
        print("     That is a publish in flight, not drift. It becomes drift if "
              "the workspace advances again before they publish.")
    else:
        print(f"OK: every published crate matches workspace {workspace_version}. "
              f"Drift resolved — refresh the baseline: "
              f"bash scripts/check_version_drift.sh --update-baseline")
if npm_name and npm_status and ("acknowledged" in npm_status or "predates" in npm_status):
    print()
    print(f"OK (npm): {npm_name} {npm_reg or 'absent'} on npm trails package.json {npm_src} — "
          f"acknowledged standing gap, GREEN on purpose. Publish the SDK or refresh the baseline.")
sys.exit(0)
PY
