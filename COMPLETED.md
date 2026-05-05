# Completed Work

## Phase 0 — Repo hygiene

Added repo scaffolding so subsequent phases can ship clean PRs:

- `LICENSE` (MIT, 2026, Will Bradley).
- `README.md` with description, status, screenshot placeholder, build/install/platform-support/license sections.
- `.gitignore` expanded for Rust/IDE/OS noise; `Cargo.lock` intentionally committed (binary crate).
- `Cargo.toml` package metadata: `description`, `repository`, `license`, `readme`, `keywords`, `categories`.
- `.github/workflows/ci.yml` with `linux` (required) and `macos` (`continue-on-error: true` until Phase 6) jobs running `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`.
- Verified `chk`, `cargo build`, and `cargo test` all pass clean.

## Phase 1 — Linux read-only listing

Stood up the IR, sampler, and a minimal ratatui app that displays a load-sorted process list on Linux:

- Added deps via `cargo add`: `ratatui`, `crossterm`, `procfs`, `crossbeam-channel`, `clap` (`derive`), `anyhow`, `uzers`.
- `src/consts.rs`: full constant set (`SAMPLE_INTERVAL`, `LOAD_VIEW_VISIBLE_ROWS`, `SCROLLOFF`, `MIN_COLS`/`MIN_ROWS`, `ERROR_FLASH_DURATION`, `CPU_WARN_PCT`/`CPU_DANGER_PCT`).
- `src/process.rs`: IR — `ProcessId { pid, start_time }`, `Process` (all fields populated, including `is_kernel_thread`, `age`, `cpu_time_total`), `SystemStats`, `Snapshot { processes, by_id, sampled_at, system }`.
- `src/source.rs`: `trait ProcessSource`, with cfg-gated `pub use linux::LinuxProcessSource as PlatformSource`.
- `src/source/linux.rs`: `LinuxProcessSource` via `procfs`. Caches uid→username via `uzers`; caches `ticks_per_sec`, `page_size`, `boot_time` at construction. Per-pid `ProcError::NotFound` is skipped (TOCTOU); other errors propagate. `mem_used = mem_total - mem_available` (htop convention). Identifies kernel threads as `ppid == 2 || pid == 2`.
- `src/sampler.rs`: `spawn(...)` returns `Receiver<Arc<Snapshot>>`. Bounded(1) channel; on `Full`, sampler drop-NEWEST (TODO drop-OLDEST). Testable free fn `fill_cpu_pct` clamps `[0, 100*num_cpus]` (via `available_parallelism`); guards zero/negative `dt`; PID reuse (different `start_time`) → cpu_pct stays None.
- `src/app.rs` + `src/app/state.rs`: terminal owned via `ratatui::try_init` / `ratatui::restore`; crossterm event-forwarder thread; `crossbeam_channel::select!` over event + snapshot streams; Ctrl-C quits.
- `src/main.rs`: `clap` CLI (`--interval`, `--version`, `--help`); constructs `PlatformSource` (probes `/proc/self/stat`) BEFORE installing panic hook + entering raw mode, so /proc errors print and exit 1.
- `src/format.rs`: `bytes()` (B/KiB/MiB/GiB, binary units, `{:.1}` if value <10 else `{:.0}`); `age()` Phase-1 placeholder (`Xs`).
- `src/ui.rs` + `src/ui/load_view.rs`: load view renders `PID USER CPU% RSS COMMAND`; CPU%-desc sort with None last; CPU% renders `—` when None; cmdline empty falls back to `[<comm>]`.
- 13 unit tests pass: `bytes` boundary cases, `fill_cpu_pct` (normal/zero-dt/PID-reuse/new-process), and a Linux smoke test that asserts `getpid()` is in the snapshot.
- `chk` clean (fmt + clippy `-D warnings`); `cargo nextest run` green.
