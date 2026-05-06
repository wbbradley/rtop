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

## Phase 2 — Search + load view interactivity

Wired up the search box, search DSL, fuzzy filter, load-view interactivity, sort cycling, and pause:

- Added `nucleo-matcher` (0.3.1) via `cargo add` for synchronous per-keystroke fuzzy match. Documented refinement from the original `nucleo` crate listed in PLAN.md (we want `Matcher::fuzzy_match`, not the injection/orchestrator surface).
- `src/consts.rs`: added `SEARCH_BOX_HEIGHT: u16 = 3` and `LOAD_VIEW_HEIGHT: u16 = 13`.
- `src/format.rs`: added `time_plus(Duration)` (`1h23m` / `12m45s` / `45s`), reused for both TIME+ and AGE this phase. Phase 7 will refine.
- `src/search.rs` + `src/search/parser.rs` + `src/search/filter.rs`: DSL (`pid:`, `ppid:`, `user:`, `name:`, `cmd:`, `state:` + bare fuzzy) parsed into `Query { terms, auto_select_pid }`, AND across terms; bare terms fuzzy-match against `name + " " + cmdline + " " + user`. `pid:` does not filter — it sets `auto_select_pid` and the load view scrolls/highlights to it. Sort dispatched on `SortKey` (CPU desc with None-last, RSS desc, TIME+ desc on `cpu_time_total`, AGE desc on `age`).
- `src/app/event.rs`: `Focus { Search, Load }` and `SortKey { Cpu, Rss, TimePlus, Age }` with `next()` cycler and `label()` accessor.
- `src/app/state.rs`: `App` now carries `focus`, `query_text`, `query`, `paused`, `sort`, `load_cursor`, `load_view_offset`, `filtered_indices`, `pending_g`. Constructor `App::new(initial_filter)` parses the initial filter. `refilter()` reparses, refilters, honors `auto_select_pid`, and clamps the cursor.
- `src/app.rs`: focus-aware `handle_key` dispatcher. Search focus accepts printable input, Backspace, Esc (clears non-empty query), Tab/Shift-Tab → Load, Enter → Load (cursor=0), Ctrl-n/Ctrl-p move load cursor without leaving search. Load focus accepts j/k/G, gg (two-key chord via `pending_g`), Ctrl-d/Ctrl-u half-page, `s` to cycle sort, space to toggle pause, `/` clears + jumps to search, Esc/Tab/Shift-Tab return to search, Enter drills with `pid:<X>`. Pause is implemented as the main thread ignoring incoming snapshots while `paused` is set.
- `src/ui.rs`: vertical layout split into search (3) / load (13) / tree (Min(0)) with a tree placeholder block. Loading state shows in the load pane until the first snapshot arrives.
- `src/ui/search_box.rs`: bordered single-line input with horizontal scroll keeping the cursor visible; prefix tokens (`pid:` etc.) highlighted bold cyan; cursor only drawn when search has focus.
- `src/ui/load_view.rs`: full column set (PID/USER/CPU%/RSS/TIME+/STATE/AGE/COMMAND); STATE color-coded (R green, D red, Z red bold, T yellow, others default); selected row reverse video; `SCROLLOFF`-aware viewport offset clamp computed against actual rendered visible rows (capped at `LOAD_VIEW_VISIBLE_ROWS`).
- `src/main.rs`: added `--filter <expr>` CLI flag (clap derive); threaded through `app::run`.
- 24 new unit tests (37 total) cover parser edge cases (empty, whitespace, prefix recognition, pid-as-int vs fail-open, embedded colon, trailing colon, multi-term AND, unknown prefix), filter behavior against a 5-process fixture (identity, user/name/state filters, bare fuzzy `firef`→firefox, two-term AND, `pid:` returns full list, `ppid:` filters normally), `time_plus` boundaries, and `SortKey` cycling/labels.
- `chk` clean; `cargo nextest run` green.

## Phase 3 — Tree pane

Added the third pane below the load view: spine of ancestors + DFS of the load-view-selected process's subtree, with its own cursor and `Enter`-drill.

- `src/tree.rs`: pure data layer with `GutterKind { Spine, Branch, Leaf }` and `TreeNode { proc_idx, depth, gutter_kind, is_last_child, ancestors_last }`. Helpers `build_pid_to_idx`, `build_parent_to_children` (children sorted by pid for deterministic rendering), and `build_visible(snap, p2c, pid_to_idx, selected)`. The chain walk defends against `ppid == 0`, self-parent cycles, and orphans whose parent disappeared mid-sample. DFS pushes/pops `is_last` onto a working `ancestors_last` stack.
- `src/app/event.rs`: `Focus` extended with `Tree` variant.
- `src/app/state.rs`: `App` carries `tree_visible`, `tree_cursor`, `tree_offset`, `tree_cache_key: Option<(Arc-ptr-as-usize, ProcessId)>`, `tree_cursor_id`. `selected_process_id()` helper. `ensure_tree_built()` short-circuits via the cache key, otherwise rebuilds; the cursor jumps to the newly-selected node when the load-view selection changes, and re-anchors by `tree_cursor_id` across snapshot ticks when selection is unchanged. `adjust_tree_offset_for_scrolloff()` mirrors the load-view variant.
- `src/app.rs`: `run_loop` calls `ensure_tree_built()` before each `terminal.draw`. `handle_key` dispatches Tree focus to `handle_tree_key`. Tab/BackTab cycles updated: Search → Load → Tree → Search. Tree handler implements `j`/`k`/`gg`/`G`/`Ctrl-d`/`Ctrl-u` against `tree_visible.len()` (half-page reuses `LOAD_VIEW_VISIBLE_ROWS / 2`), `Esc`/`Tab` → Search, `BackTab` → Load, `/` clears query and jumps to Search, `Enter` commits `pid:<X>` to the search box and refilters.
- `src/ui.rs`: replaced the placeholder `tree` block with `tree_view::render`. `mod tree;` added in `main.rs`.
- `src/ui/tree_view.rs`: renders a `Paragraph<Vec<Line>>` per visible row in the format `{pid:>7} {cpu:>5} {rss:>8}  {gutter}{command}` plus an inline cyan `[user]` when the node's user differs from its parent's. Gutter glyphs are `│  ` / `   ` per ancestor column followed by `├─` / `└─` connector. Selected row uses reverse video. Diverged from PLAN.md's literal column ordering — fixed-width PID/CPU%/RSS come before the variable-width gutter so numeric columns stay aligned.
- 7 new unit tests (44 total): four `build_visible` cases (root / mid / leaf / missing selection), `ancestors_last_flags_branch` exercising the `[true, false]` flag sequence on a five-process fixture, plus two `App::ensure_tree_built` tests for cursor reset on selection change and cursor preservation across snapshot ticks.
- `chk` clean; `cargo nextest run` green (44 passed).

## Phase 4 — Status line, help modal, empty/error states

Added the UX scaffolding around the three working panes — persistent status line, `?`-triggered help modal, empty filter state, terminal-too-small fallback, and the flash infrastructure that Phase 5 will hook into:

- `src/consts.rs`: added `STATUS_LINE_HEIGHT: u16 = 1`, `HELP_MODAL_WIDTH: u16 = 60`, `HELP_MODAL_HEIGHT: u16 = 20`.
- `src/app/event.rs`: `Focus::label()` returning `"search"` / `"load"` / `"tree"`. Dropped `#[allow(dead_code)]` on `SortKey::label` (now live in the status line). Added `focus_label_distinct` test.
- `src/app/state.rs`: `App` carries `help_open: bool` and `flash: Option<(String, Instant)>`. Free helpers `hint_for(focus)` (adaptive per-pane hint string) and `flash_active(&flash, now)` (returns `Some(s)` while inside `ERROR_FLASH_DURATION`, else `None`). `App::set_flash(msg)` records the current `Instant`; `#[allow(dead_code)]` until Phase 5 wires it.
- `src/app.rs`: `handle_key` short-circuits modal handling and the `?` toggle BEFORE focus dispatch, so the search-focus printable-char branch never sees `?`. While `help_open`, only `Esc` and `?` are honored; everything else is swallowed.
- `src/ui.rs`: re-architected `draw` — checks `frame.area()` against `MIN_COLS`/`MIN_ROWS` first and renders a single centered `terminal too small (W×H < MIN×MIN)` message when below; otherwise vertical split is `SEARCH_BOX_HEIGHT` / `LOAD_VIEW_HEIGHT` / `Min(0)` / `STATUS_LINE_HEIGHT`. Help modal renders last over the full area.
- `src/ui/status_line.rs`: two-pass paragraph rendering — left-aligned focus + counts + sort + paused + load/mem groups; right-aligned hint (dim) or current flash (red bold). Pre-snapshot fallback shows `—/— procs` and `…sampling`. Bytes via `format::bytes`.
- `src/ui/help_modal.rs`: centered `HELP_MODAL_WIDTH × HELP_MODAL_HEIGHT` rect cleared with `Clear`, bordered block titled ` help `, tabular `key → action` rows under bold section headers (`[ search ]`, `[ load ]`, `[ tree ]`, `[ any ]`). Private `centered_rect` helper.
- `src/ui/load_view.rs`: when `filtered_indices` is empty, renders a dim+italic `no matches` centered inside the bordered load pane and returns before laying out the table.
- 5 new unit tests (49 total): `focus_label_distinct`, `hint_for_each_focus`, `flash_active_returns_some_within_window`, `flash_active_returns_none_after_window`, `flash_active_none_when_unset`.
- `chk` clean; `cargo nextest run` green (49 passed).

## Phase 5 — Signal modal

Wired up `K`-triggered signal sending against the focused pane's cursor:

- Added `nix` (0.31.2) via `cargo add` with `--features signal,process` for `Signal`, `kill`, `Pid`, and `Errno`.
- `src/consts.rs`: `SIGNAL_MODAL_WIDTH = 44`, `SIGNAL_MODAL_HEIGHT = 14`.
- `src/signal.rs` (new): `SignalChoice { signal, label }` and `SIGNAL_CHOICES` — the canonical TERM / KILL / HUP / INT / USR1 / USR2 / STOP / CONT catalog driving both modal rendering and dispatch.
- `src/app/state.rs`: new `SignalModal { target_pid, target_label, cursor, awaiting_confirm }` colocated with `App`. `App` carries `signal_modal: Option<SignalModal>`. Free helpers `needs_confirm(pid, self_pid)` (true iff `pid == 1 || pid == self_pid`) and `signal_target(&App)` (resolves focused-pane cursor → `(pid, "PID <pid>  <cmdline-or-comm>")`, with `[<comm>]` fallback for empty cmdline). Dropped `#[allow(dead_code)]` on `set_flash`.
- `src/app.rs`: `handle_key` short-circuits to `handle_signal_modal_key` whenever the modal is open (after the help-modal short-circuit). Each focus handler grew a `Char('K')` arm calling `open_signal_modal`. Modal handler implements `j`/`k`/`Down`/`Up` to move cursor (saturating, no wrap), `Esc` to cancel, `Enter` to send (or flip into `awaiting_confirm` if `needs_confirm`); confirm state takes only `y`/`Y` (sends) or `Esc` (closes), any other key flips back to selection. `send_signal` calls `nix::sys::signal::kill(Pid::from_raw(pid), Some(sig))` and flashes `EPERM: …`, `ESRCH: …`, or a generic message on failure; success is silent.
- `src/ui/signal_modal.rs` (new): centered, bordered, ` send signal ` title (or ` confirm? (y/N) ` while `awaiting_confirm`); first row is the target label clipped to inner width; signal rows render `SIG<NAME>` with the cursored row in reverse video and `TERM` bold cyan when not selected; final row is dim hint `j/k pick · Enter send · Esc cancel`. `ui::draw` renders the signal modal AFTER the help modal so it sits on top if both somehow open.
- 7 new unit tests (56 total): `needs_confirm_pid_1_true`, `needs_confirm_self_true`, `needs_confirm_other_false`, plus `signal_target` cases for Search/Load/Tree focus + a no-snapshot None case. Reuses the existing 4-process `snap()` fixture.
- `chk` clean (clippy folded the cursor-bound check into a match guard); `cargo nextest run` green (56 passed).
