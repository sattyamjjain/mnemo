<!--
Thanks for contributing to Mnemo! Every commit in this PR must be signed off
under the Developer Certificate of Origin (`git commit -s`). The DCO check will
fail otherwise — see CONTRIBUTING.md.
-->

## Summary

<!-- What does this change do, and why? -->

## Test Plan

<!-- How did you verify it? e.g. `cargo test --workspace`, the relevant bench,
     manual steps. Paste the key output. -->

## Checklist

- [ ] Every commit is **signed off** (`git commit -s`) — Developer Certificate of
      Origin ([CONTRIBUTING.md](../CONTRIBUTING.md#developer-certificate-of-origin-dco)).
- [ ] `cargo fmt --all -- --check` and `cargo clippy --all-targets --workspace --exclude mnemo-python -- -D warnings` pass.
- [ ] Tests added/updated where behavior changed; `cargo test --workspace` passes (or the failure is a known, unrelated red).
- [ ] Docs updated if public APIs or positioning changed.
- [ ] For substantial contributions, I am willing to sign the [CLA](../CLA.md) if the maintainers request it.
