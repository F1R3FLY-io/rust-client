# Contributing to rust-client

Thanks for contributing! This repo follows a `dev → main` flow.

## Branching policy

```
feature/your-change ──► dev ──► main
```

- **All feature/fix PRs target `dev`.** This is the default branch.
- **Maintainers** cut releases by merging `dev` into `main`.

Do not open PRs directly against `main`. It is protected and reserved for release flow.

## Opening a PR

1. Fork the repo (or create a feature branch if you have write access).
2. Create your branch from the latest `dev`:
   ```bash
   git checkout dev
   git pull origin dev
   git checkout -b feature/short-description
   ```
3. Make your changes. Keep commits focused; write clear messages.
4. Update documentation as needed — `README.md`, files under `docs/`, command-specific docs in `docs/commands/`, and any inline doc comments. Docs and code should ship in the same PR.
5. Push and open a PR. The base branch should default to `dev` — leave it as is.

## Quality checks

Before opening a PR:

- `cargo build --release` passes
- `cargo fmt --check` passes (run `cargo fmt` if not)
- `cargo clippy --all-targets -- -D warnings` passes
- `cargo test` passes (unit tests)
- For changes that touch node interaction, also run:
  ```bash
  ./scripts/smoke_test.sh
  cargo test --test smoke -- --ignored
  ```

CI runs these on every PR. Failing CI blocks merge.

## Versioning & releases

Versions bump automatically (patch) on merge to `main` via the Release workflow — `dev` does not release. Do not manually edit `Cargo.toml` version or `CHANGELOG.md` — those are managed by the release pipeline.

## Code style

- Follow rustfmt defaults
- Prefer `Option<T>` over sentinel values
- Avoid silent error swallowing — propagate or log
- Match the existing module layout (`src/commands/`, `src/grpc/`, etc.)

## Questions

Open a discussion or draft PR if you're unsure about scope or approach.
