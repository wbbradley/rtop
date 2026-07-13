use std::{sync::Arc, thread};

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender, bounded, select, tick};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use tui_input::{InputRequest, backend::crossterm::EventHandler};

use crate::{
    consts::{EVENT_CHANNEL_CAP, STATE_SAVE_INTERVAL, TREE_HALF_PAGE},
    persist::{self, PersistedState},
    process::Snapshot,
    ui,
};

pub mod event;
pub mod state;

use event::Focus;
use state::{App, SignalModal};

pub fn run(
    snapshot_rx: Receiver<Arc<Snapshot>>,
    boot: PersistedState,
    persist_enabled: bool,
) -> anyhow::Result<()> {
    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let result = run_loop(&mut terminal, snapshot_rx, boot, persist_enabled);
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    snapshot_rx: Receiver<Arc<Snapshot>>,
    boot: PersistedState,
    persist_enabled: bool,
) -> anyhow::Result<()> {
    let (event_tx, event_rx) = bounded::<Event>(EVENT_CHANNEL_CAP);
    spawn_event_thread(event_tx);

    let mut app = App::from_boot(boot);
    // Baseline for change-detected saves: what is currently on disk. Only writes
    // once the session diverges from it (e.g. the cursor anchors on the first
    // snapshot, or the user edits the query / toggles a view flag).
    let mut last_saved = Some(app.persisted_state());
    let save_tick = tick(STATE_SAVE_INTERVAL);

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
                    if app.should_accept_snapshot() {
                        app.latest = Some(s);
                        app.refilter();
                    }
                }
                Err(_) => break,
            },
            recv(save_tick) -> _ => maybe_save(&app, &mut last_saved, persist_enabled),
        }
    }

    // Final flush on any exit path (quit or a disconnected channel).
    maybe_save(&app, &mut last_saved, persist_enabled);
    Ok(())
}

/// Persist the session if it changed since the last write. A no-op when
/// persistence is disabled (`--no-restore` runs are fully ephemeral). Best-effort:
/// any IO error is ignored (it never touches the terminal, so raw mode stays
/// intact).
fn maybe_save(app: &App, last_saved: &mut Option<PersistedState>, persist_enabled: bool) {
    if !persist_enabled {
        return;
    }
    let current = app.persisted_state();
    if last_saved.as_ref() == Some(&current) {
        return;
    }
    let _ = persist::save(&current);
    *last_saved = Some(current);
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
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => app.help_open = false,
            _ => {}
        }
        return;
    }
    if app.signal_modal.is_some() {
        handle_signal_modal_key(app, k);
        return;
    }
    // F1 opens help from any context. `?` opens help only when not editing the
    // search box; while search-focused it falls through and types a literal `?`.
    if matches!(k.code, KeyCode::F(1)) {
        app.help_open = true;
        return;
    }
    if matches!(k.code, KeyCode::Char('?')) && app.focus != Focus::Search {
        app.help_open = true;
        return;
    }
    match app.focus {
        Focus::Search => handle_search_key(app, k),
        Focus::Tree => handle_tree_key(app, k),
    }
}

fn handle_search_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    // Reserved search keys keep their focus/nav semantics; intercept them before
    // handing the rest to the `tui-input` editing model.
    match k.code {
        KeyCode::Char('n') if ctrl => {
            move_tree_cursor(app, 1);
            return;
        }
        KeyCode::Char('p') if ctrl => {
            move_tree_cursor(app, -1);
            return;
        }
        // Alt-b/Alt-f word motion. crossterm reports Alt as `ALT`, but tui-input
        // only maps word motion under `META`, so issue the request directly.
        // (Ctrl-Left/Ctrl-Right word motion works through delegation.)
        KeyCode::Char('b') if alt => {
            app.query_input.handle(InputRequest::GoToPrevWord);
            return;
        }
        KeyCode::Char('f') if alt => {
            app.query_input.handle(InputRequest::GoToNextWord);
            return;
        }
        KeyCode::Tab | KeyCode::BackTab => {
            app.focus = Focus::Tree;
            return;
        }
        KeyCode::Esc => {
            app.set_query(String::new());
            app.refilter();
            return;
        }
        KeyCode::Enter => {
            app.focus = Focus::Tree;
            app.ensure_tree_built();
            app.jump_tree_cursor_to_first_match();
            return;
        }
        // Ctrl-u = readline kill-to-start (the crate's native Ctrl-u kills the
        // whole line, so it is remapped here).
        KeyCode::Char('u') if ctrl => {
            app.query_kill_to_start();
            app.refilter();
            return;
        }
        _ => {}
    }
    // Everything else — printable chars (including `?`) and all editing keys —
    // is handled by tui-input. Refilter only when the buffer value changed.
    if let Some(sc) = app.query_input.handle_event(&Event::Key(k))
        && sc.value
    {
        app.refilter();
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
        KeyCode::Char('d') if ctrl => move_tree_cursor(app, TREE_HALF_PAGE as isize),
        KeyCode::Char('u') if ctrl => move_tree_cursor(app, -(TREE_HALF_PAGE as isize)),
        KeyCode::Char(' ') => {
            app.paused = !app.paused;
        }
        KeyCode::Char('/') => {
            app.set_query(String::new());
            app.focus = Focus::Search;
            app.refilter();
        }
        KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
            app.focus = Focus::Search;
        }
        KeyCode::Enter => {
            if let Some(pid) = current_tree_cursor_pid(app) {
                app.set_query(format!("pid:{pid}"));
                app.refilter();
            }
        }
        _ => {}
    }
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
        KeyCode::Char('j') | KeyCode::Down if modal.cursor + 1 < n => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn press_mod(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            handle_search_key(app, press(KeyCode::Char(c)));
        }
    }

    #[test]
    fn search_mid_string_insert() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "abc"); // caret at end (3)
        handle_search_key(&mut app, press(KeyCode::Left)); // caret 2
        handle_search_key(&mut app, press(KeyCode::Left)); // caret 1
        handle_search_key(&mut app, press(KeyCode::Char('X'))); // insert at 1
        assert_eq!(app.query_str(), "aXbc");
        assert_eq!(app.query_input.cursor(), 2);
    }

    #[test]
    fn search_word_motions_alt_b_f() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "foo bar baz"); // caret 11
        handle_search_key(&mut app, press_mod(KeyCode::Char('b'), KeyModifiers::ALT));
        assert_eq!(app.query_input.cursor(), 8); // start of "baz"
        handle_search_key(&mut app, press_mod(KeyCode::Char('b'), KeyModifiers::ALT));
        assert_eq!(app.query_input.cursor(), 4); // start of "bar"
        handle_search_key(&mut app, press_mod(KeyCode::Char('f'), KeyModifiers::ALT));
        assert_eq!(app.query_input.cursor(), 8); // forward to "baz"
    }

    #[test]
    fn search_word_motions_ctrl_arrows() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "foo bar"); // caret 7
        handle_search_key(&mut app, press_mod(KeyCode::Left, KeyModifiers::CONTROL));
        assert_eq!(app.query_input.cursor(), 4); // start of "bar"
        handle_search_key(&mut app, press_mod(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(app.query_input.cursor(), 7); // end
    }

    #[test]
    fn search_delete_word_back_ctrl_w() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "foo bar"); // caret 7
        handle_search_key(
            &mut app,
            press_mod(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        assert_eq!(app.query_str(), "foo ");
    }

    #[test]
    fn search_kill_to_end_ctrl_k() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "hello world"); // caret 11
        handle_search_key(&mut app, press(KeyCode::Home)); // caret 0
        for _ in 0..5 {
            handle_search_key(&mut app, press(KeyCode::Right)); // caret 5 (after "hello")
        }
        handle_search_key(
            &mut app,
            press_mod(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert_eq!(app.query_str(), "hello");
    }

    #[test]
    fn search_kill_to_start_ctrl_u_readline() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "hello world");
        // Move caret to just before "world" (codepoint 6).
        handle_search_key(&mut app, press(KeyCode::Home));
        for _ in 0..6 {
            handle_search_key(&mut app, press(KeyCode::Right));
        }
        handle_search_key(
            &mut app,
            press_mod(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );
        assert_eq!(app.query_str(), "world");
        assert_eq!(app.query_input.cursor(), 0);
    }

    #[test]
    fn reserved_tab_focuses_tree() {
        let mut app = App::new(String::new(), false);
        assert_eq!(app.focus, Focus::Search);
        handle_search_key(&mut app, press(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Tree);
    }

    #[test]
    fn reserved_esc_clears_query() {
        let mut app = App::new(String::new(), false);
        type_str(&mut app, "abc");
        handle_search_key(&mut app, press(KeyCode::Esc));
        assert_eq!(app.query_str(), "");
        // Esc stays in the search box.
        assert_eq!(app.focus, Focus::Search);
    }

    #[test]
    fn reserved_enter_focuses_tree() {
        let mut app = App::new(String::new(), false);
        handle_search_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Tree);
    }

    #[test]
    fn reserved_ctrl_n_p_do_not_type_into_query() {
        let mut app = App::new(String::new(), false);
        handle_search_key(
            &mut app,
            press_mod(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        handle_search_key(
            &mut app,
            press_mod(KeyCode::Char('p'), KeyModifiers::CONTROL),
        );
        assert_eq!(app.query_str(), "");
    }

    #[test]
    fn question_mark_types_literally_when_search_focused() {
        let mut app = App::new(String::new(), false);
        // `?` usually arrives with SHIFT; tui-input inserts it regardless.
        handle_key(&mut app, press_mod(KeyCode::Char('?'), KeyModifiers::SHIFT));
        assert!(!app.help_open);
        assert_eq!(app.query_str(), "?");
    }

    #[test]
    fn question_mark_opens_help_from_tree() {
        let mut app = App::new(String::new(), false);
        app.focus = Focus::Tree;
        handle_key(&mut app, press(KeyCode::Char('?')));
        assert!(app.help_open);
    }

    #[test]
    fn f1_opens_help_from_search() {
        let mut app = App::new(String::new(), false);
        assert_eq!(app.focus, Focus::Search);
        handle_key(&mut app, press(KeyCode::F(1)));
        assert!(app.help_open);
    }

    #[test]
    fn f1_closes_open_help() {
        let mut app = App::new(String::new(), false);
        app.help_open = true;
        handle_key(&mut app, press(KeyCode::F(1)));
        assert!(!app.help_open);
    }
}
