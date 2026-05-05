use std::{io::stdout, time::Duration};

use anyhow::Context;
use clap::Parser;
use crossterm::ExecutableCommand;

mod app;
mod consts;
mod format;
mod process;
mod sampler;
mod source;
mod ui;

use source::ProcessSource;

#[derive(Parser, Debug)]
#[command(name = "rtop", version, about = "TUI process monitor")]
struct Cli {
    /// Sample interval in seconds.
    #[arg(long, default_value_t = 1.0)]
    interval: f64,
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
    app::run(rx)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = stdout().execute(crossterm::terminal::LeaveAlternateScreen);
        original(info);
    }));
}
