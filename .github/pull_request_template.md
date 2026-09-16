<!--
Base branch: dev

All feature/fix PRs should target `dev`. PRs against `main` will be redirected.
See CONTRIBUTING.md for the branching policy.
-->

## Summary

<!-- What changes and why -->

## Test plan

- [ ] `cargo build --release`
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Integration tests (if touching node interaction): `cargo test --release --features integration --test integration`

## Notes

<!-- Anything reviewers should know: tradeoffs, follow-ups, related PRs/issues -->
