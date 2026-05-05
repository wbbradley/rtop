# Completed Work

## Phase 0 — Repo hygiene

Added repo scaffolding so subsequent phases can ship clean PRs:

- `LICENSE` (MIT, 2026, Will Bradley).
- `README.md` with description, status, screenshot placeholder, build/install/platform-support/license sections.
- `.gitignore` expanded for Rust/IDE/OS noise; `Cargo.lock` intentionally committed (binary crate).
- `Cargo.toml` package metadata: `description`, `repository`, `license`, `readme`, `keywords`, `categories`.
- `.github/workflows/ci.yml` with `linux` (required) and `macos` (`continue-on-error: true` until Phase 6) jobs running `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`.
- Verified `chk`, `cargo build`, and `cargo test` all pass clean.
