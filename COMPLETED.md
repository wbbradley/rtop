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

- Added `nucleo-matcher` (0.3.1) via `cargo add` for synchronous per-keystroke fuzzy match. (superseded by Phase 5.5) Documented refinement from the original `nucleo` crate listed in PLAN.md (we want `Matcher::fuzzy_match`, not the injection/orchestrator surface).
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

## Phase 6 — macOS backend

Stood up `MacOsProcessSource` so rtop runs on macOS with the same UI surface as Linux:

- Added macOS-conditional deps via `cargo add --target 'cfg(target_os = "macos")' libc libproc mach2`. Moved the existing `procfs` dep under `[target.'cfg(target_os = "linux")'.dependencies]` so non-Linux builds skip its build script (which hard-fails on non-Linux).
- `src/source.rs`: cfg-gated `pub mod macos` + `pub use macos::MacOsProcessSource as PlatformSource;` mirroring the Linux block.
- `src/source/macos.rs` (new): `MacOsProcessSource` with a `pidrusage(getpid())` ctor probe (mirrors Linux's `/proc/self/stat` probe), cached `argmax` (`sysctl(CTL_KERN, KERN_ARGMAX)`), `_SC_PAGESIZE`, and a reusable scratch `args_buf`. Pivoted from the plan's `kinfo_proc` approach (libc 0.2 doesn't expose that struct on macOS) to `libproc::processes::pids_by_type(ProcFilter::All)` + per-pid `pidinfo::<BSDInfo>` for ppid/uid/state/start/comm and `pidrusage::<RUsageInfoV2>` for `ri_user_time`/`ri_system_time` (fed into `cpu_time_total`) and `ri_resident_size`. Full argv via `sysctl(KERN_PROCARGS2)` into the scratch buffer; `parse_procargs2` extracted as a pure function so it can be unit-tested without the kernel. State map: SIDL/SRUN/SSLEEP/SSTOP/SZOMB → `I`/`R`/`S`/`T`/`Z`. `start_time = secs * 1_000_000 + usecs` for stable `ProcessId` identity across PID reuse. Skip pid 0 (kernel_task surrogate). On per-pid `pidinfo`/`pidrusage` failure (sandboxed, `EPERM`), `continue` — same shape as Linux's `NotFound` skip.
- Memory: `sysctl(CTL_HW, HW_MEMSIZE)` for total. `host_statistics64(HOST_VM_INFO64)` for VM stats; `mach2 0.6` doesn't expose `host_statistics64` so it's declared as a private extern and `HOST_VM_INFO64` is a local const. `used = (active + wired + compressed) * page_size` (Activity Monitor's "memory used" formula). Load avg via `libc::getloadavg(loads, 3)`.
- `is_kernel_thread = false` always on macOS (no equivalent of Linux's PID 2 subtree); the renderer already handles this case.
- `.github/workflows/ci.yml`: dropped `continue-on-error: true` from the macOS job — it must now pass.
- README platform-support section now lists Linux + macOS as full-support.
- 4 new unit tests (62 total): `smoke` (asserts `getpid()` is in the live snapshot), `map_state_known_values`, `parse_procargs2_basic` (hand-built buffer with `argc=2 | exec_path | padding | argv[0] | argv[1] | env`), `parse_procargs2_empty_on_short_buffer`, `parse_procargs2_zero_argc`.
- `chk` clean on macOS (rustfmt + clippy `-D warnings`); `cargo nextest run` green (62 passed); release build succeeds.

## Phase 7 — Polish

Final ship-quality polish — color rules, age formatter, kernel-thread filter, magic-number audit, README feature tour:

- `src/consts.rs`: added `EVENT_CHANNEL_CAP: usize = 64`, `MACOS_ARGMAX_FALLBACK: usize = 256 * 1024`, `KERNEL_THREAD_PARENT_PID: i32 = 2`.
- `src/format.rs`: implemented `age()` for the largest-two-units rule (`1d4h`, `4h12m`, `12m32s`, `32s`); local consts `SECS_PER_MIN`/`SECS_PER_HOUR`/`SECS_PER_DAY`. Removed `#[allow(dead_code)]`. Replaced the `age_seconds` stub test with 10 boundary cases (zero, sub-minute, minute boundary, mid-hour, hour boundary, just-under-a-day, day boundary, days+hours).
- `src/source/linux.rs`: kernel-thread marking is now a transitive BFS from `KERNEL_THREAD_PARENT_PID` over a `parent → children` map built once after the per-pid loop. Replaces the in-loop `ppid == 2 || pid == 2` heuristic. Added `HashSet`/`VecDeque` imports.
- `src/source/macos.rs`: `read_argmax().unwrap_or(MACOS_ARGMAX_FALLBACK)` instead of an inline literal.
- `src/ui/load_view.rs`: new `cpu_cell()` helper applies CPU% color thresholds — red ≥ `CPU_DANGER_PCT`, yellow ≥ `CPU_WARN_PCT`, dim < 1.0, dim `—` for `None`. Whole row gets `Modifier::DIM` when `is_kernel_thread` (composes with `Modifier::REVERSED` for selection). AGE column switched from `format::time_plus` to `format::age`.
- `src/ui/tree_view.rs`: matching `cpu_span()` helper returning a `Span<'static>` with the same threshold rules; preserves the existing `{:>5.1}` width. Whole `Line` gets `Modifier::DIM` when `is_kernel_thread`.
- `src/main.rs`: added `--no-kernel-threads` clap flag; threaded through `app::run`. `--interval` default-value comment now cross-references `consts::SAMPLE_INTERVAL`.
- `src/app.rs`: `run` / `run_loop` accept `hide_kernel_threads: bool`; `bounded::<Event>(EVENT_CHANNEL_CAP)`.
- `src/app/state.rs`: new `App::hide_kernel_threads` field; `App::new(initial_filter, hide_kernel_threads)`; `refilter()` retains only non-kernel-thread indices when the flag is set. Updated all 6 existing test call sites; added two new tests (`no_kernel_threads_excludes_kernel_threads`, `kernel_threads_included_when_flag_off`) backed by a `snap_with_kernel_threads()` fixture that flags pids 2 and 4.
- `README.md`: added `## Features` section between `## Status` and `## Screenshot` covering the three-pane TUI, search DSL, sort modes, tree, signal modal, pause, CLI flags, and Linux+macOS support.
- 11 new unit tests (84 total): 9 net-new `format::age` boundaries (replacing the 1-case stub) + 2 kernel-thread filter tests.
- `chk` clean; `cargo nextest run` green (73 tests on macOS — 11 Linux-only smoke + parser cases skipped via cfg).

## Phase 5.5 — Substring search (drop fuzzy)

Replaced the bare-term fuzzy matcher with case-insensitive substring matching so all terms (prefixed and bare) share the same semantics; dropped the `nucleo-matcher` dependency:

- `src/search/filter.rs`: removed the `Matcher`/`Utf32Str` setup, `hay_buf`/`needle_buf` scratch buffers, and the `nucleo_matcher` import. `term_matches` simplifies to `fn term_matches(p: &Process, term: &Term) -> bool`. The `Term::Bare(s)` arm now reuses the existing `contains_ci` helper against `name + " " + cmdline + " " + user`.
- `Cargo.toml`: `cargo remove nucleo-matcher` (also drops it from `Cargo.lock`); `description` updated from "fuzzy search" to "substring search".
- `PLAN.md`: Architecture Reference bare-terms bullet now reads "case-insensitive substring match against …"; `nucleo` removed from the Crates list; top-of-file blurb updated to "substring search box".
- `README.md`: top-of-file blurb updated to "substring search box".
- 3 test updates in `src/search/filter.rs`: renamed `bare_fuzzy_matches_cmdline` → `bare_substring_matches_cmdline` (kept `firef` positive case, added `firefox` positive case, added `frfx` negative case that was a fuzzy-only hit pre-5.5); added `bare_term_is_case_insensitive` (`FIREFOX` matches PID 202); added `multi_bare_terms_and` (`bash wbbradley` matches PID 101 only).
- `chk` clean; `cargo nextest run` green (58 passed).

## Shrink load pane to 4 visible data rows by default

Made the tree pane the dominant region by default — filtering is the primary mode of use, so the load pane now occupies just 7 rows instead of 13:

- `src/consts.rs`: `LOAD_VIEW_VISIBLE_ROWS: usize` 10 → 4; `LOAD_VIEW_HEIGHT: u16` 13 → 7 (top border + header + 4 data rows + bottom border).
- `PLAN.md`: Architecture Reference updated to match — load view description `~13 rows` → `~7 rows` and constants list `LOAD_VIEW_VISIBLE_ROWS: usize (10)` → `(4)`.
- No other code changes required. `src/ui.rs` reads `LOAD_VIEW_HEIGHT` for vertical layout; `src/ui/load_view.rs` clamps `visible_rows = max_rows.min(LOAD_VIEW_VISIBLE_ROWS)` so the smaller cap takes effect automatically; `src/app.rs` derives the half-page distance as `(LOAD_VIEW_VISIBLE_ROWS / 2).max(1)` which becomes 2 (still a valid half page); `SCROLLOFF = 3` is independent of pane size.
- `chk` clean; `cargo test` green (69 passed).

## Slow default refresh interval from 1s to 5s

Bumped the default sample/refresh cadence from 1s to 5s for a calmer passive monitor, and wired the CLI default to the constant so the two can't drift again:

- `src/consts.rs`: `SAMPLE_INTERVAL` `Duration::from_secs(1)` → `Duration::from_secs(5)`.
- `src/main.rs`: clap `--interval` `default_value_t = 1.0` → `default_value_t = consts::SAMPLE_INTERVAL.as_secs_f64()`; doc comment updated to `(5.0s)`. `default_value_t` accepts arbitrary expressions evaluated at `Command`-build time, so the non-const `Duration::as_secs_f64()` call is fine.
- `src/sampler.rs`: drop-NEWEST rationale comment updated from "With 1s ticks…" to "With a multi-second tick…" so it no longer tracks the old default.
- `PLAN.md` Architecture Reference: two stale "1s" references updated to "5s" (data-flow section and constants list).
- `CHANGELOG.md`: added `[Unreleased]` section above `[0.1.1]` noting the bump and `--interval 1` as the escape hatch.
- `cargo run -- --help` confirms `[default: 5]`; `chk` clean; `cargo test` green (69 passed). CPU% math was unaffected (already interval-relative).

## Comma-separated OR terms in search

Extended the search DSL with a top-level OR operator while preserving the existing AND-of-terms semantics inside each group:

- `src/search/parser.rs`: replaced `Query { terms: Vec<Term>, … }` with `Query { groups: Vec<Vec<Term>>, … }`. `parse()` now whitespace-tokenizes, peels leading/trailing commas off each token as OR-separators (a token of all commas peels both ways), runs the existing single-term logic on the residue, and drops empty groups silently. Commas embedded in a token (e.g. `user:root,alice`, `firefox,vim`) remain literal. `auto_select_pid` is the first valid `pid:<int>` encountered left-to-right across all groups.
- `src/search/filter.rs`: retain predicate became `groups.iter().any(|g| g.iter().all(term_matches))`. Empty `groups` ⇒ no filtering. A pid-only group still matches everything (consistent with the existing single-group `pid:` behavior).
- `src/ui/help_modal.rs`: added a `","` → `OR groups` binding under `[ search ]`.
- `README.md`: DSL bullet updated to "Space-separated terms within a group are AND-ed; a comma separates OR-groups."
- `PLAN.md`: Search DSL section updated; `search OR/negation` removed from the out-of-scope list (only `search negation` remains).
- Comma styling in the search box was deliberately deferred to keep the diff focused; separator commas render plain.
- New parser tests (15) cover token-boundary commas, literal commas inside tokens, leading/trailing/multiple commas, comma-only tokens, multi-group queries, and `pid:` in non-first groups. New filter tests (5) cover OR of bare terms, OR of prefixed terms, AND-within-OR-across, pid-only OR group, and all-empty-groups inputs. `chk` clean; `cargo test` green (86 passed).

## Tree view: wrap long commands on argv-token boundaries

Made the tree pane soft-wrap long argv onto continuation lines indented to the command's start column, so the full command is readable without horizontal scrolling. Tree view only — load view unchanged.

- `src/ui/tree_view.rs`: rewrote `build_line` → `build_row(p, node, parent_user, pane_width) -> Vec<Line<'static>>`. New pure helpers `wrap_argv` (greedy token packing with first/cont budgets, hard-break at column edge, ellipsize on overflow), `row_budgets`, `row_visual_height`, `continuation_prefix`. Refactored `clamp_offset` to take a per-row height fn — top scrolloff, then walk-back-from-`last_idx`-summing-heights to enforce bottom scrolloff, then "cursor visible" overrides everything. The 24-col metadata block and ancestor `│  ` spine chars are preserved on continuation lines; only the current node's `└─ ` / `├─ ` is blanked.
- `render()` now precomputes a per-row heights vector, calls `clamp_offset`, and renders logical rows from the clamped offset until `visible_rows` visual lines are consumed. Selection styling (`Modifier::REVERSED`) is applied per-line so the highlight covers all wrap continuations. Kernel-thread `Modifier::DIM` composes the same way.
- Cap: `MAX_TREE_WRAP_LINES = 3` local const; ellipsis (`…`) terminates the 3rd line when more would follow. Kernel-thread / no-cmdline rows still render as `[name]` on a single line. No new `consts.rs` entries; no changes to `app.rs`, `app/state.rs`, `consts.rs`, `tree.rs`, or `load_view.rs`.
- 13 new unit tests (99 total): `wrap_argv` boundary cases (short, packing, token-boundary wrap, hard-break, hard-break-with-truncation, 3-line cap with ellipsis, asymmetric first/cont budgets, zero-first-budget guard); `clamp_offset` (empty, uniform top scrolloff, uniform bottom scrolloff, tall row forces higher offset, cursor row taller than viewport top-aligns cursor).
- `chk` clean (clippy applied 2 micro-fixes); `cargo test` green (99 passed).

## Focused pane indicator: orange border on the active pane

Made the focused pane visually distinct by coloring its border and title in a truecolor orange (`Color::Rgb(254, 128, 25)`). First deliberate truecolor use in the codebase — every other semantic color (CPU, STATE, kernel-thread dim, USER, search-prefix cyan, errors) still uses named ANSI variants.

- `src/consts.rs`: added `FOCUS_ACCENT: Color = Color::Rgb(254, 128, 25)` and the `use ratatui::style::Color;` import.
- `src/ui.rs`: added `pub fn focused_block(title, focused) -> Block<'static>` helper — returns a bordered+titled block, with `border_style` + `title_style` set to `FOCUS_ACCENT` when focused. Applied to the pre-snapshot "loading…" placeholder block.
- `src/ui/search_box.rs`, `src/ui/load_view.rs`, `src/ui/tree_view.rs`: replaced direct `Block::bordered().title(...)` construction with `crate::ui::focused_block(...)` against the relevant `app.focus == Focus::*` predicate. Dropped the now-unused `Block` import from each.
- `PLAN.md` Architecture Reference: Visual rules bullet relaxed from "ANSI 16 only in v1, no truecolor, no theming" to "ANSI 16 for semantic colors … One truecolor accent (FOCUS_ACCENT, RGB 254,128,25) reserved for the focused-pane indicator. No theming." Out-of-v1-scope dropped `truecolor` from `theming/truecolor`.
- No changes to `app/state.rs`, `app/event.rs`, key handling, status line, help modal, or signal modal. Reverse-video cursor row inside Load and Tree is untouched.
- `chk` clean; `cargo test` green (99 passed). No new tests — render path is exercised end-to-end manually per the task spec.

## Make `pid:` filter the load view to the matched PID

Eliminated the special case that made `pid:<X>` highlight-without-filter, so the `Enter`-drill keybinding now actually narrows the load pane.

- `src/search/filter.rs`: `Field::Pid` arm of `term_matches` now does `p.id.pid == value.parse::<i32>()`, mirroring `Field::Ppid`. Dropped the `// pid: does NOT filter` comment.
- `src/search/filter.rs` tests: renamed `pid_does_not_filter` → `pid_filters_to_exact_match`; renamed `or_with_pid_only_group_matches_all` → `or_pid_and_nonexistent_name_filters_to_pid` with new expectation `vec![42]`. Added `pid_or_group_unions_pids` (`pid:42, pid:303` → both PIDs, cursor lands on 42) and `pid_nonexistent_yields_no_matches`.
- `PLAN.md` Architecture Reference, Search DSL: replaced the "pid:X is special" bullet with the new "filters by exact PID equality; cursor auto-positions on first match" wording.
- `CHANGELOG.md`: added an `[Unreleased] / ### Changed` entry flagging the breaking change for anyone using `--filter pid:<X>` to keep all rows visible.
- `auto_select_pid` cursor-positioning in `App::refilter()` works as-is; no `app/state.rs` changes. Help modal didn't mention `pid:` semantics; no change.
- `chk` clean; `cargo test` green (101 passed, +3 vs. prior 99 — the prior count grew with the renamed/added tests).


## Drop the load pane; tree shows union of matches' ancestors + descendants

Re-centered the UI on the search box + tree pane. The load pane was a redundant intermediary; the tree now filters itself directly off the query, and the load pane is gone.

- `src/app/event.rs`: removed `Focus::Load` and the entire `SortKey` enum + tests.
- `src/app/state.rs`: replaced `load_cursor`, `load_view_offset`, `sort`, `filtered_indices`, and the load-cursor-keyed `tree_cache_key` with `matched_pids: HashSet<i32>` and a `(snapshot_ptr, query_text)` cache key. `refilter()` now just populates `matched_pids`. `signal_target` always reads the tree cursor. New `jump_tree_cursor_to_first_match()` helper for the search-Enter handoff. Replaced cursor-anchoring tests with `tree_cursor_preserved_if_pid_visible` / `tree_cursor_jumps_to_first_match_when_old_pid_pruned`.
- `src/tree.rs`: replaced `build_visible(snap, p2c, pid_to_idx, selected)` with `build_filtered(snap, p2c, pid_to_idx, matched, hide_kernel_threads)`. Algorithm: closure over `matched` adding all ancestors (parent chain) and all descendants (BFS); kthread PIDs masked out when `hide_kernel_threads`. Roots = kept procs whose parent isn't kept; sorted by PID. Recursive DFS emits `TreeNode`s with `is_last_child` / `ancestors_last` computed against the visible-kept siblings (so `└─` vs `├─` is correct after pruning). Dropped `GutterKind::Spine` — only `Branch` and `Leaf` remain.
- `src/app.rs`: removed `handle_load_key`, `current_selected_pid`, the load `move_cursor` / `half_page` family. `handle_search_key`: `Tab` and `BackTab` both jump to `Focus::Tree`; `Enter` focuses the tree and jumps the cursor to the first match; `Ctrl-n` / `Ctrl-p` drive the tree cursor; `Esc` unconditionally clears the query. `handle_tree_key`: added `space` (pause), `Esc`/`Tab`/`BackTab` all return to Search, half-page uses `TREE_HALF_PAGE`.
- `src/search/filter.rs`: extracted `pub fn matches(query, p) -> bool` (the per-process predicate). Deleted the no-longer-used `pub fn filter(query, snap) -> Vec<usize>` and the SortKey-keyed `compare` / `sort_indices`. Added unit tests for each prefixed term type, bare-term substring, AND-within-OR, and comma-OR.
- `src/ui.rs`: layout is now `SEARCH_BOX_HEIGHT` / `Min(0)` / `STATUS_LINE_HEIGHT`. The pre-snapshot "loading…" placeholder now renders inside the tree-pane block. Dropped `pub mod load_view`.
- `src/ui/load_view.rs`: deleted.
- `src/ui/tree_view.rs`: dropped the `GutterKind::Spine` arm in `build_gutter`; otherwise unchanged (iterates the new `tree_visible`).
- `src/ui/status_line.rs`: dropped `[sort: cpu]`. `<matched>/<total> procs` uses `matched_pids.len()` for the matched count.
- `src/ui/help_modal.rs`: rewrote contents: `[ search ]` / `[ tree ]` / `[ any ]` sections only. Documented the new Esc semantics, Enter handoff, `K`, `space`, and `/` in the tree.
- `src/format.rs`: deleted `age()`, `time_plus()`, and the `SECS_PER_*` constants — they were only used by the removed load pane.
- `src/consts.rs`: removed `LOAD_VIEW_VISIBLE_ROWS` and `LOAD_VIEW_HEIGHT`. Added `TREE_HALF_PAGE: usize = 10`.
- `PLAN.md` Architecture Reference: Layout collapsed from 3 panes to 2; Data flow updated to describe the chain+subtree closure; Key bindings table rewritten; Constants discipline lists `TREE_HALF_PAGE` and drops the load constants. `pid:` DSL bullet updated.
- `README.md`: rewrote Features to advertise the two-pane match-driven tree model.
- `CHANGELOG.md` `[Unreleased]`: added `### Changed` entries flagging the breaking removals (load pane, sort cycle, Esc semantics) and the focus-cycle change.
- `chk` clean; `cargo test` green (80 passed; older load-pane tests retired).

## Comma without whitespace splits OR-groups; empty matched renders `(no matches)`

Fixed two compounding bugs that caused `bash,dbus-daemon` (no spaces) to render the entire process forest:

- `src/search/parser.rs`: rewrote the tokenization loop. For each whitespace-delimited token, split on `,` and close the current OR-group between fragments; non-empty fragments are fed fresh through `push_term` (so the post-comma fragment does not inherit the prior fragment's prefix). Runs of commas / leading / trailing commas collapse via the existing empty-`current` `close_group` guard. Removed the two tests that locked in the old "comma inside token is literal" behavior; added four positives: `comma_inside_bare_token_splits_into_two_groups`, `comma_inside_prefixed_token_splits_and_fragment_does_not_inherit`, `comma_separated_prefixed_terms_keep_each_prefix`, `runs_of_commas_collapse_without_whitespace`.
- `src/tree.rs`: `build_filtered`'s `matched` argument switched from `&HashSet<i32>` to `Option<&HashSet<i32>>` so the caller can distinguish "no filter active" (`None` → full forest) from "query matched nothing" (`Some(empty)` → empty result). Updated doc comment. Renamed `build_filtered_empty_matched_shows_all` → `build_filtered_none_shows_all`, `build_filtered_multiple_roots_with_empty_matched` → `build_filtered_multiple_roots_with_no_filter`; added `build_filtered_some_empty_returns_empty`. Test helper split into `build` (Some) and `build_unfiltered` (None).
- `src/app/state.rs`: `ensure_tree_built` now passes `None` when `query.groups.is_empty()`, otherwise `Some(&matched_pids)`. The "empty visible tree" branch below already clears the cursor and caches the key.
- `src/ui/tree_view.rs`: when the tree is empty and the query is non-empty, render a centered dim `(no matches)` placeholder inside the tree block's inner area. Empty query + empty tree (no snapshot yet) keeps the silent return.
- `src/search/filter.rs`: added `matches_or_across_groups_no_whitespace` (`name:firefox,name:vim` matches 202 and 303 but not 1).
- `chk` clean; `cargo test` green (84 passed, +4 vs. prior 80).

## Regex search terms with amber match highlighting

Turned the string-valued search terms into case-insensitive, unanchored regexes and paint their matches amber in the visible tree rows:

- `cargo add regex` (1.13.0). `src/consts.rs`: added `SEARCH_MATCH_FG: Color = Color::Rgb(255, 176, 0)` (amber).
- New `src/search/compiled.rs`: `CompiledQuery` — the compiled parallel of the pure `Query`/`Term` AST (which stays intact, since `regex::Regex` is not `PartialEq`/`Eq`). Holds `groups: Vec<Vec<CompiledTerm>>`, a flat `highlight: Vec<Regex>` union for the renderer, `has_invalid`, and `empty`. `CompiledTerm` variants: `Pid(i32)`, `Ppid(i32)`, `State(char)`, `Str { field: StrTarget, re }`, `Invalid` (uncompilable → non-constraining), `Never` (`ppid:<non-int>` → never matches). String terms compile via `RegexBuilder::new(pat).case_insensitive(true).build()`; `Regex` is `Clone` (Arc-backed) so the highlight union clones cheaply. Local `first_char` helper (filter's copy retired). Three unit tests.
- `src/search.rs`: declared `compiled` module; re-exports `CompiledQuery`.
- `src/search/filter.rs`: `matches`/`term_matches` now take `&CompiledQuery`/`&CompiledTerm`; `Invalid` → non-constraining (`true`), `Never` → `false`, `Str` runs `re.is_match(..)` over the appropriate field. Retired `contains_ci`/`first_char`; kept `cmdline_joined`. Adapted the existing behavior tests via a `compiled(s)` helper; added regex-semantics tests (`^`/`$` anchors, `.*`, case-insensitivity) and invalid-skip tests.
- `src/app/state.rs`: `App` gained `pub compiled: CompiledQuery`, built in `App::new` and recomputed in `refilter` alongside `query`; the filter loop now calls `matches(&self.compiled, p)`. `query` stays canonical for the structural `groups.is_empty()` / `auto_select_pid` checks.
- `src/ui/tree_view.rs`: added `highlight_spans(text, &[Regex], base) -> Vec<Span>` (collects non-empty match ranges across all regexes, merges overlapping/adjacent, emits base/amber sub-spans; byte-slicing is UTF-8-safe on regex boundaries). `build_row` gained a `highlights: &[Regex]` param applied to the command text, the `[<name>]` fallback, the differing-user tag, and continuation lines; `render` passes `app.compiled.highlight_regexes()`. Selected-row `REVERSED` / kthread `DIM` stay at the `Line` level. Six `highlight_spans` tests.
- `src/ui/status_line.rs`: added `INVALID_REGEX_HINT`; right-slot precedence is kill-error flash (red bold) → dim "invalid regex" (when `app.compiled.has_invalid()`) → normal dim hint.
- Docs: PLAN.md Search DSL + Visual rules (second truecolor); README.md ("substring" → regex, amber highlight); CHANGELOG.md `[Unreleased]`.
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` clean; `cargo nextest run` green (104 passed, +20 vs. prior 84). Manual pty smoke of `name:^fire` and `fire(` — no panic.

Original PLAN.md entry, verbatim as it existed before work began:

````markdown
### Regex search terms with amber match highlighting

Turn the string-valued search terms into regular expressions and paint their matches in the visible tree rows with an amber foreground.

**Scope / semantics**

- String-valued terms become **unanchored, case-insensitive-by-default** regexes: bare terms and the `user:` / `name:` / `cmd:` prefixed terms. `pid:` / `ppid:` stay exact integer equality (they also drive `auto_select_pid` and the `Enter`-to-drill that writes `pid:<X>`), and `state:` stays single-char equality. The OR-of-AND-groups structure (comma = OR-group boundary, space = AND within a group) is unchanged.
- `regex::Regex` is unanchored by default, preserving today's substring feel; `^`/`$` anchors and inline flags become available. Case-insensitivity via `RegexBuilder::case_insensitive(true)`; a user can opt back into case sensitivity with an inline `(?-i)`.
- Keep the pure `Query`/`Term` AST in `search/parser.rs` and its equality-based unit tests intact — `regex::Regex` is not `PartialEq`/`Eq`, so regexes must **not** live inside `Term`/`Query`. Compile into a separate structure: add `src/search/compiled.rs` (declared in `search.rs`, per the `module.rs` + `module/submodule.rs` layout) exposing a `CompiledQuery` built from a `Query`, plus an accessor returning the flat set of string-term regexes for the renderer.

**Invalid / partial regex (common while typing, e.g. `fire(`)**

- Filter with only the successfully-compiled terms: a term whose regex fails to compile is treated as **non-constraining** (skipped within its AND-group) rather than failing the whole tree. Consequence: a query that is *only* a partial regex momentarily shows the full forest — acceptable, and better than a blank pane while typing.
- Surface a **persistent, low-key** "invalid regex" hint on the status-line right side (the `[error|hint]` slot) for as long as the current query has an uncompilable term. This is **not** the existing `flash` mechanism (`state.rs` `set_flash` / `flash_active`), which auto-clears after `ERROR_FLASH_DURATION` and is styled red-bold for `kill(2)` errors — that is the wrong fit for a per-keystroke typing state. Add a distinct bit of `App` state (set during compile in `refilter`) and render it **dim** (e.g. `Modifier::DIM`, muted fg), visually subordinate to the transient kill-error flash. Define precedence in `status_line.rs`: an active kill-error flash wins the slot; otherwise show the invalid-regex hint; otherwise the normal hint.

**Highlighting**

- In `tree_view::build_row`, after `wrap_argv` produces the per-line command strings (and for the `[<name>]` kernel-thread fallback and the differing-user tag), split each raw text span into amber / plain sub-spans wherever any active highlight regex matches. Add a `highlight_spans(text, &[Regex]) -> Vec<Span>` helper. Use the **union** of all string-term regexes across all OR-groups (simplest; may occasionally highlight, e.g., a `user:` pattern inside the command text — accepted).
- Matches that straddle a `wrap_argv` line break or are cut by the `…` ellipsis will highlight only partially — accepted, not worth mapping offsets through the wrap transform.
- Amber comes from a new `consts.rs` const, e.g. `SEARCH_MATCH_FG: Color = Color::Rgb(255, 176, 0)` — no magic literals in the renderer. Note this is a **second truecolor** beyond `FOCUS_ACCENT`, so relax the "one truecolor accent" line in the Visual rules section (`Color::Yellow` is already taken by CPU-warn and state `T`, so it can't be reused). Preserve correct layering with the selected-row `REVERSED` and kernel-thread `DIM` line styles.

**Performance**

- Compile regexes once per query change in `App::refilter`, never per row. Store the `CompiledQuery` (and the flat highlight regexes) on `App`; `matches`/`term_matches` take the compiled form. Filtering already iterates all processes per keystroke; regex `is_match` replaces the `to_lowercase().contains()` allocations in `contains_ci` for string terms. Row highlighting only runs over the visible-height rows.

**Acceptance criteria**

- `name:^fire`, `cmd:profile$`, `user:^root$`, and bare `fire.*fox` filter as regexes; case-insensitive by default; `pid:`/`ppid:`/`state:` semantics unchanged; comma/space OR/AND semantics unchanged (existing parser + filter tests still pass).
- A syntactically invalid regex never panics and never blanks the tree — it is skipped and the dim "invalid regex" hint appears; typing a partial regex keeps the UI responsive.
- Visible command text (plus the `[<name>]` fallback and the user tag) shows matched substrings in amber; the highlight coexists correctly with the reverse-video selected row and kernel-thread dimming.
- New unit tests: regex match/no-match, case-insensitivity default, invalid-regex skip behavior (and that the invalid-regex flag is set), and a `highlight_spans` test asserting span segmentation for no-match, single-match, and multi-match inputs.
- `cargo fmt`, `cargo clippy -D warnings`, and tests all green.

**Files touched**

- `Cargo.toml` — `cargo add regex`.
- `src/consts.rs` — add `SEARCH_MATCH_FG`.
- `src/search.rs` + new `src/search/compiled.rs` — `CompiledQuery` + highlight-regex accessor.
- `src/search/filter.rs` — `matches`/`term_matches` switch to compiled regexes; string-term `contains_ci` retired; invalid-term skip.
- `src/app/state.rs` — `App` stores the compiled query, highlight regexes, and the invalid-regex flag; `refilter` compiles.
- `src/ui/tree_view.rs` — `highlight_spans` + amber application in `build_row`.
- `src/ui/status_line.rs` — render the dim invalid-regex hint with correct precedence vs. the kill-error flash.
- Docs: `PLAN.md` Search DSL section and Visual rules (note the second truecolor); `README.md` ("substring search DSL" → regex); optionally the help modal (`src/ui/help_modal.rs`).
````

## Persist session state on quit and restore it on boot

Added best-effort persistence of the interactive session to a per-user JSON state file,
restored on the next launch so a restart resumes where the user left off.

- Deps via `cargo add`: `directories`, `serde` (derive), `serde_json`.
- `src/persist.rs` (new, serde boundary): `PersistedState` (primitive fields —
  `query_text`, `paused`, `hide_kernel_threads`, `cursor_pid`/`cursor_start_time`, and
  `focus` as a dedicated `PersistedFocus` enum), with `From` conversions so the domain
  `Focus`/`ProcessId` stay serde-free. `state_file_path()` uses
  `ProjectDirs::from("","","rtop")` → `state_dir()` (Linux `~/.local/state/rtop/`) else
  `data_dir()` (macOS `~/Library/Application Support/rtop/`), file `state.json`. `load()`
  returns default on any error (missing/corrupt/unreadable). `save()` writes a sibling
  `state.tmp` + `sync_all` + atomic `rename`. `resolve_boot()` is a pure precedence resolver
  (`--no-restore` → defaults; non-empty `--filter` overrides the query; `--no-kernel-threads`
  forces hide).
- `src/consts.rs`: `STATE_SAVE_INTERVAL` (3s, debounced) and `STATE_FILE_NAME`.
- `src/app/state.rs`: `App::from_boot(PersistedState)` (the production constructor; `App::new`
  is now `#[cfg(test)]`), `persisted_state()` (snapshots the persisted fields from live
  state), `should_accept_snapshot()` (`!paused || latest.is_none()` — the first snapshot is
  always accepted so a restored-paused session still populates), and a new
  `pending_cursor_id` field holding the restored cursor anchor until the first
  snapshot-backed tree build, where `ensure_tree_built` folds it into the existing
  re-anchor-by-`ProcessId` logic (falls back to first match / row 0 if the process is gone).
- `src/app.rs`: `run`/`run_loop` take the resolved `boot` state plus a `persist_enabled`
  flag; a `crossbeam_channel::tick(STATE_SAVE_INTERVAL)` arm plus a final flush on every exit
  path save via change-detection (`maybe_save` compares `persisted_state()` to the last
  written value rather than a hand-maintained dirty flag — strictly more robust, can't miss a
  mutation site). Snapshot arm now gated by `should_accept_snapshot()`.
- `src/main.rs`: `mod persist`; `--filter` is now `Option<String>`; new `--no-restore` flag;
  loads + resolves boot state before raw mode and passes it (and `!no_restore`) into `run`.

Deviations from the spec, all deliberate:
- **Change-detection instead of a dirty flag.** `maybe_save` diffs the current
  `persisted_state()` against the last-saved value, which satisfies "save only when changed"
  without sprinkling `dirty = true` across ~10 mutation sites (and never misses one).
- **`--no-restore` is fully ephemeral (skips load AND save).** Because the tree cursor
  auto-anchors on the first snapshot, a do-nothing session always diverges from its boot
  baseline; if `--no-restore` still saved, a throwaway run would overwrite the user's saved
  query with an empty one. Skipping the save honors "behave exactly as today" and avoids that
  footgun.
- **Explicit empty `--filter ""` keeps the restored query** (only a non-empty `--filter`
  overrides), per the spec's "a non-empty `--filter` wins" wording.

Tests: 21 new unit tests (persist round-trip; decode corrupt/empty/partial → default;
`load_from` missing → default; `save`→`load_from` round trip incl. rename-over-existing;
`resolve_boot` precedence matrix; `Focus`↔`PersistedFocus`; `from_boot` field restore;
pending-anchor placement alive→placed and gone→fallback; `should_accept_snapshot`;
`persisted_state` cursor capture). Verified end-to-end by driving the real TUI through a PTY:
query+focus+cursor persist across restart, do-nothing restore preserves state, `--no-restore`
does not touch the file, `--filter` overrides the restored query, a corrupt file is ignored
without aborting startup, and a restored-paused session still populates on the first snapshot.
`cargo fmt` + `cargo clippy --all-targets -D warnings` clean; `cargo test` green (125 tests).

Discovered follow-up (added to Next Up): the status line's left stats and right-aligned hint
render into the same rect, so the `mem:` stats get overwritten by the hint at ~120 cols.

### Original PLAN.md entry (verbatim, before work began)

### Persist session state on quit and restore it on boot

rtop starts fresh every run. Add best-effort persistence of the interactive session — the
search query plus view state — to a per-user state file, written periodically and on quit,
and restored on startup so a restart resumes where the user left off. This is runtime
**state**, distinct from a user-editable config file (still out of scope).

**What to persist (full session).** Only fields that are user-facing and meaningful across a
restart. `query_text` is the canonical search field — `query`/`compiled` are re-derived from
it by `App::new`/`refilter`, so store only the raw string.

- `query_text: String` (`src/app/state.rs:23`) — the search query.
- `focus: Focus` (`:22`) — Search vs Tree pane.
- `paused: bool` (`:29`).
- `hide_kernel_threads: bool` (`:52`).
- Tree cursor anchor: the `ProcessId` (`pid` + `start_time`) under the cursor
  (`tree_cursor_id`, `:46`), stored as primitives; best-effort re-anchor on first snapshot.

Do **not** persist derived/ephemeral fields: `latest`, `quit`, `query`, `compiled`,
`matched_pids`, `pending_g`, `tree_visible`/`tree_cursor`/`tree_offset`/`tree_cache_key`,
`help_open`, `signal_modal`, `flash`. The refresh `interval` is a CLI arg, not in `App`
state (`src/main.rs:24`, `src/sampler.rs`) — out of scope.

**State file.**

- Location via the `directories` crate (`cargo add directories`):
  `ProjectDirs::from("", "", "rtop")`; use `state_dir()` when present (Linux
  `~/.local/state/rtop/`), else fall back to `data_dir()` (macOS
  `~/Library/Application Support/rtop/`). File name `state.json`. Create the parent dir if
  needed. rtop targets both macOS and Linux (`src/source/macos.rs`, `src/source/linux.rs`).
- Format: JSON via `serde` + `serde_json` (`cargo add serde --features derive`,
  `cargo add serde_json`). Define a dedicated `PersistedState` struct of **primitive** fields
  (String, bools, `Option<i32>`/`Option<u64>` for the cursor pid/start_time, `focus` as a
  small `Serialize`/`Deserialize` enum or a bool). Do not add serde derives to the domain
  types `ProcessId` (`src/process.rs`) or `Focus` (`src/app/event.rs`); convert at the module
  boundary.
- New module `src/persist.rs` (`module.rs` layout, not `mod.rs`) exposing
  `load() -> PersistedState` (returns default on **any** error) and
  `save(&PersistedState)` (write to a temp file in the same dir, then rename → atomic).
  Both are best-effort: never panic, never abort startup or shutdown. Missing file →
  defaults; corrupt/unreadable → ignore (optionally a dim status flash) and continue. Do all
  path/IO **before or outside raw mode** so a state-file failure can never corrupt the
  terminal (mirror the "surface source errors before raw mode" discipline at
  `src/main.rs:40-44`).

**Boot precedence.**

- `--no-restore` (new bool flag on `Cli`, `src/main.rs:20-34`): skip loading entirely;
  behave exactly as today (query from `--filter` or empty, default toggles).
- Change `--filter` from `default_value = ""` to `Option<String>` so "absent" is
  distinguishable from an explicit value. A non-empty `--filter` wins for the query;
  otherwise the restored query applies.
- The existing `--no-kernel-threads` flag forces hide = true when passed; otherwise use the
  restored value.
- Wire the resolved values into `App::new` (`src/app/state.rs:91-115`) — extend it, or add a
  constructor that also accepts `focus`, `paused`, and the restored cursor anchor. Reconcile
  these in `main()`/`app::run` (`src/main.rs:47-48`, `src/app.rs:20-45`), keeping load/IO out
  of the TUI loop.

**Restore edge cases (must handle).**

- **Paused restore.** `run_loop` only updates `latest`/`refilter` when `!app.paused`
  (`src/app.rs:63-69`). If restored `paused == true`, the view would never populate. Ensure
  the **first** snapshot always populates `latest` (e.g. accept it when `latest.is_none()`
  regardless of pause), then honor pause thereafter.
- **Cursor anchor.** `ensure_tree_built` nulls `tree_cursor_id` on its no-snapshot early
  return (`src/app/state.rs:164-172`) and re-anchors by matching the full `ProcessId`
  (pid + start_time, robust vs PID reuse) at `:204-219`. The restored anchor must survive
  until the first snapshot-backed build — hold it in a separate "pending anchor" field
  applied on the first build (or guard the null-out), then let the existing re-anchor logic
  place the cursor; fall back to first-match / row 0 if the process is gone.

**Save timing (periodic + on quit).**

- Add a periodic tick to the `select!` in `run_loop` (`src/app.rs:57-72`) via
  `crossbeam_channel::tick(STATE_SAVE_INTERVAL)`; on tick, if persisted state changed since
  the last write, save. Track a dirty flag set wherever a persisted field mutates: query
  edits (`src/app.rs:123,128,132,176,185`), pause toggle (`:172`), focus changes
  (`:134,136,179,181`), cursor moves (`update_tree_cursor_id`, `g`/`G` at `:154-168`).
- On the quit/`break` path (`src/app.rs:53-55`, and the channel-disconnect breaks), do a
  final save.
- Add `STATE_SAVE_INTERVAL` (a few seconds; debounce) to `src/consts.rs` — no magic literals
  (per the discipline note at PLAN.md "Constants").
- Note: a panic bypasses this save (`install_panic_hook`, `src/main.rs:51-58`, never touches
  `App`); periodic saving makes best-effort loss acceptable. Concurrent rtop instances
  writing the same file: last-writer-wins is acceptable.

**Docs.** Update the "Out of v1 scope" line (PLAN.md:160) so "config file" reads as the
user-editable *config* remaining out of scope while runtime *state* persistence is now in
scope; add the state file and `--no-restore` to the CLI (PLAN.md:148-152) and Crates
(PLAN.md:154-156) reference sections.

**Done when.**

- A fresh run with no state file behaves exactly as today.
- After typing a query, moving the cursor, and toggling pause / hide-kernel-threads, quitting
  and relaunching restores: the query (search re-filters identically), pane focus, paused,
  hide-kernel-threads, and the cursor re-anchors to the same process if still alive (else
  first match / top).
- A non-empty `--filter` overrides the restored query; `--no-restore` starts fresh.
- A corrupt / missing / unreadable state file never aborts startup or shutdown, and a
  state-file error never leaves the terminal in raw mode.
- State is written atomically (temp + rename) both periodically and on quit.
- Unit tests cover: `persist` round-trip (serialize → deserialize equals original); `load`
  returns default on missing and on corrupt input; the boot-precedence resolver (`--filter`
  vs restored vs `--no-restore`, and `--no-kernel-threads` vs restored); "first snapshot
  populates even when restored paused"; and cursor re-anchor by `ProcessId` after restore
  (extend the existing `tree_cursor_preserved_if_pid_visible` pattern in
  `src/app/state.rs`).
- `chk` clean (fmt + clippy `-D warnings`); `cargo test` green. New deps added via
  `cargo add`.

## Status line: left stats and right hint overlap at narrow widths

Split the status-line row into two disjoint, width-constrained rects so the right-aligned
hint can no longer clobber the left-side load/mem stats:

- `src/ui/status_line.rs`: `render` now measures the left stats line via `Line::width()`,
  gives them a `left_area` of exactly that width (capped to the row), and hands the hint a
  `right_area` covering only the remaining columns. Because the two rects are disjoint, the
  hint truncates into — or vanishes from — its own lane instead of overwriting the `mem:`
  figure. Extracted the right-slot line construction (flash error → invalid-regex → focus
  hint) into a `build_right(app, now)` helper.
- 3 new `TestBackend`-backed render tests (128 total): the full `mem: 512MiB/1.0GiB` figure
  survives at 100 cols (a width where the old single-`area` render clobbered it — confirmed
  by temporarily reverting the fix and watching the test fail with `mem: 51type to filter…`),
  stats and hint coexist in disjoint columns when wide, and a 30-col row neither panics nor
  displaces the left half.
- `cargo clippy --all-targets` clean; `cargo nextest run` green.

Original PLAN.md entry (verbatim):

> ### Status line: left stats and right hint overlap at narrow widths
>
> The status line renders the left paragraph (`[focus]  N/M procs  [paused]   load: … mem: …`)
> and the right-aligned hint into the **same** `area` (`src/ui/status_line.rs:18-43`), drawing
> the hint second. When their combined width exceeds the terminal, the hint overwrites the tail
> of the left stats — around 120 cols the `mem:` figure disappears. Split the status-line rect
> into two width-constrained halves (or truncate/hide the hint when space is tight) so the
> load/mem stats are never clobbered. Discovered while verifying session persistence.

## Rich search-box editing via `tui-input`

Replaced the append/backspace-only search input with a full single-line editing model
backed by the `tui-input` crate (0.15.3, default `ratatui-crossterm` feature; verified a
single `crossterm 0.29.0` in the tree so the `Event` types unify).

- `src/app/state.rs`: `App.query_text: String` → `query_input: tui_input::Input`. Added
  `query_str()` accessor, `set_query()` (fresh `Input`, caret at end), and
  `query_kill_to_start()` (readline Ctrl-u: keep cursor→end, caret to 0). `refilter`, the
  `ensure_tree_built` cache key, and `persisted_state` all read through `query_str()`.
- `src/app.rs` `handle_search_key`: intercept the reserved keys (`Ctrl-n`/`Ctrl-p` tree
  nudge, `Tab`/`BackTab`/`Enter` focus, `Esc` clear) plus `Ctrl-u` (remapped to kill-to-start;
  the crate's native `Ctrl-u` is whole-line) and `Alt-b`/`Alt-f` word motion — the crate maps
  word motion under `META` but crossterm reports Alt as `ALT`, so those two issue
  `InputRequest::GoTo{Prev,Next}Word` directly. Everything else (printable incl. `?`, char
  motion, `Home`/`End`, `Backspace`/`Delete`, `Ctrl-w`, `Ctrl-k`, `Ctrl-y`, `Ctrl-Left/Right`)
  delegates to `Input::handle_event`; `refilter()` runs only when the value actually changed.
- `src/app.rs` `handle_key`: `F1` opens help from any context; `?` opens help only when focus
  is not the search box (search-focused `?` types a literal `?`); `F1`/`Esc`/`?` all close it.
- `src/ui/search_box.rs`: dropped the `chars().count()` scroll math for
  `input.visual_scroll(inner_w - 1)` + `input.visual_cursor()` (Unicode-width correct),
  reserving the final inner column for the caret so it never slides under the border. Prefix
  highlighting is unchanged (`highlight(app.query_str())`).
- `src/persist.rs`: added `query_cursor: Option<usize>` to `PersistedState` (`None` in older
  files → caret restored to end for backward compat; `with_cursor` clamps a stale value). A
  non-empty `--filter` resets the caret to `None` so it lands at the end of the CLI text.
- `src/ui/help_modal.rs` + `state::hint_for`: documented the editing keys, `?`-inserts-literally,
  and F1-for-help; bumped `HELP_MODAL_HEIGHT` 20 → 24 to fit the expanded search section.
- Tests (150 total, all green; clippy clean; `cargo fmt` applied): editing/nav via the real
  `handle_search_key`/`handle_key` (mid-string insert, Alt word motion, Ctrl-arrow word motion,
  Ctrl-w, Ctrl-k, readline Ctrl-u, reserved-key focus/nav, `?`-literal vs help routing, F1
  open/close); `query_cursor` restore/clamp/persistence round-trip and kill-to-start in
  `state.rs`; `highlight` spans + `TestBackend` cursor positioning (short + overflow) in
  `search_box.rs`; `query_cursor` serde + `--filter` caret reset in `persist.rs`; updated the
  status-line hint-tail assertion to the new `F1 help` text.

Original PLAN.md entry (verbatim):

> ### Rich search-box editing via `tui-input`
>
> Replace the append/backspace-only search input with a full single-line editing model backed by the `tui-input` crate. Today `App.query_text: String` (`app/state.rs`) is edited with `push`/`pop` only in `handle_search_key` (`app/event.rs`), the cursor is pinned to end-of-string, and horizontal scroll is hand-computed from `chars().count()` (scalar count, not display width, so wide/combining chars misalign). Users can only append and backspace — no mid-line cursoring, word motions, or line-kill.
>
> **Why `tui-input` (the out-of-the-box answer):** it is the standard single-line input model for ratatui, depends on exactly our `ratatui 0.30.0` + `crossterm 0.29.0`, and — crucially — renders nothing itself. It owns only the buffer + cursor, so the bold-cyan DSL-prefix highlighting in `ui/search_box.rs` is preserved verbatim: we keep calling `highlight(input.value())`. The self-rendering `tui-textarea`/`ratatui-textarea` widget draws its own text and would fight that styled-token rendering (and is multi-line overkill); rejected.
>
> **Steps**
>
> - `cargo add tui-input` (default features → `ratatui-crossterm`, matches our version pins).
> - `app/state.rs`: change `query_text: String` → `query_input: tui_input::Input`; add a `query_str(&self) -> &str` accessor returning `self.query_input.value()`. Update every reader: `refilter`, the `ensure_tree_built` cache key, and the tree-context assignments in `app/event.rs` (`/` clear, Esc clear, `pid:<X>` drill) — set the value by constructing a fresh `Input` so the caret lands at end.
> - `app/event.rs` `handle_search_key`: intercept the reserved search keys first — `Ctrl-n`, `Ctrl-p`, `Tab`, `BackTab`, `Esc`, `Enter` — so they keep their focus/nav semantics; then delegate everything else (including printable chars and all editing keys) to `input.handle_event(&Event::Key(k))` and call `refilter()` when it returns `Some(StateChanged)`.
> - **Ctrl-u = kill-to-start (readline-accurate), handled explicitly.** `tui-input` has no `DeleteTillStart` request (its default Ctrl-u = `DeleteLine`, whole line), so intercept Ctrl-u in `handle_search_key`: keep the substring from the cursor to end, reset the caret to 0 (e.g. rebuild the `Input`). Ctrl-k (kill-to-end) uses the crate's native `DeleteTillEnd`. Note: the crate's yank buffer won't capture a manually-killed prefix, so Ctrl-y after Ctrl-u is a known no-op — acceptable, mention in help if space allows.
> - **`?` now types literally in the search box; F1 becomes the help trigger.** In `handle_key` (`app/event.rs`), replace the global `?`→help mapping: F1 opens the help modal in any context; `?` opens help only when focus is **not** the search box. When the search box is focused, `?` falls through to `handle_search_key` and `tui-input` inserts it at the cursor. Update `ui/help_modal.rs` and any hint text (`state::hint_for`) accordingly.
> - `ui/search_box.rs`: render `highlight(app.query_str())` unchanged; replace the manual scroll with `input.visual_scroll(inner_w)` for the `Paragraph::scroll()` offset, and place the cursor at `inner.x + (input.visual_cursor() - scroll)` via `set_cursor_position` (only when focused). Delete the `chars().count()` scroll math and the "cursor = end of string" logic — `visual_scroll`/`visual_cursor` are Unicode-width correct.
> - **Persist the caret offset too.** Extend the persisted session state (`persist.rs` / `state::persisted_state` / `state::from_boot`) with a `query_cursor` char index alongside the query string; on boot, restore via `tui_input::Input::new(query_text).with_cursor(query_cursor)` clamped to the value length. Keep it backward-compatible with older state files (default the cursor to end when the field is absent). Prefer a plain `usize` field over enabling `tui-input`'s `serde` feature so the persisted schema stays explicit; confirm the exact `Input` constructor/`with_cursor` API during implementation.
> - Update `ui/help_modal.rs` (and optionally the Search hint) to document the new editing keys, `?`-inserts-literally, and F1-for-help.
> - No new `consts.rs` tunable is expected (scroll is derived from box width, editing lives in the crate). Add one only if a real magic number appears during implementation.
>
> **Out of scope:** bracketed-paste / multi-char paste into the search box (crossterm bracketed-paste events aren't enabled today).
>
> **New search-context editing bindings** (all `tui-input` defaults except Ctrl-u, which is remapped as above; none conflict with the existing Tab/Shift-Tab/Esc/Ctrl-n/Ctrl-p/Enter set): Left/Right & Ctrl-b/Ctrl-f (char), Alt-b/Alt-f & Ctrl-Left/Right (word), Home/Ctrl-a & End/Ctrl-e, Backspace & Delete, Ctrl-w (delete word back), Ctrl-k (kill to end), Ctrl-u (kill to start), Ctrl-y (yank last kill).
>
> **Acceptance**
>
> - Can insert and delete in the middle of the query; the cursor visibly moves and the box scrolls horizontally to keep it in view (Unicode-width correct).
> - Word motions (Alt-b/f, Ctrl-Left/Right), Home/End, Ctrl-w, Ctrl-k, and readline Ctrl-u (kill-to-start) all behave as specified.
> - Bold-cyan `pid:`/`user:`/`name:`/etc. prefix highlighting still renders while editing.
> - `?` types a literal `?` into the query while search-focused; F1 opens help from any context; `?` still opens help from the tree.
> - All existing search bindings (focus switch, clear, tree-cursor nudge, jump-to-match) still work; on restore, the query **and** caret offset come back.
> - Unit tests: mid-string insert, word-left/right, delete-word-back, kill-to-end, kill-to-start, that reserved keys still perform focus/nav actions, and that `query_cursor` round-trips through persistence.
>
> Commit semantically, e.g. `feat: rich cursor editing in search box via tui-input`.
