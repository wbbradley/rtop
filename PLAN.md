# rtop — Implementation Plan

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a substring search box driving a load-sorted list, and a context-sensitive process tree below.

## Next Up

_No queued tasks._

## Architecture Reference

Self-contained context so a developer picking up any phase has what they need.

### Layout (top → bottom)

1. **Search box** (3 rows incl. border) — single-line, scrolls horizontally. Initial focus.
2. **Tree pane** (remainder) — visible forest computed from the search query.
3. **Status line** (1 row, no border): `[focus] [N/M procs] [paused?]   [load: x x x  mem: x/y GiB]   [error|hint]`.

### Data flow

Search box state is canonical. The tree pane shows the closure of the matched-PID set: every match plus its full parent chain (root → match) and complete descendant subtree. Empty query → full forest.

### Search DSL

- String-valued terms are **case-insensitive, unanchored regexes** (`regex` crate): the `user:` / `name:` / `cmd:` prefixes and bare terms. `^`/`$` anchor; an inline `(?-i)` opts back into case sensitivity. Bare terms match against `name + " " + cmdline + " " + user`.
- `pid:` / `ppid:`: exact integer equality. `state:`: single-char equality.
- Space-separated terms within an OR-group = AND. Comma separates OR-groups (adjacent to whitespace or token boundary; commas inside a token are literal). No negation in v1.
- `pid:X` filters by exact PID equality; the tree cursor auto-positions on the first matching node.
- An uncompilable regex (common mid-typing, e.g. `fire(`) is non-constraining — skipped within its AND-group — and a dim "invalid regex" hint shows in the status-line right slot.
- Matched substrings are painted amber (`SEARCH_MATCH_FG`) in the visible tree rows.

### Refresh & threading

- One sampler thread, one main/UI thread. No tokio. Channel: `crossbeam-channel` carrying `Arc<Snapshot>`.
- Sampler ticks every `SAMPLE_INTERVAL` (default 5s). UI renders on union of {keypress, new snapshot, terminal resize}.
- CPU% computed in sampler from `(prev_utime + prev_stime)` vs current; identity is `(pid, start_time)` to handle PID reuse.
- `space` toggles sampling pause.

### Process IR

```rust
struct ProcessId { pid: i32, start_time: u64 }  // identity

struct Process {
    id: ProcessId,
    ppid: i32,
    uid: u32,
    user: String,        // resolved at sample time
    name: String,        // /proc/<pid>/comm or basename of argv[0]
    cmdline: Vec<String>,// full argv; empty for kernel threads
    state: char,         // R/S/D/Z/T/I
    rss_bytes: u64,
    cpu_pct: Option<f32>,// None on first sample
    cpu_time_total: Duration, // for TIME+ display
    age: Duration,       // wall clock - start_time
    is_kernel_thread: bool, // hint for renderer (Linux: ppid==2 chain)
}

struct Snapshot {
    processes: Vec<Process>,
    by_id: HashMap<ProcessId, usize>, // for fast lookup
    sampled_at: Instant,
    system: SystemStats, // load avg, mem total/used
}
```

### Module layout

`module.rs + module/submodule.rs` (no `mod.rs`).

```
src/
  main.rs
  consts.rs                 — all global constants (SAMPLE_INTERVAL, MIN_COLS, etc.)
  app.rs                    — top-level app state + event loop
  app/state.rs
  app/event.rs              — Key/Snapshot/Resize event union
  process.rs                — Process, ProcessId, Snapshot
  source.rs                 — trait ProcessSource
  source/linux.rs           — procfs-backed source (Phase 1)
  source/macos.rs           — libproc-backed source (Phase 6)
  sampler.rs                — sampler thread driver
  search.rs
  search/parser.rs          — DSL → Query AST
  search/filter.rs          — Query + Snapshot → filtered indices
  format.rs                 — bytes (KiB/MiB/GiB), age (1d4h/4h12m/12m32s/32s), TIME+
  ui.rs                     — top-level draw routine
  ui/search_box.rs
  ui/load_view.rs
  ui/tree_view.rs
  ui/status_line.rs
  ui/help_modal.rs
  ui/signal_modal.rs
```

### Constants discipline

No magic numbers anywhere. All tunables live in `consts.rs`:

- `SAMPLE_INTERVAL: Duration` (5s)
- `SCROLLOFF: usize` (3)
- `TREE_HALF_PAGE: usize` (10)
- `MIN_COLS: u16` (80), `MIN_ROWS: u16` (24)
- `ERROR_FLASH_DURATION: Duration` (3s)
- `CPU_WARN_PCT: f32` (50.0), `CPU_DANGER_PCT: f32` (80.0)

### Key bindings (full)

| Context | Key | Action |
|---|---|---|
| any | `Ctrl-C` | quit |
| any | `?` | open help modal |
| tree | `Tab` / `Shift-Tab` / `Esc` | focus search |
| search | `Tab` / `Shift-Tab` | focus tree |
| search | `Esc` | clear query (stay in search) |
| search | `Ctrl-n` / `Ctrl-p` | move tree cursor without leaving search |
| search | `Enter` | focus tree, jump cursor to first match |
| tree | `j`/`k`/`gg`/`G` | navigate cursor (DFS order); scrolloff=`SCROLLOFF` |
| tree | `Ctrl-d`/`Ctrl-u` | half-page viewport scroll (`TREE_HALF_PAGE`) |
| tree | `Enter` | commit `pid:<X>` to search (drill) |
| tree | `K` | open signal modal |
| tree | `space` | toggle sampling pause |
| tree | `/` | clear query and focus search |
| signal modal | `j`/`k` | pick signal |
| signal modal | `Enter` | send signal |
| signal modal | `Esc` | cancel |

### Visual rules

- ANSI 16 for semantic colors (CPU, STATE, kernel threads, USER, errors). Two truecolor accents: `FOCUS_ACCENT` (RGB 254,128,25) for the focused-pane indicator and `SEARCH_MATCH_FG` (RGB 255,176,0, amber) for search-match highlighting in the tree. No theming.
- Selected row: reverse video.
- CPU%: yellow > `CPU_WARN_PCT`, red > `CPU_DANGER_PCT`, dim < 1.
- STATE colors: R=green, S=default, D=red, Z=red bold, T=yellow.
- Kernel threads: dim gray throughout.
- USER cyan when transitioning from parent's user (in tree pane).
- Search prefix tokens (`pid:`, etc.): bold cyan in the input box.
- Errors: red bold in status line right side, auto-clear after `ERROR_FLASH_DURATION`.

### Display formatting

- Bytes: `1.2GiB`, `345MiB`, `12KiB`. Binary units, exact terminology.
- Age: largest two units, e.g. `1d4h`, `4h12m`, `12m32s`, `32s`. Never milliseconds.
- TIME+: `1h23m` style, no centiseconds.
- STATE: single char.
- Unreadable cmdline → `[<comm>]` (kernel thread / ps aux convention).

### CLI flags (clap, derive)

- `--interval <secs>` overrides `SAMPLE_INTERVAL`.
- `--filter <expr>` pre-populates search box.
- `--no-kernel-threads` starts with kernel threads hidden.
- `--version`, `--help`.

### Crates

`ratatui`, `crossterm`, `procfs` (Linux), `libproc` + `libc` (macOS, Phase 6), `crossbeam-channel`, `clap` (derive), `nix` for `kill(2)`. All added via `cargo add`.

### Out of v1 scope

Threads, renice, kill-tree, multi-select, `cwd:` filter, search negation, manual h/l fold ops in tree, runtime pane resize, config file, theming.
