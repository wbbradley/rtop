# rtop

A TUI process monitor in the spirit of `top`/`htop`, with vim-style navigation, a fuzzy search box driving a load-sorted list, and a context-sensitive process tree below.

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

Linux now; macOS planned (Phase 6).

## License

MIT — see [LICENSE](LICENSE).
