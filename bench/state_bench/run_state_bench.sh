#!/usr/bin/env bash
# Run mnemo on Microsoft STATE-Bench (Agent Learning Track), end-to-end.
#
# This orchestrates the full protocol run once credentials are available:
#   1. check out microsoft/STATE-Bench at the PINNED commit,
#   2. install its deps (uv) and build the mnemo Python SDK into that venv,
#   3. copy the mnemo adapter into the STATE-Bench repo-root agents/ folder,
#   4. for each domain and each seed: build learnings -> run_batch -> metrics,
#   5. leave scored trajectories + metrics under results/.
#
# It FAILS LOUD if the protocol-locked GPT-5.4 eval client (or the agent client)
# is not configured. Use `--build-only` for the credential-free smoke (builds
# the mnemo learnings stores and skips the LLM run).
#
# See ./README.md for the full write-up, the pinned SHA, and honest framing.
set -euo pipefail

PINNED_SHA="4efcbf2d4fe60df04878859b692d9391f3d5b33a"   # STATE-Bench v0.8.1
BENCH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MNEMO_ROOT="$(cd "${BENCH_DIR}/../.." && pwd)"
WORK_DIR="${STATE_BENCH_DIR:-/private/tmp/mnemo_sb_work/STATE-Bench}"

SEEDS="${SEEDS:-3}"                 # outer seeds; the task requires >= 3 for spread
NUM_RUNS="${NUM_RUNS:-5}"           # STATE-Bench per-task runs (official = 5)
TOP_K="${TOP_K:-3}"                 # official retrieve-learnings top-k
AGENT_MODEL="${STATE_BENCH_AGENT_MODEL_NAME:-gpt-5.1}"
NUM_WORKERS="${NUM_WORKERS:-1}"     # 1 avoids DuckDB file-lock across process workers
DOMAINS=(travel customer_support shopping_assistant)
BUILD_ONLY=0
[ "${1:-}" = "--build-only" ] && BUILD_ONLY=1

echo "== STATE-Bench x mnemo =="
echo "  pinned SHA : ${PINNED_SHA}"
echo "  work dir   : ${WORK_DIR}"
echo "  seeds=${SEEDS} num_runs=${NUM_RUNS} top_k=${TOP_K} agent_model=${AGENT_MODEL} build_only=${BUILD_ONLY}"

# --- 1. checkout STATE-Bench at the pinned SHA -------------------------------
if [ ! -d "${WORK_DIR}/.git" ]; then
  mkdir -p "$(dirname "${WORK_DIR}")"
  git clone https://github.com/microsoft/STATE-Bench "${WORK_DIR}"
fi
git -C "${WORK_DIR}" fetch --quiet --all
git -C "${WORK_DIR}" checkout --quiet "${PINNED_SHA}"
echo "  checked out $(git -C "${WORK_DIR}" rev-parse --short HEAD)"

# --- 2. deps + mnemo SDK into the STATE-Bench venv ---------------------------
( cd "${WORK_DIR}" && uv sync )
( cd "${WORK_DIR}" && uv pip install maturin )
# Build the mnemo public Python SDK (PyO3) into STATE-Bench's venv.
( cd "${WORK_DIR}" && uv run maturin develop --release -m "${MNEMO_ROOT}/python/Cargo.toml" )

# --- 3. install the adapter under repo-root agents/ --------------------------
mkdir -p "${WORK_DIR}/agents"
cp "${BENCH_DIR}/agents/mnemo_memory_agent.py" "${WORK_DIR}/agents/mnemo_memory_agent.py"

# --- credential gate (skipped for --build-only) ------------------------------
if [ "${BUILD_ONLY}" -eq 0 ]; then
  : "${STATE_BENCH_EVAL_ENDPOINT:?locked GPT-5.4 eval client not configured (see .env.example / README)}"
  : "${STATE_BENCH_EVAL_DEPLOYMENTS:?locked GPT-5.4 eval deployment not set}"
  if [ -z "${STATE_BENCH_AGENT_ENDPOINT:-}" ] && [ -z "${STATE_BENCH_AGENT_MODEL:-}" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
    echo "ERROR: no agent client configured (STATE_BENCH_AGENT_* or OPENAI_API_KEY)." >&2
    exit 2
  fi
fi

export MNEMO_STATEBENCH_DB_DIR="${BENCH_DIR}/results/mnemo_stores"

# --- 4. per domain x seed: build learnings -> run -> metrics -----------------
for seed in $(seq 1 "${SEEDS}"); do
  for domain in "${DOMAINS[@]}"; do
    export MNEMO_STATEBENCH_DOMAIN="${domain}"
    echo "-- seed ${seed} / domain ${domain}: build learnings --"
    ( cd "${WORK_DIR}" && uv run python "${BENCH_DIR}/build_learnings.py" \
        --train-dir "datasets/train_task_trajectories/${domain}" \
        --domain "${domain}" )

    if [ "${BUILD_ONLY}" -eq 1 ]; then continue; fi

    out="${BENCH_DIR}/results/seed${seed}/${domain}"
    echo "-- seed ${seed} / domain ${domain}: run_batch --"
    ( cd "${WORK_DIR}" && uv run python -m state_bench.scripts.run_batch \
        --domain "${domain}" \
        --agent-class MnemoMemoryAgent \
        --agent-model-name "${AGENT_MODEL}" \
        --num-runs "${NUM_RUNS}" \
        --retrieve-learnings-top-k "${TOP_K}" \
        --num-workers "${NUM_WORKERS}" \
        --output-dir "${out}/" )
    ( cd "${WORK_DIR}" && uv run python -m state_bench.scripts.compute_metrics \
        --domain "${domain}" \
        --results-dir "${out}/" \
        --num-runs "${NUM_RUNS}" \
        --output-dir "${out}/" )
  done
done

echo "== done. scored trajectories + metrics under ${BENCH_DIR}/results/seed*/ =="
echo "   summarise per-domain pass@1 / pass^5 / UX / cost and the across-seed spread"
echo "   into results/state_bench.md (cite the GPT-5.1-no-memory baseline)."
