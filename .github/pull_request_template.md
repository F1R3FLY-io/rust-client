<!--
Base branch: staging

All feature/fix PRs should target `staging`. PRs against `dev` or `main` will be redirected.
See CONTRIBUTING.md for the branching policy.
-->

## Summary

<!-- What changes and why -->

## Test plan

- [ ] `cargo build --release`
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Smoke tests (if touching node interaction): `./scripts/smoke_test.sh`

## Notes

<!-- Anything reviewers should know: tradeoffs, follow-ups, related PRs/issues -->
