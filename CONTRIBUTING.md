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

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
