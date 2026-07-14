# Contributing to Mnemo

Thank you for your interest in contributing to Mnemo! This document provides guidelines and instructions for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/<your-username>/mnemo.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run tests: `cargo test --workspace`
6. Push and open a pull request

## Development Setup

### Prerequisites

- Rust 1.85+ (see `rust-toolchain.toml`)
- Python 3.10+ (for Python SDK development)
- Node.js 18+ (for TypeScript SDK development)
- Go 1.21+ (for Go SDK development)

### Building

```bash
cargo build --workspace
```

### Running Tests

```bash
cargo test --workspace
```

### Code Quality

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Pull Request Process

1. Ensure all tests pass (`cargo test --workspace`)
2. Ensure code is formatted (`cargo fmt`)
3. Ensure clippy is clean (`cargo clippy --all-targets --all-features`)
4. Update documentation if you changed any public APIs
5. Add tests for new functionality
6. Keep commits focused and write clear commit messages
7. **Sign off every commit** with `git commit -s` — this repo requires a
   Developer Certificate of Origin sign-off (see
   [Developer Certificate of Origin (DCO)](#developer-certificate-of-origin-dco)
   below). The [DCO check](.github/workflows/dco.yml) enforces it on every PR.

## Code Style

- Follow standard Rust formatting (`cargo fmt`)
- Use meaningful variable and function names
- Add doc comments for public APIs
- Keep functions focused and small
- Prefer returning `Result<T>` over panicking

## Reporting Bugs

Use the [GitHub Issues](https://github.com/sattyamjjain/mnemo/issues) tab with the bug report template.

## Requesting Features

Use the [GitHub Issues](https://github.com/sattyamjjain/mnemo/issues) tab with the feature request template.

## Spec-drift policy

The daily-product-prompt pipeline that generates this repo's release
schedule runs against an external skill template whose anchored
description sometimes drifts from this repo's actual description.
The repo description on `main` is **canonical** — see
[`docs/spec-drift-2026-05-04.md`](docs/spec-drift-2026-05-04.md) for
the recorded reconciliation, the rationale, and the mapping from
skill-template surface anchors to where each one actually lives in
this codebase.

**If you are landing a surface-affecting change** (renaming a public
crate, removing a primary API, changing the wire-protocol version,
deprecating a backend), please:

1. Read `docs/spec-drift-*.md` — the most-recent file is the active
   reconciliation.
2. If your change widens the divergence (the skill template would now
   be even more wrong), file a new `docs/spec-drift-<date>.md` in the
   same PR and update the link from this section.
3. If your change *narrows* the divergence (e.g. landing the actual
   `mnemo-langgraph` Rust adapter the skill template anticipated),
   call that out explicitly in the PR body so the schedule pipeline
   can retire that anchor row.

## Developer Certificate of Origin (DCO)

Every contribution to Mnemo must be signed off under the
[Developer Certificate of Origin 1.1](https://developercertificate.org/). The
DCO is a lightweight, per-commit attestation that you wrote the patch (or
otherwise have the right to submit it under the project's Apache-2.0 license). It
is **not** a copyright assignment — you keep your copyright.

Sign off by adding a `Signed-off-by` trailer to each commit, which `git` does for
you with the `-s` flag:

```bash
git commit -s -m "fix: correct recall ordering under recency tie"
# adds:  Signed-off-by: Your Name <your.email@example.com>
```

The `Signed-off-by` name and email **must match the commit author**. To sign off
a branch of existing commits: `git rebase --signoff main`. A
[DCO check workflow](.github/workflows/dco.yml) verifies this on every pull
request and fails the check if any non-merge commit is missing a matching
sign-off.

By signing off, you certify the DCO, reproduced here in full:

```
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.

Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

## Contributor License Agreement (CLA)

For substantial contributions the project may additionally ask you to sign a
Contributor License Agreement — an individual ICLA, or a corporate CCLA if you
are contributing on behalf of an employer. Both are the standard Apache-style
texts and are reproduced in [`CLA.md`](CLA.md). The CLA does **not** change the
project's license (Mnemo stays Apache-2.0) and does **not** transfer your
copyright; it grants the project a clear license to redistribute your
contribution. For everyday contributions the per-commit DCO sign-off above is
sufficient; the CLA is requested only when scope warrants it.

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
