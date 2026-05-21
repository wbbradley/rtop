# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-05-20

### Breaking Changes
- Dropped the load pane. The TUI is now two panes — search box on top, tree below — and the tree filters itself directly off the search query. For every matching process the tree shows its full parent chain (root → match) plus its complete subtree; multiple matches across the forest become separate roots. Empty query → full forest; no matches → empty tree.
- Removed the sort cycle (`s` key) and its `[sort: …]` status indicator. Tree order is parent-chain + DFS, with sibling order by PID.
- `Esc` in the search box now unconditionally clears the query (previously it only cleared when the query was non-empty).
- `pid:<X>` now filters processes whose PID equals `X` (multiple values via OR-groups: `pid:42, pid:7`). Previously `pid:` was special-cased to highlight without filtering; the cursor still auto-positions on the first match.

### Changed
- Focus cycles between just `search` and `tree`. `Tab` / `Shift-Tab` flip between them. `Ctrl-n` / `Ctrl-p` in the search box step the tree cursor without leaving search focus. `Enter` in search jumps focus to the tree and parks the cursor on the first match.
- Tree-pane half-page scroll (`Ctrl-d` / `Ctrl-u`) now derives from the new `TREE_HALF_PAGE` constant (10 rows).

## [0.1.5] - 2026-05-19

### Added
- Focused pane indicator: the active pane's border and title now render in a distinct orange accent (`Color::Rgb(254, 128, 25)`). Cycles with `Tab` / `Shift-Tab`. Non-focused panes keep the default border; the reverse-video cursor row inside Load and Tree is unchanged. Terminals without truecolor fall back to the nearest representable color via ratatui's backend.

## [0.1.4] - 2026-05-19

### Changed
- Tree pane now soft-wraps long commands onto continuation lines instead of running off the right edge. Wrapping breaks on argv-token boundaries, hard-breaks single tokens wider than the pane, and caps any one row at 3 visual lines (with a trailing `…` when more would follow). Continuation lines are indented to the command's start column and preserve the ancestor spine `│` characters. Selection highlight and kernel-thread dimming span all visual lines of the wrapped row. Cursor navigation (`j`/`k`/`gg`/`G`/`Ctrl-d`/`Ctrl-u`) still moves by logical rows; continuations are invisible to the cursor.

## [0.1.3] - 2026-05-18

### Added
- Comma-OR operator in the search DSL. Comma separates top-level OR-groups; space-separated terms within a group still AND. Commas embedded in a single whitespace-delimited token remain literal (e.g. `user:root,alice` is one term with value `root,alice`; `user:root, user:alice` is two OR-groups).
- Help modal: `,` listed under `[ search ]` as `OR groups`.

## [0.1.2] - 2026-05-18

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
