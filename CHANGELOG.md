# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed
- Default sample/refresh interval is now 5 seconds (was 1 second). Pass `--interval 1` to restore the previous cadence.

## [0.1.1] - 2026-05-17

### Changed
- Load pane now defaults to 4 visible data rows (7 rows total including border and header) instead of 10 (13 rows). The tree pane absorbs the freed vertical space, reflecting that filtering is the primary mode of use. `Ctrl-d`/`Ctrl-u` half-page scroll in the load view is now 2 rows (was 5).

## [0.1.0] - 2026-05-09

Initial public release.

### Added
- Three-pane TUI: search box, load-sorted process list, context-sensitive process tree.
- Substring search DSL with `pid:`, `ppid:`, `user:`, `name:`, `cmd:`, `state:` prefixes plus bare terms (case-insensitive substring across name + cmdline + user). Space-separated terms are AND-ed. `pid:<X>` auto-scrolls and highlights without filtering.
- Sort modes (CPU%, RSS, TIME+, AGE) cycled with `s`.
- Tree pane: spine of ancestors + DFS subtree of the load-view selection, with its own cursor and `Enter`-drill into a selected PID.
- Signal modal (`K`): TERM / KILL / HUP / INT / USR1 / USR2 / STOP / CONT, with confirm-required flow for PID 1 and self-signal.
- Pause toggle (`space`).
- Status line with focus, counts, sort, paused, load, mem, and a hint/flash.
- Help modal (`?`).
- CLI flags: `--filter <expr>`, `--interval <secs>`, `--no-kernel-threads`.
- Linux backend via `procfs` (Phase 1).
- macOS backend via `libproc` + `sysctl` + `host_statistics64` (Phase 6).
- Color rules: STATE colors (R/D/Z/T), CPU% thresholds (yellow ≥ 50%, red ≥ 80%, dim < 1.0), kernel-thread row dimming, USER cyan on parent transition in the tree, search prefix tokens bold cyan.
- Age formatter with the largest-two-units rule (`1d4h`, `4h12m`, `12m32s`, `32s`).
- Bytes formatter (binary units).
- Kernel-thread marking: transitive descendants of PID 2 on Linux.
- Empty-cmdline fallback renders `[<comm>]`.
- Terminal-too-small fallback when below `MIN_COLS` × `MIN_ROWS`.
- Vim-style key bindings throughout (j/k, gg/G, Ctrl-d/Ctrl-u, Tab/Shift-Tab focus cycling, `/` jump-to-search, `Esc` return-to-search).
