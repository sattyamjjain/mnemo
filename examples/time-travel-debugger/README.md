# Mnemo Time-Travel Debugger UI

> v0.4.0-rc3 (Task Q4). Vanilla HTML/JS, no build step.

A single-page debugger that lets you scroll through engine state at
two arbitrary timestamps and diff the recall result. Backed by the
`as_of` parameter on `GET /v1/memories` (Sprint 11 point-in-time
recall).

## Run it

The page just needs Mnemo's REST API reachable from the browser.

```bash
# Start Mnemo with REST enabled.
mnemo --rest-port 8000

# Serve the debugger (any static server will do).
cd examples/time-travel-debugger
python3 -m http.server 4000

# Open http://localhost:4000 in a browser.
```

If your Mnemo deployment lives on another origin, set
`MNEMO_CORS_ORIGINS=http://localhost:4000` before starting `mnemo`
so the fetch passes CORS.

## What it shows

* **Left panel** — the recall result at "Time A".
* **Right panel** — the recall result at "Time B".
* **Diff highlighting** — records present in B but not A get a green
  bar; records in A that B no longer cites get a red bar and fade.

## Why this exists

Mnemo's audit log already proves *which* memories an agent saw at a
moment in time. The debugger answers the next question — *what would
the agent have seen had it asked the same question yesterday?* —
which is the question that comes up when a model gives an
inexplicable answer and you need to figure out whether the recall
ranking changed under the agent's feet.
