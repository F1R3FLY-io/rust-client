# Changelog

All notable changes to the F1r3fly rust-client will be documented in this file.
This changelog is automatically generated from conventional commits.


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


