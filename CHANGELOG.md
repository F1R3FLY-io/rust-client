# Changelog

All notable changes to the F1r3fly rust-client will be documented in this file.
This changelog is automatically generated from conventional commits.


## [0.2.0] - 2026-05-05

### Bug Fixes

- address PR review feedback
- tolerate Scala node in status, WS events, and smoke tests
- update epoch-rewards smoke test to verify parsed output
- use HTTP API for epoch-rewards to parse full response data
- use correct URI rho:vault:system in test_systemvault.rho

### CI

- revert Rust CI to standalone node
- install protobuf-compiler for models build.rs
- add arch-specific RUSTFLAGS for gxhash (aes+neon on arm64)
- add build, test, and release workflows

### Documentation

- add API changelog for Jan-Mar 2026
- omit branch in library dependency example
- add library usage documentation to README

### Features

- expand extract_par_data to handle URIs, bytes, and collections
- update for API redesign, add integration tests, CI shard
- display native token metadata in status command
- support all 9 event types, rename watch-blocks to watch-events
- align with f1r3node PR #398 - RevAddress → VaultAddress rename

### Refactoring

- client library restructure, new commands, docs (#16)
- address PR #10 review feedback

### Deps

- switch from path to git tag rust-v0.4.13

### Smoke_test

- build release first, portable timeout for macOS

### Style

- apply cargo fmt


