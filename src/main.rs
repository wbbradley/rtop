use std::{io::stdout, time::Duration};

use anyhow::Context;
use clap::Parser;
use crossterm::ExecutableCommand;

mod app;
mod consts;
mod format;
mod persist;
mod process;
mod sampler;
mod search;
mod signal;
mod source;
mod tree;
mod ui;

use source::ProcessSource;

#[derive(Parser, Debug)]
#[command(name = "rtop", version, about = "TUI process monitor")]
struct Cli {
    /// Sample interval in seconds. Default mirrors `consts::SAMPLE_INTERVAL` (5.0s).
    #[arg(long, default_value_t = consts::SAMPLE_INTERVAL.as_secs_f64())]
    interval: f64,

    /// Pre-populate the search box with this expression. A non-empty value
    /// overrides any restored session query for this run.
    #[arg(long)]
    filter: Option<String>,

    /// Hide kernel threads from the load view.
    #[arg(long)]
    no_kernel_threads: bool,

    /// Do not restore the persisted session; start fresh (query from --filter
    /// or empty, default view toggles).
    #[arg(long)]
    no_restore: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let interval = Duration::from_secs_f64(cli.interval);

    // Load persisted session state and resolve boot precedence BEFORE raw mode,
    // so a state-file failure can never leave the terminal in a bad state.
    // `--no-restore` skips reading the file entirely.
    let restored = if cli.no_restore {
        persist::PersistedState::default()
    } else {
        persist::load()
    };
    let boot = persist::resolve_boot(restored, cli.no_restore, cli.filter, cli.no_kernel_threads);

    // Construct platform source FIRST — any /proc readability error surfaces here,
    // BEFORE raw mode.
    let source: Box<dyn ProcessSource> =
        Box::new(source::PlatformSource::new().context("failed to initialize process source")?);

    install_panic_hook();

    let rx = sampler::spawn(source, interval);
    app::run(rx, boot, !cli.no_restore)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = stdout().execute(crossterm::terminal::LeaveAlternateScreen);
        original(info);
    }));
}
