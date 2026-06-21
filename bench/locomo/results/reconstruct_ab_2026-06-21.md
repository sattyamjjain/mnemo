# Reconstruct vs. RRF A/B — gold-coverage@5

> 2026-06-21 — active-reconstruction recall (MRAgent, arXiv:2606.06036) vs. default hybrid RRF.
> Fixture: 24 multi-hop clusters (head matches the query; the gold answer lives in a
> graph-linked detail that shares no token with the query). See the module doc for the
> honesty caveats — this is a mechanism check, not an absolute-number claim.

| strategy | gold-coverage@5 |
|----------|------------------:|
| `auto` (RRF) | 0.083 |
| `reconstruct` | 0.208 |
| **delta** | **+0.125** |

Of 24 multi-hop golds, flat RRF surfaced 2 at k=5; reconstruction
surfaced 5 by walking the memory graph for linked/causal context.
