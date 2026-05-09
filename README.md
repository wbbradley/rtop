# rtop

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a substring search box driving a load-sorted list, and a context-sensitive process tree below.

## Status

Early development; see [PLAN.md](PLAN.md) for roadmap.

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
