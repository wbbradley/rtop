# rtop

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a substring search box driving a load-sorted list, and a context-sensitive process tree below.

## Status

Early development; see [PLAN.md](PLAN.md) for roadmap.

## Features

- **Vim-style two-pane TUI**: search box on top, process tree below. The tree filters itself directly off the query — no intermediate sorted list.
- **Regex search DSL**: `pid:`, `ppid:`, `user:`, `name:`, `cmd:`, `state:` prefixes; bare terms search across name+cmdline+user. String-valued terms (`user:`/`name:`/`cmd:` and bare terms) are case-insensitive, unanchored regexes (`^`/`$` anchors, `(?-i)` for case sensitivity); `pid:`/`ppid:` are integer equality and `state:` is single-char. Space-separated terms within a group are AND-ed; a comma separates OR-groups. An invalid regex is skipped (never blanks the tree) with a dim "invalid regex" hint.
- **Match-driven tree**: every matching process is shown together with its full parent chain (root → match) and its complete subtree, with matched substrings highlighted in amber. Multiple disjoint matches become separate roots; empty query shows the full forest. Tree cursor navigates with `j`/`k`/`gg`/`G`/Ctrl-d/Ctrl-u; `Enter` drills into a PID.
- **Signal sending**: press `K` in the tree to open the signal modal (TERM/KILL/HUP/INT/USR1/USR2/STOP/CONT). Confirms PID 1 and self-signal.
- **Pause** sampling with `space`; resume with `space`.
- **Session persistence**: the search query, pane focus, paused state, hide-kernel-threads, and the tree cursor's process are saved to a per-user state file and restored on the next launch, so a restart resumes where you left off. `--no-restore` starts fresh (and does not save).
- **CLI flags**: `--filter <expr>` pre-populates the search box (overrides the restored query); `--interval <secs>` overrides sample interval; `--no-kernel-threads` hides kernel threads; `--no-restore` skips session restore.
- **Cross-platform**: Linux (`procfs`) + macOS (`libproc`/`sysctl`).

## Screenshot

_Screenshot coming soon._

## Build

```sh
cargo build --release
./target/release/rtop
```

## Install

```sh
cargo install --path .
```

## Platform support

- Linux (full support, via `procfs`).
- macOS (full support, via `libproc` + `sysctl`; signals via `nix::sys::signal::kill`. Some
  kernel-only stats from `procfs` are not available on macOS, but the visible UI is identical).

## License

MIT — see [LICENSE](LICENSE).
