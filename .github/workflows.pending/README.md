# Pending workflow files (v0.3.3)

The two `.yml.txt` files in this directory are GitHub Actions workflows
that need a token with `workflow` scope to land. The OAuth token used
to push the v0.3.3 branch lacks that scope, so they are parked here
rather than under `.github/workflows/`.

To apply (one-time, by a maintainer with workflow scope):

```bash
mkdir -p .github/workflows
git mv .github/workflows.pending/benchmarks-nightly.yml.txt \
       .github/workflows/benchmarks-nightly.yml
git mv .github/workflows.pending/security.yml.txt \
       .github/workflows/security.yml
rmdir .github/workflows.pending 2>/dev/null  # may be non-empty if other pending files exist
git commit -m "chore(ci): activate v0.3.3 workflows (benchmarks-nightly, security)"
git push
```

After that, both workflows take effect on the next push. Confirm by
opening Actions → see `benchmarks-nightly` and `security` listed.

## What each does

- **`security.yml`** — runs `cargo audit` and `cargo deny check advisories`
  on every push, PR, and nightly at 04:07 UTC. Reads ignore lists from
  `.cargo/audit.toml` and `deny.toml`.
- **`benchmarks-nightly.yml`** — runs `mnemo.benches.locomo_runner` against
  LoCoMo and LongMemEval nightly at 02:11 UTC, scored with the
  `claude-haiku-4-5-20251001` LLM judge. Compares the result against
  `docs/benchmarks/baseline.json` via
  `.github/scripts/check_bench_regression.py` and fails the job on a
  >3pp recall@10 drop. Requires `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`,
  and `HF_TOKEN` to be set as repo secrets.
