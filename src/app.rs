use std::{sync::Arc, thread};

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender, bounded, select};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{process::Snapshot, ui};

pub mod state;

use state::App;

pub fn run(snapshot_rx: Receiver<Arc<Snapshot>>) -> anyhow::Result<()> {
    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let result = run_loop(&mut terminal, snapshot_rx);
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    snapshot_rx: Receiver<Arc<Snapshot>>,
) -> anyhow::Result<()> {
    let (event_tx, event_rx) = bounded::<Event>(64);
    spawn_event_thread(event_tx);

    let mut app = App::default();

    loop {
        terminal
            .draw(|f| ui::draw(f, &app))
            .context("failed to draw frame")?;

        if app.quit {
            break;
        }

        select! {
            recv(event_rx) -> ev => match ev {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                    if k.modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(k.code, KeyCode::Char('c'))
                    {
                        app.quit = true;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
            recv(snapshot_rx) -> snap => match snap {
                Ok(s) => app.latest = Some(s),
                Err(_) => break,
            },
        }
    }

    Ok(())
}

fn spawn_event_thread(tx: Sender<Event>) {
    thread::Builder::new()
        .name("rtop-events".to_string())
        .spawn(move || {
            while let Ok(ev) = crossterm::event::read() {
                if tx.send(ev).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn event thread");
}
