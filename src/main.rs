use std::{io::stdout, time::Duration};

use anyhow::Context;
use clap::Parser;
use crossterm::ExecutableCommand;

mod app;
mod consts;
mod format;
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
    /// Sample interval in seconds. Default mirrors `consts::SAMPLE_INTERVAL` (1.0s).
    #[arg(long, default_value_t = 1.0)]
    interval: f64,

    /// Pre-populate the search box with this expression.
    #[arg(long, default_value = "")]
    filter: String,

    /// Hide kernel threads from the load view.
    #[arg(long)]
    no_kernel_threads: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let interval = Duration::from_secs_f64(cli.interval);

    // Construct platform source FIRST — any /proc readability error surfaces here,
    // BEFORE raw mode.
    let source: Box<dyn ProcessSource> =
        Box::new(source::PlatformSource::new().context("failed to initialize process source")?);

    install_panic_hook();

    let rx = sampler::spawn(source, interval);
    app::run(rx, cli.filter, cli.no_kernel_threads)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = stdout().execute(crossterm::terminal::LeaveAlternateScreen);
        original(info);
    }));
}
