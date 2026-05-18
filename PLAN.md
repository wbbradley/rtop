# rtop — Implementation Plan

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a substring search box driving a load-sorted list, and a context-sensitive process tree below.

## Architecture Reference

Self-contained context so a developer picking up any phase has what they need.

### Layout (top → bottom)

1. **Search box** (3 rows incl. border) — single-line, scrolls horizontally. Initial focus.
2. **Load view** (~7 rows: header + `LOAD_VIEW_VISIBLE_ROWS` data rows + border) — sorted-by-load list.
3. **Tree pane** (remainder) — spine + subtree of the load view's selected process.
4. **Status line** (1 row, no border): `[focus] [N/M procs] [sort: cpu] [paused?]   [load: x x x  mem: x/y GiB]   [error|hint]`.

### Data flow

Search box state is canonical. Load view filters/sorts off it. Tree shows spine + subtree of the load view's currently-selected process.

### Search DSL

- Prefixed terms (case-insensitive substring): `pid:`, `ppid:`, `user:`, `name:`, `cmd:`, `state:`.
- Bare terms: case-insensitive substring match against `name + " " + cmdline + " " + user`.
- Space-separated terms = AND. No OR/negation in v1.
- `pid:X` is special: exact equality, auto-scrolls + highlights the row in load view, does not filter the rest out. (All other prefixes filter normally.)

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
- `LOAD_VIEW_VISIBLE_ROWS: usize` (4)
- `SCROLLOFF: usize` (3)
- `MIN_COLS: u16` (80), `MIN_ROWS: u16` (24)
- `ERROR_FLASH_DURATION: Duration` (3s)
- `CPU_WARN_PCT: f32` (50.0), `CPU_DANGER_PCT: f32` (80.0)

### Key bindings (full)

| Context | Key | Action |
|---|---|---|
| any | `Ctrl-C` | quit |
| any | `?` | open help modal |
| any | `Tab` / `Shift-Tab` | cycle focus forward/backward (search → load → tree → search) |
| any non-search | `/` | jump focus to search and clear it |
| any non-search | `Esc` | return focus to search |
| search | `Esc` | clear search if non-empty; otherwise no-op |
| search | `Ctrl-n` / `Ctrl-p` | move load view selection without leaving search focus |
| search | `Enter` | select first match (move focus to load view) |
| load view | `j`/`k`/`gg`/`G` | navigate selection; scrolloff=`SCROLLOFF` |
| load view | `Ctrl-d`/`Ctrl-u` | half-page selection move |
| load view | `Enter` | drill: commit `pid:<X>` to search |
| load view | `s` | cycle sort: CPU → RSS → TIME+ → AGE → CPU |
| load view | `K` | open signal modal |
| load view | `space` | toggle sampling pause |
| tree | `j`/`k`/`gg`/`G` | navigate cursor (DFS order); scrolloff=`SCROLLOFF` |
| tree | `Ctrl-d`/`Ctrl-u` | half-page viewport scroll |
| tree | `Enter` | commit `pid:<X>` to search (drill) |
| signal modal | `j`/`k` | pick signal |
| signal modal | `Enter` | send signal |
| signal modal | `Esc` | cancel |

### Visual rules

- ANSI 16 only in v1, no truecolor, no theming.
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

Threads, renice, kill-tree, multi-select, `cwd:` filter, search OR/negation, manual h/l fold ops in tree, runtime pane resize, config file, theming/truecolor.

---

## Next Up

