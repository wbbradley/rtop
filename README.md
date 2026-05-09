# rtop

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a substring search box driving a load-sorted list, and a context-sensitive process tree below.

## Status

Early development; see [PLAN.md](PLAN.md) for roadmap.

## Features

- **Vim-style three-pane TUI**: search box, load-sorted process list, context-sensitive process tree.
- **Substring search DSL**: `pid:`, `ppid:`, `user:`, `name:`, `cmd:`, `state:` prefixes; bare terms search across name+cmdline+user. Space-separated terms are AND-ed.
- **Sort modes**: CPU%, RSS, TIME+, AGE — cycle with `s`.
- **Process tree**: spine of ancestors + DFS subtree of the load-view selection. `Enter` drills into a PID; tree cursor navigates with `j`/`k`/`gg`/`G`/Ctrl-d/Ctrl-u.
- **Signal sending**: press `K` to open the signal modal (TERM/KILL/HUP/INT/USR1/USR2/STOP/CONT). Confirms PID 1 and self-signal.
- **Pause** sampling with `space`; resume with `space`.
- **CLI flags**: `--filter <expr>` pre-populates the search box; `--interval <secs>` overrides sample interval; `--no-kernel-threads` hides kernel threads.
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
