use std::{sync::Arc, thread};

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender, bounded, select};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{process::Snapshot, ui};

pub mod event;
pub mod state;

use event::Focus;
use state::{App, SignalModal};

pub fn run(snapshot_rx: Receiver<Arc<Snapshot>>, initial_filter: String) -> anyhow::Result<()> {
    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let result = run_loop(&mut terminal, snapshot_rx, initial_filter);
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    snapshot_rx: Receiver<Arc<Snapshot>>,
    initial_filter: String,
) -> anyhow::Result<()> {
    let (event_tx, event_rx) = bounded::<Event>(64);
    spawn_event_thread(event_tx);

    let mut app = App::new(initial_filter);

    loop {
        app.ensure_tree_built();
        terminal
            .draw(|f| ui::draw(f, &app))
            .context("failed to draw frame")?;

        if app.quit {
            break;
        }

        select! {
            recv(event_rx) -> ev => match ev {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => handle_key(&mut app, k),
                Ok(_) => {}
                Err(_) => break,
            },
            recv(snapshot_rx) -> snap => match snap {
                Ok(s) => {
                    if !app.paused {
                        app.latest = Some(s);
                        app.refilter();
                    }
                }
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

fn handle_key(app: &mut App, k: KeyEvent) {
    if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
        app.quit = true;
        return;
    }
    if app.help_open {
        match k.code {
            KeyCode::Esc | KeyCode::Char('?') => app.help_open = false,
            _ => {}
        }
        return;
    }
    if app.signal_modal.is_some() {
        handle_signal_modal_key(app, k);
        return;
    }
    if matches!(k.code, KeyCode::Char('?')) {
        app.help_open = true;
        return;
    }
    match app.focus {
        Focus::Search => handle_search_key(app, k),
        Focus::Load => handle_load_key(app, k),
        Focus::Tree => handle_tree_key(app, k),
    }
}

fn handle_search_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Char('n') if ctrl => move_cursor(app, 1),
        KeyCode::Char('p') if ctrl => move_cursor(app, -1),
        KeyCode::Char('K') if !ctrl => open_signal_modal(app),
        KeyCode::Char(c) if !ctrl => {
            app.query_text.push(c);
            app.refilter();
        }
        KeyCode::Backspace => {
            app.query_text.pop();
            app.refilter();
        }
        KeyCode::Esc if !app.query_text.is_empty() => {
            app.query_text.clear();
            app.refilter();
        }
        KeyCode::Tab => app.focus = Focus::Load,
        KeyCode::BackTab => app.focus = Focus::Tree,
        KeyCode::Enter => {
            app.focus = Focus::Load;
            app.load_cursor = 0;
            app.adjust_offset_for_scrolloff(usize::MAX);
        }
        _ => {}
    }
}

fn handle_load_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);

    let was_pending_g = app.pending_g;
    // Reset by default; specific arms set it back to true if appropriate.
    app.pending_g = false;

    match k.code {
        KeyCode::Char('j') => move_cursor(app, 1),
        KeyCode::Char('k') => move_cursor(app, -1),
        KeyCode::Char('K') => open_signal_modal(app),
        KeyCode::Char('g') => {
            if was_pending_g {
                app.load_cursor = 0;
                app.adjust_offset_for_scrolloff(usize::MAX);
            } else {
                app.pending_g = true;
            }
        }
        KeyCode::Char('G') => {
            let len = app.filtered_indices.len();
            app.load_cursor = len.saturating_sub(1);
            app.adjust_offset_for_scrolloff(usize::MAX);
        }
        KeyCode::Char('d') if ctrl => {
            let half = half_page(app);
            move_cursor(app, half as isize);
        }
        KeyCode::Char('u') if ctrl => {
            let half = half_page(app);
            move_cursor(app, -(half as isize));
        }
        KeyCode::Char('s') => {
            app.sort = app.sort.next();
            app.refilter();
        }
        KeyCode::Char(' ') => {
            app.paused = !app.paused;
        }
        KeyCode::Char('/') => {
            app.query_text.clear();
            app.focus = Focus::Search;
            app.refilter();
        }
        KeyCode::Esc => {
            app.focus = Focus::Search;
        }
        KeyCode::Tab => {
            app.focus = Focus::Tree;
        }
        KeyCode::BackTab => {
            app.focus = Focus::Search;
        }
        KeyCode::Enter => {
            if let Some(pid) = current_selected_pid(app) {
                app.query_text = format!("pid:{pid}");
                app.refilter();
            }
        }
        _ => {}
    }
}

fn handle_tree_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);

    let was_pending_g = app.pending_g;
    app.pending_g = false;

    match k.code {
        KeyCode::Char('j') => move_tree_cursor(app, 1),
        KeyCode::Char('k') => move_tree_cursor(app, -1),
        KeyCode::Char('K') => open_signal_modal(app),
        KeyCode::Char('g') => {
            if was_pending_g {
                app.tree_cursor = 0;
                update_tree_cursor_id(app);
                app.adjust_tree_offset_for_scrolloff(usize::MAX);
            } else {
                app.pending_g = true;
            }
        }
        KeyCode::Char('G') => {
            let len = app.tree_visible.len();
            app.tree_cursor = len.saturating_sub(1);
            update_tree_cursor_id(app);
            app.adjust_tree_offset_for_scrolloff(usize::MAX);
        }
        KeyCode::Char('d') if ctrl => {
            let half = tree_half_page();
            move_tree_cursor(app, half as isize);
        }
        KeyCode::Char('u') if ctrl => {
            let half = tree_half_page();
            move_tree_cursor(app, -(half as isize));
        }
        KeyCode::Char('/') => {
            app.query_text.clear();
            app.focus = Focus::Search;
            app.refilter();
        }
        KeyCode::Esc => {
            app.focus = Focus::Search;
        }
        KeyCode::Tab => {
            app.focus = Focus::Search;
        }
        KeyCode::BackTab => {
            app.focus = Focus::Load;
        }
        KeyCode::Enter => {
            if let Some(pid) = current_tree_cursor_pid(app) {
                app.query_text = format!("pid:{pid}");
                app.refilter();
            }
        }
        _ => {}
    }
}

fn move_cursor(app: &mut App, delta: isize) {
    if app.filtered_indices.is_empty() {
        return;
    }
    let len = app.filtered_indices.len() as isize;
    let cur = app.load_cursor as isize;
    let new = (cur + delta).clamp(0, len - 1);
    app.load_cursor = new as usize;
    app.adjust_offset_for_scrolloff(usize::MAX);
}

fn half_page(_app: &App) -> usize {
    use crate::consts::LOAD_VIEW_VISIBLE_ROWS;
    (LOAD_VIEW_VISIBLE_ROWS / 2).max(1)
}

fn current_selected_pid(app: &App) -> Option<i32> {
    let snap = app.latest.as_deref()?;
    let &idx = app.filtered_indices.get(app.load_cursor)?;
    snap.processes.get(idx).map(|p| p.id.pid)
}

fn move_tree_cursor(app: &mut App, delta: isize) {
    if app.tree_visible.is_empty() {
        return;
    }
    let len = app.tree_visible.len() as isize;
    let cur = app.tree_cursor as isize;
    let new = (cur + delta).clamp(0, len - 1);
    app.tree_cursor = new as usize;
    update_tree_cursor_id(app);
    app.adjust_tree_offset_for_scrolloff(usize::MAX);
}

fn update_tree_cursor_id(app: &mut App) {
    let snap = match app.latest.as_deref() {
        Some(s) => s,
        None => {
            app.tree_cursor_id = None;
            return;
        }
    };
    app.tree_cursor_id = app
        .tree_visible
        .get(app.tree_cursor)
        .map(|n| snap.processes[n.proc_idx].id);
}

fn tree_half_page() -> usize {
    use crate::consts::LOAD_VIEW_VISIBLE_ROWS;
    (LOAD_VIEW_VISIBLE_ROWS / 2).max(1)
}

fn current_tree_cursor_pid(app: &App) -> Option<i32> {
    let snap = app.latest.as_deref()?;
    let n = app.tree_visible.get(app.tree_cursor)?;
    Some(snap.processes[n.proc_idx].id.pid)
}

fn open_signal_modal(app: &mut App) {
    if let Some((pid, label)) = state::signal_target(app) {
        app.signal_modal = Some(SignalModal {
            target_pid: pid,
            target_label: label,
            cursor: 0,
            awaiting_confirm: false,
        });
    } else {
        app.set_flash("no process selected".to_string());
    }
}

fn handle_signal_modal_key(app: &mut App, k: KeyEvent) {
    let Some(modal) = app.signal_modal.as_mut() else {
        return;
    };
    let n = crate::signal::SIGNAL_CHOICES.len();
    if modal.awaiting_confirm {
        match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let pid = modal.target_pid;
                let sig = crate::signal::SIGNAL_CHOICES[modal.cursor].signal;
                let label = crate::signal::SIGNAL_CHOICES[modal.cursor].label;
                app.signal_modal = None;
                send_signal(app, pid, sig, label);
            }
            KeyCode::Esc => app.signal_modal = None,
            _ => modal.awaiting_confirm = false,
        }
        return;
    }
    match k.code {
        KeyCode::Char('j') | KeyCode::Down
            if modal.cursor + 1 < n => {
                modal.cursor += 1;
            }
        KeyCode::Char('k') | KeyCode::Up => {
            modal.cursor = modal.cursor.saturating_sub(1);
        }
        KeyCode::Esc => app.signal_modal = None,
        KeyCode::Enter => {
            let pid = modal.target_pid;
            let self_pid = std::process::id() as i32;
            if state::needs_confirm(pid, self_pid) {
                modal.awaiting_confirm = true;
            } else {
                let sig = crate::signal::SIGNAL_CHOICES[modal.cursor].signal;
                let label = crate::signal::SIGNAL_CHOICES[modal.cursor].label;
                app.signal_modal = None;
                send_signal(app, pid, sig, label);
            }
        }
        _ => {}
    }
}

fn send_signal(app: &mut App, pid: i32, sig: nix::sys::signal::Signal, label: &str) {
    use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
    match kill(Pid::from_raw(pid), Some(sig)) {
        Ok(()) => {}
        Err(Errno::EPERM) => app.set_flash(format!("EPERM: signal SIG{label} to PID {pid} denied")),
        Err(Errno::ESRCH) => app.set_flash(format!("ESRCH: PID {pid} no longer exists")),
        Err(e) => app.set_flash(format!("kill PID {pid} SIG{label}: {e}")),
    }
}
