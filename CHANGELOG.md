# Changelog

All notable changes to the F1r3fly rust-client will be documented in this file.
This changelog is automatically generated from conventional commits.


## [0.2.6] - 2026-08-04

### CI

- drop vestigial two-node-type wording from smoke tests
- retire the Scala smoke leg and stop swallowing test failures


## [0.2.5] - 2026-07-30

### Bug Fixes

- keep transfer/deploy futures Send in wait_for_deploy_finalization


## [0.2.4] - 2026-07-29

### Bug Fixes

- point TO_ADDR at a valid REV address
- transfer() fails on deploy errors and vault rejections
- fund vault transfers with 500k phlo instead of the 50k default

### Testing

- live smoke coverage for on-chain transfer success; fail deploys on tip-lookup errors


## [0.2.3] - 2026-07-28

### CI

- add clippy, cargo-deny, and markdown link checks

### Documentation

- remove stale dependencies section from README

### Miscellaneous

- point f1r3node deps at f1r3node-rust v0.4.23


## [0.2.2] - 2026-07-28

### Bug Fixes

- pass private key to generate-vault-address command
- fall back to deploy node on observer errors
- derive native token decimals from node status
- run cargo fmt and export FIREFLY_PRIVATE_KEY in smoke test
- remove hardcoded private key from client source

### CI

- pin rust node image to v0.4.23; use valid unbonded key in smoke tests
- fix smoke compose paths/ports; release patch-only on main

### Documentation

- update --amount docs for base units default, add --whole-tokens flag

### Miscellaneous

- deprecate staging branch; releases from main only


## [0.2.1] - 2026-05-14

### Documentation

- add CONTRIBUTING and PR template; ci: pin all branches to :dev image

### Features

- deploy-status command and sig-level finalization polling (#20)
- expand extract_par_data to handle URIs, bytes, and collections

### Miscellaneous

- increase default timeouts


## [0.1.3] - 2026-04-23

### Bug Fixes

- address PR review feedback
- tolerate Scala node in status, WS events, and smoke tests

### CI

- revert Rust CI to standalone node

### Features

- update for API redesign, add integration tests, CI shard
- display native token metadata in status command
- support all 9 event types, rename watch-blocks to watch-events

### Deps

- switch from path to git tag rust-v0.4.13

### Style

- apply cargo fmt


## [0.1.2] - 2026-04-10

### Refactoring

- client library restructure, new commands, docs (#16)


## [0.1.1] - 2026-03-30

### CI

- install protobuf-compiler for models build.rs
- add arch-specific RUSTFLAGS for gxhash (aes+neon on arm64)
- add build, test, and release workflows


## [0.1.0] - 2026-03-17

### Bug Fixes

- update epoch-rewards smoke test to verify parsed output
- use HTTP API for epoch-rewards to parse full response data
- use correct URI rho:vault:system in test_systemvault.rho

### Documentation

- add API changelog for Jan-Mar 2026
- omit branch in library dependency example
- add library usage documentation to README

### Features

- align with f1r3node PR #398 - RevAddress → VaultAddress rename

### Refactoring

- address PR #10 review feedback

### Smoke_test

- build release first, portable timeout for macOS


