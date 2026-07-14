# rtop — Implementation Plan

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a substring search box driving a load-sorted list, and a context-sensitive process tree below.

## Next Up

### Search DSL negation (fzf-style `!` exclusion terms)

Add per-term negation to the search DSL via a leading `!`, using **fzf-style,
fully commutative** semantics. The top level stays **OR of comma-groups**; each
comma-group is an **AND of terms**; a term is positive or negated. Reordering
comma-groups (OR) or terms within a group (AND) never changes the result — there
is no ordering, no last-rule-wins, no linear narrowing.

**Match rule.** A process matches a comma-group iff it matches **all** positive
terms **and none** of the negated terms in that group. It matches the query iff
it matches **any** comma-group. Empty query still matches everything.
Examples: `python !test` → "python but not test"; `python !test,ruby !spec` →
"(python AND not test) OR (ruby AND not spec)"; `!test` alone → everything not
matching `test`.

**Backward compatibility.** A query with no `!` is exactly today's OR-of-AND-
groups; the regression tests below must prove identical matched sets.

#### Parser (`src/search/parser.rs`)

- Add polarity to `Term` — e.g. `Prefixed { field, value, negated: bool }` and
  `Bare { value, negated: bool }` (or wrap the existing enum in a `negated`
  field). Update `push_term` (parser.rs:70-97) to detect a leading `!` on the
  comma-fragment **before** the typed-prefix `split_once(':')`, strip it, set
  `negated = true`, then parse the remainder exactly as today.
- `!user:root` negates the typed term; only a leading `!` on the fragment is the
  marker, so `name:!foo` keeps `!` as part of the regex value (searches regex
  `!foo`). Leading `!` is always the negation marker — searching for a literal
  leading `!` is unsupported in v1 (no `\!` escape).
- A negated `pid:` term must **not** set `auto_select_pid`: guard the existing
  first-valid-pid logic (parser.rs:77-79) on `!negated`.

#### Compile (`src/search/compiled.rs`)

- Carry `negated` onto `CompiledTerm` (a bool field is simplest).
- `highlight` collects regexes from **positive string terms only** — skip
  pushing negated terms into the highlight set in `compile_str` (compiled.rs).
- `has_invalid` / `empty` semantics unchanged; a negated invalid regex still sets
  `has_invalid` (drives the dim hint).

#### Filter (`src/search/filter.rs`)

- Rewrite `matches` (filter.rs:8-15) as: `cq.is_empty()` → `true`; else
  `groups.any(|g| g.iter().all(|t| term_ok(p, t)))`.
- `term_ok` must keep invalid/`Never` **non-constraining in both polarities** so
  a negated invalid term never culls:
  - `Invalid` → `true` regardless of `negated` (so `!fire(` never blanks the
    tree, matching today's inclusion behavior).
  - Otherwise compute the raw predicate `m` (today's `term_matches`, filter.rs:
    17-36); return `if negated { !m } else { m }`. `Never` yields `m = false`,
    so a positive `!ppid:x`-style term fails the group (as today) and a negated
    one is `true` (a never-matching term, negated, excludes nothing).

#### Renderer / status-line

- No change required: `tree_view.rs` highlighting consumes `highlight_regexes()`
  (now positive-only) and `status_line.rs` still keys the dim invalid-regex hint
  off `has_invalid()`.

#### app/state.rs

- No change required: `refilter` (state.rs:196-214) and `ensure_tree_built`'s
  `matched_arg` (state.rs:255-259) still key off `query.groups.is_empty()` and
  `matched_pids`; the visible-tree closure logic is untouched.

#### PLAN.md doc updates (part of this task)

- In the **Search DSL** reference section, replace "No negation in v1." with the
  fzf per-term negation rule described above.
- Remove "search negation" from the **Out of v1 scope** list.

#### Tests

- `parser.rs`: `!` sets `negated`; `!user:root` is a negated typed term;
  `name:!foo` is **not** negated (value `!foo`); `!pid:1` does not set
  `auto_select_pid`; commutativity (reordering terms/groups yields equal match
  sets).
- `filter.rs`: `python !test` = python AND not test; `!test` alone = everything
  not matching test; `!fire(` (invalid) culls nothing; `!ppid:x` culls nothing;
  **regression**: existing negation-free cases (`a,b,e`; `a b,c`;
  `name:firefox,name:vim`; AND-within-group; invalid-skipped-in-group) produce
  identical results.
- `compiled.rs`: negated terms excluded from `highlight_regexes()`; a negated
  invalid term still flags `has_invalid`.

#### Acceptance criteria

1. Any query with **no** `!` produces identical matched sets to the pre-change
   build (proven by the regression tests).
2. `!term` excludes processes matching `term`, and negation composes per-term
   inside a group (`python !test`).
3. Fully commutative: reordering comma-groups or terms within a group never
   changes results.
4. An incomplete negated regex (`!fire(`) never empties the tree; the dim
   invalid-regex hint shows.
5. `!`-negated substrings are not painted amber in tree rows.
6. `!pid:X` does not auto-select / move the tree cursor.
7. `cargo test` and `cargo clippy` clean; `cargo fmt` applied.
8. The Search DSL and Out-of-scope sections of PLAN.md are updated.

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
  persist.rs                — session-state load/save (serde boundary)
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
| any | `F1` | open help modal |
| tree | `?` | open help modal |
| tree | `Tab` / `Shift-Tab` / `Esc` | focus search |
| search | `Tab` / `Shift-Tab` | focus tree |
| search | `Esc` | clear query (stay in search) |
| search | `Ctrl-n` / `Ctrl-p` | move tree cursor without leaving search |
| search | `Enter` | focus tree, jump cursor to first match |
| search | `Left`/`Right`, `Ctrl-b`/`Ctrl-f` | move cursor by char |
| search | `Alt-b`/`Alt-f`, `Ctrl-Left`/`Ctrl-Right` | move cursor by word |
| search | `Home`/`Ctrl-a`, `End`/`Ctrl-e` | cursor to start / end |
| search | `Backspace` / `Delete` | delete char before / after cursor |
| search | `Ctrl-w` | delete word before cursor |
| search | `Ctrl-k` / `Ctrl-u` | kill to end / start of line |
| search | `Ctrl-y` | yank last kill |
| search | printable (incl. `?`) | insert at cursor |
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
- `--filter <expr>` pre-populates search box (a non-empty value overrides the
  restored session query for this run).
- `--no-kernel-threads` starts with kernel threads hidden.
- `--no-restore` does not restore the persisted session and does not save on
  exit (fully ephemeral run).
- `--version`, `--help`.

Session state (search query, focus, paused, hide-kernel-threads, and the tree
cursor anchor) is persisted to a per-user JSON state file
(`state.json` under the OS state/data dir) and restored on the next launch.

### Crates

`ratatui`, `crossterm`, `procfs` (Linux), `libproc` + `libc` (macOS, Phase 6), `crossbeam-channel`, `clap` (derive), `nix` for `kill(2)`, `directories` (state-file location), `serde` (derive) + `serde_json` (state serialization). All added via `cargo add`.

### Out of v1 scope

Threads, renice, kill-tree, multi-select, `cwd:` filter, search negation, manual h/l fold ops in tree, runtime pane resize, a user-editable *config* file (runtime *state* persistence is now in scope; see the CLI section), theming.
