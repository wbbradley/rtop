use std::{collections::HashSet, sync::Arc, time::Instant};

use tui_input::Input;

use crate::{
    app::event::Focus,
    consts::{ERROR_FLASH_DURATION, SCROLLOFF},
    persist::PersistedState,
    process::{ProcessId, Snapshot},
    search::{CompiledQuery, Query, matches, parse},
    tree::{TreeNode, build_filtered, build_parent_to_children, build_pid_to_idx},
};

pub struct SignalModal {
    pub target_pid: i32,
    pub target_label: String,
    pub cursor: usize,
    pub awaiting_confirm: bool,
}

pub struct App {
    pub latest: Option<Arc<Snapshot>>,
    pub quit: bool,

    pub focus: Focus,
    /// Single-line search buffer with a movable caret (Unicode-width aware).
    /// The query string is `query_input.value()`; `query_str()` is the accessor.
    pub query_input: Input,
    pub query: Query,
    /// Compiled parallel of `query` (regexes for string terms). Derived state,
    /// recomputed alongside `query` in `refilter`; `query` stays canonical for
    /// the structural `groups.is_empty()` / `auto_select_pid` checks.
    pub compiled: CompiledQuery,
    pub paused: bool,

    /// PIDs of processes that satisfy the current query (pre-closure). Used by
    /// the status line as the "matched" count and by the tree builder as the
    /// closure seed.
    pub matched_pids: HashSet<i32>,

    // Two-key chord tracking for `gg`. Reset on any key other than another `g`.
    pub pending_g: bool,

    pub tree_visible: Vec<TreeNode>,
    pub tree_cursor: usize,
    pub tree_offset: usize,
    /// (Arc<Snapshot> ptr as usize, query string). When this matches, no rebuild.
    pub tree_cache_key: Option<(usize, String)>,
    /// ProcessId currently under the tree cursor — used to re-anchor across
    /// rebuilds when the same PID is still visible.
    pub tree_cursor_id: Option<ProcessId>,
    /// A restored cursor anchor awaiting the first snapshot-backed tree build.
    /// Consumed (taken) on that build, then the live `tree_cursor_id` takes over.
    pub pending_cursor_id: Option<ProcessId>,

    pub help_open: bool,
    pub signal_modal: Option<SignalModal>,
    pub flash: Option<(String, Instant)>,

    pub hide_kernel_threads: bool,
}

pub fn hint_for(focus: Focus) -> &'static str {
    match focus {
        Focus::Search => "type to filter | Esc reset | Enter→tree | F1 help",
        Focus::Tree => "j/k | Enter drill | K signal | space pause | Tab→search | ?→help",
    }
}

/// Returns true when sending a signal to this pid demands confirmation:
/// pid == 1 (init) or pid == self_pid (suicide).
pub fn needs_confirm(pid: i32, self_pid: i32) -> bool {
    pid == 1 || pid == self_pid
}

/// Target: the process under the tree cursor (regardless of focus). Returns
/// (pid, label) where label is "PID <pid>  <cmdline-or-comm>".
pub fn signal_target(app: &App) -> Option<(i32, String)> {
    let snap = app.latest.as_deref()?;
    let proc_idx = app.tree_visible.get(app.tree_cursor)?.proc_idx;
    let p = snap.processes.get(proc_idx)?;
    let cmd = if p.cmdline.is_empty() {
        format!("[{}]", p.name)
    } else {
        p.cmdline.join(" ")
    };
    Some((p.id.pid, format!("PID {}  {}", p.id.pid, cmd)))
}

pub fn flash_active(flash: &Option<(String, Instant)>, now: Instant) -> Option<&str> {
    let (msg, t) = flash.as_ref()?;
    if now.duration_since(*t) < ERROR_FLASH_DURATION {
        Some(msg.as_str())
    } else {
        None
    }
}

impl App {
    /// Test-only convenience constructor. Production code builds via
    /// [`App::from_boot`] with a resolved [`PersistedState`].
    #[cfg(test)]
    pub fn new(initial_filter: String, hide_kernel_threads: bool) -> Self {
        Self::from_boot(PersistedState {
            query_text: initial_filter,
            hide_kernel_threads,
            ..PersistedState::default()
        })
    }

    /// Build the app from a resolved boot/session state. Ephemeral and derived
    /// fields start empty; the restored cursor anchor is held in
    /// `pending_cursor_id` until the first snapshot-backed tree build.
    pub fn from_boot(boot: PersistedState) -> Self {
        let query = parse(&boot.query_text);
        let compiled = CompiledQuery::compile(&query);
        let pending_cursor_id = boot.cursor_id();
        // Restore the caret when the state file recorded one; a `None`
        // (older file) leaves the caret at end of the restored query.
        let query_cursor = boot.query_cursor;
        let query_input = {
            let input = Input::new(boot.query_text);
            match query_cursor {
                Some(c) => input.with_cursor(c),
                None => input,
            }
        };
        Self {
            latest: None,
            quit: false,
            focus: boot.focus.into(),
            query_input,
            query,
            compiled,
            paused: boot.paused,
            matched_pids: HashSet::new(),
            pending_g: false,
            tree_visible: Vec::new(),
            tree_cursor: 0,
            tree_offset: 0,
            tree_cache_key: None,
            tree_cursor_id: None,
            pending_cursor_id,
            help_open: false,
            signal_modal: None,
            flash: None,
            hide_kernel_threads: boot.hide_kernel_threads,
        }
    }

    /// The current search-query text.
    pub fn query_str(&self) -> &str {
        self.query_input.value()
    }

    /// Replace the search buffer, placing the caret at the end of `value`.
    pub fn set_query(&mut self, value: String) {
        self.query_input = Input::new(value);
    }

    /// Readline `Ctrl-u`: delete from the start of the line up to the caret,
    /// leaving the caret at column 0. `tui-input` has no such request (its
    /// native `Ctrl-u` kills the whole line), so it is rebuilt by hand.
    pub fn query_kill_to_start(&mut self) {
        let cursor = self.query_input.cursor();
        let tail: String = self.query_input.value().chars().skip(cursor).collect();
        self.query_input = Input::new(tail).with_cursor(0);
    }

    /// Snapshot the user-facing session fields for persistence. Reads the live
    /// cursor anchor (`tree_cursor_id`) so a moved cursor is captured.
    pub fn persisted_state(&self) -> PersistedState {
        PersistedState {
            query_text: self.query_str().to_string(),
            query_cursor: Some(self.query_input.cursor()),
            focus: self.focus.into(),
            paused: self.paused,
            hide_kernel_threads: self.hide_kernel_threads,
            cursor_pid: self.tree_cursor_id.map(|id| id.pid),
            cursor_start_time: self.tree_cursor_id.map(|id| id.start_time),
        }
    }

    /// Whether an incoming snapshot should populate the view. Normally gated by
    /// pause, but the very first snapshot is always accepted so a session
    /// restored as paused still has something to display.
    pub fn should_accept_snapshot(&self) -> bool {
        !self.paused || self.latest.is_none()
    }

    pub fn set_flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), Instant::now()));
    }

    /// Recompute the matched-PID set from the current query text.
    pub fn refilter(&mut self) {
        self.query = parse(self.query_str());
        self.compiled = CompiledQuery::compile(&self.query);
        self.matched_pids.clear();
        let Some(snap) = self.latest.as_deref() else {
            return;
        };
        if self.query.groups.is_empty() {
            return;
        }
        for p in &snap.processes {
            if self.hide_kernel_threads && p.is_kernel_thread {
                continue;
            }
            if matches(&self.compiled, p) {
                self.matched_pids.insert(p.id.pid);
            }
        }
    }

    pub fn adjust_tree_offset_for_scrolloff(&mut self, visible_rows: usize) {
        if self.tree_visible.is_empty() {
            self.tree_offset = 0;
            return;
        }
        let so = SCROLLOFF;
        if self.tree_cursor < self.tree_offset + so {
            self.tree_offset = self.tree_cursor.saturating_sub(so);
        }
        if visible_rows != usize::MAX && self.tree_cursor + so + 1 > self.tree_offset + visible_rows
        {
            self.tree_offset = (self.tree_cursor + so + 1).saturating_sub(visible_rows);
        }
        let max_offset = self
            .tree_visible
            .len()
            .saturating_sub(visible_rows.min(self.tree_visible.len()));
        if self.tree_offset > max_offset {
            self.tree_offset = max_offset;
        }
    }

    pub fn ensure_tree_built(&mut self) {
        let Some(snap) = self.latest.clone() else {
            self.tree_visible.clear();
            self.tree_cursor = 0;
            self.tree_offset = 0;
            self.tree_cache_key = None;
            self.tree_cursor_id = None;
            return;
        };

        let key = (Arc::as_ptr(&snap) as usize, self.query_str().to_string());
        if self.tree_cache_key.as_ref() == Some(&key) {
            return;
        }

        let pid_to_idx = build_pid_to_idx(&snap);
        let parent_to_children = build_parent_to_children(&snap);
        let matched_arg = if self.query.groups.is_empty() {
            None
        } else {
            Some(&self.matched_pids)
        };
        self.tree_visible = build_filtered(
            &snap,
            &parent_to_children,
            &pid_to_idx,
            matched_arg,
            self.hide_kernel_threads,
        );

        if self.tree_visible.is_empty() {
            self.tree_cursor = 0;
            self.tree_offset = 0;
            self.tree_cursor_id = None;
            self.tree_cache_key = Some(key);
            return;
        }

        // Preserve cursor by ProcessId when possible; otherwise jump to the
        // first matched node (or row 0 if none). On the first snapshot-backed
        // build after a restore, the pending restored anchor stands in for the
        // (not-yet-set) live cursor id.
        let anchor = self.tree_cursor_id.or(self.pending_cursor_id.take());
        let new_cursor = if let Some(prev_id) = anchor
            && let Some(pos) = self
                .tree_visible
                .iter()
                .position(|n| snap.processes[n.proc_idx].id == prev_id)
        {
            pos
        } else {
            self.tree_visible
                .iter()
                .position(|n| {
                    self.matched_pids
                        .contains(&snap.processes[n.proc_idx].id.pid)
                })
                .unwrap_or(0)
        };

        self.tree_cursor = new_cursor.min(self.tree_visible.len().saturating_sub(1));
        self.tree_cursor_id = Some(snap.processes[self.tree_visible[self.tree_cursor].proc_idx].id);
        self.tree_cache_key = Some(key);
        self.adjust_tree_offset_for_scrolloff(usize::MAX);
    }

    /// Move the tree cursor to the first matched node (DFS order). If there is
    /// no matched node visible, leave the cursor where it is.
    pub fn jump_tree_cursor_to_first_match(&mut self) {
        let Some(snap) = self.latest.as_deref() else {
            return;
        };
        if self.tree_visible.is_empty() {
            self.tree_cursor = 0;
            self.tree_cursor_id = None;
            return;
        }
        if let Some(pos) = self.tree_visible.iter().position(|n| {
            self.matched_pids
                .contains(&snap.processes[n.proc_idx].id.pid)
        }) {
            self.tree_cursor = pos;
            self.tree_cursor_id = Some(snap.processes[self.tree_visible[pos].proc_idx].id);
            self.adjust_tree_offset_for_scrolloff(usize::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::Arc,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::{
        consts::ERROR_FLASH_DURATION,
        process::{Process, ProcessId, Snapshot, SystemStats},
    };

    fn mk_proc(pid: i32, ppid: i32) -> Process {
        Process {
            id: ProcessId {
                pid,
                start_time: pid as u64,
            },
            ppid,
            uid: 0,
            user: "u".into(),
            name: format!("p{pid}"),
            cmdline: vec![format!("p{pid}")],
            state: 'S',
            rss_bytes: 0,
            cpu_pct: Some(0.0),
            cpu_time_total: Duration::ZERO,
            age: Duration::ZERO,
            is_kernel_thread: false,
        }
    }

    fn snap() -> Arc<Snapshot> {
        // 1 → 2, 1 → 3, 3 → 4
        let procs = vec![mk_proc(1, 0), mk_proc(2, 1), mk_proc(3, 1), mk_proc(4, 3)];
        let mut by_id = HashMap::new();
        for (i, p) in procs.iter().enumerate() {
            by_id.insert(p.id, i);
        }
        Arc::new(Snapshot {
            processes: procs,
            by_id,
            sampled_at: Instant::now(),
            system: SystemStats {
                load_1: 0.0,
                load_5: 0.0,
                load_15: 0.0,
                mem_total_bytes: 0,
                mem_used_bytes: 0,
            },
        })
    }

    fn cursor_pid(app: &App) -> i32 {
        let s = app.latest.as_deref().unwrap();
        s.processes[app.tree_visible[app.tree_cursor].proc_idx]
            .id
            .pid
    }

    #[test]
    fn tree_cursor_preserved_if_pid_visible() {
        let s = snap();
        let mut app = App::new(String::new(), false);
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        // Move cursor to pid 4.
        let pos4 = app
            .tree_visible
            .iter()
            .position(|n| s.processes[n.proc_idx].id.pid == 4)
            .unwrap();
        app.tree_cursor = pos4;
        app.tree_cursor_id = Some(s.processes[app.tree_visible[pos4].proc_idx].id);

        // New snapshot, same shape — cursor PID still visible, stays on pid 4.
        let s2 = snap();
        app.latest = Some(s2.clone());
        app.refilter();
        app.ensure_tree_built();
        assert_eq!(cursor_pid(&app), 4);
    }

    #[test]
    fn tree_cursor_jumps_to_first_match_when_old_pid_pruned() {
        let s = snap();
        let mut app = App::new(String::new(), false);
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        // Park cursor on pid 2.
        let pos2 = app
            .tree_visible
            .iter()
            .position(|n| s.processes[n.proc_idx].id.pid == 2)
            .unwrap();
        app.tree_cursor = pos2;
        app.tree_cursor_id = Some(s.processes[app.tree_visible[pos2].proc_idx].id);

        // Filter to pid:4 → pid 2 gets pruned from the visible tree.
        app.set_query("pid:4".to_string());
        app.refilter();
        app.ensure_tree_built();
        // Visible should be {1, 3, 4}; cursor jumps to first match (pid 4).
        assert_eq!(cursor_pid(&app), 4);
    }

    #[test]
    fn signal_target_uses_tree_cursor_regardless_of_focus() {
        let s = snap();
        let mut app = App::new(String::new(), false);
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        // Move cursor to pid 4.
        let pos4 = app
            .tree_visible
            .iter()
            .position(|n| s.processes[n.proc_idx].id.pid == 4)
            .unwrap();
        app.tree_cursor = pos4;
        app.tree_cursor_id = Some(s.processes[app.tree_visible[pos4].proc_idx].id);

        app.focus = Focus::Search;
        let (pid_s, _) = signal_target(&app).unwrap();
        assert_eq!(pid_s, 4);

        app.focus = Focus::Tree;
        let (pid_t, _) = signal_target(&app).unwrap();
        assert_eq!(pid_t, 4);
    }

    #[test]
    fn hint_for_each_focus() {
        let s = hint_for(Focus::Search);
        let t = hint_for(Focus::Tree);
        assert!(!s.is_empty());
        assert!(!t.is_empty());
        assert_ne!(s, t);
    }

    #[test]
    fn flash_active_returns_some_within_window() {
        let now = Instant::now();
        let flash = Some(("err".to_string(), now));
        let later = now + Duration::from_secs(1);
        assert_eq!(flash_active(&flash, later), Some("err"));
    }

    #[test]
    fn flash_active_returns_none_after_window() {
        let now = Instant::now();
        let flash = Some(("err".to_string(), now));
        let later = now + ERROR_FLASH_DURATION + Duration::from_millis(1);
        assert_eq!(flash_active(&flash, later), None);
    }

    #[test]
    fn flash_active_none_when_unset() {
        let flash: Option<(String, Instant)> = None;
        assert_eq!(flash_active(&flash, Instant::now()), None);
    }

    #[test]
    fn needs_confirm_pid_1_true() {
        assert!(needs_confirm(1, 9999));
    }

    #[test]
    fn needs_confirm_self_true() {
        assert!(needs_confirm(42, 42));
    }

    #[test]
    fn needs_confirm_other_false() {
        assert!(!needs_confirm(123, 9999));
    }

    #[test]
    fn signal_target_none_without_snapshot() {
        let app = App::new(String::new(), false);
        assert!(signal_target(&app).is_none());
    }

    fn snap_with_kernel_threads() -> Arc<Snapshot> {
        let mut procs = vec![mk_proc(1, 0), mk_proc(2, 0), mk_proc(3, 1), mk_proc(4, 2)];
        // Mark pid 2 and pid 4 as kernel threads.
        procs[1].is_kernel_thread = true;
        procs[3].is_kernel_thread = true;
        let mut by_id = HashMap::new();
        for (i, p) in procs.iter().enumerate() {
            by_id.insert(p.id, i);
        }
        Arc::new(Snapshot {
            processes: procs,
            by_id,
            sampled_at: Instant::now(),
            system: SystemStats {
                load_1: 0.0,
                load_5: 0.0,
                load_15: 0.0,
                mem_total_bytes: 0,
                mem_used_bytes: 0,
            },
        })
    }

    #[test]
    fn hide_kernel_threads_excluded_from_visible_tree() {
        let s = snap_with_kernel_threads();
        let mut app = App::new(String::new(), true);
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        let visible_pids: Vec<i32> = app
            .tree_visible
            .iter()
            .map(|n| s.processes[n.proc_idx].id.pid)
            .collect();
        // Empty matched + hide_kernel_threads → kthreads stripped from visible.
        assert!(!visible_pids.contains(&2));
        assert!(!visible_pids.contains(&4));
        assert!(visible_pids.contains(&1));
        assert!(visible_pids.contains(&3));
    }

    #[test]
    fn kernel_threads_visible_when_flag_off() {
        let s = snap_with_kernel_threads();
        let mut app = App::new(String::new(), false);
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        assert_eq!(app.tree_visible.len(), 4);
    }

    #[test]
    fn from_boot_restores_focus_paused_query() {
        let boot = PersistedState {
            query_text: "name:foo".to_string(),
            query_cursor: None,
            focus: crate::persist::PersistedFocus::Tree,
            paused: true,
            hide_kernel_threads: true,
            cursor_pid: Some(3),
            cursor_start_time: Some(3),
        };
        let app = App::from_boot(boot);
        assert_eq!(app.query_str(), "name:foo");
        assert_eq!(app.focus, Focus::Tree);
        assert!(app.paused);
        assert!(app.hide_kernel_threads);
        assert_eq!(
            app.pending_cursor_id,
            Some(ProcessId {
                pid: 3,
                start_time: 3
            })
        );
        // Live cursor id is not set until the first snapshot-backed build.
        assert_eq!(app.tree_cursor_id, None);
    }

    #[test]
    fn restore_places_cursor_on_pending_anchor_if_alive() {
        // snap() has pids 1..=4 with start_time == pid; anchor pid 4 is alive.
        let boot = PersistedState {
            cursor_pid: Some(4),
            cursor_start_time: Some(4),
            ..PersistedState::default()
        };
        let mut app = App::from_boot(boot);
        let s = snap();
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        assert_eq!(cursor_pid(&app), 4);
        // Pending anchor consumed; live cursor id now tracks pid 4.
        assert_eq!(app.pending_cursor_id, None);
        assert_eq!(
            app.tree_cursor_id,
            Some(ProcessId {
                pid: 4,
                start_time: 4
            })
        );
    }

    #[test]
    fn restore_cursor_falls_back_when_process_gone() {
        // Anchor references a process not present in the snapshot.
        let boot = PersistedState {
            cursor_pid: Some(999),
            cursor_start_time: Some(999),
            ..PersistedState::default()
        };
        let mut app = App::from_boot(boot);
        let s = snap();
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        // Empty query → no matches → falls back to row 0 (pid 1, the root).
        assert_eq!(cursor_pid(&app), 1);
        assert_eq!(app.pending_cursor_id, None);
    }

    #[test]
    fn should_accept_snapshot_first_even_when_paused() {
        let boot = PersistedState {
            paused: true,
            ..PersistedState::default()
        };
        let mut app = App::from_boot(boot);
        // No snapshot yet: accept the first regardless of pause.
        assert!(app.should_accept_snapshot());
        app.latest = Some(snap());
        // Subsequent snapshots honor pause.
        assert!(!app.should_accept_snapshot());
        app.paused = false;
        assert!(app.should_accept_snapshot());
    }

    #[test]
    fn persisted_state_captures_cursor_after_build() {
        let mut app = App::new(String::new(), false);
        let s = snap();
        app.latest = Some(s.clone());
        app.refilter();
        app.ensure_tree_built();
        let pos4 = app
            .tree_visible
            .iter()
            .position(|n| s.processes[n.proc_idx].id.pid == 4)
            .unwrap();
        app.tree_cursor = pos4;
        app.tree_cursor_id = Some(s.processes[app.tree_visible[pos4].proc_idx].id);

        let ps = app.persisted_state();
        assert_eq!(ps.cursor_pid, Some(4));
        assert_eq!(ps.cursor_start_time, Some(4));
        assert_eq!(ps.query_text, "");
        assert_eq!(ps.focus, crate::persist::PersistedFocus::Search);
    }

    #[test]
    fn query_cursor_restored_from_boot() {
        let boot = PersistedState {
            query_text: "hello".to_string(),
            query_cursor: Some(2),
            ..PersistedState::default()
        };
        let app = App::from_boot(boot);
        assert_eq!(app.query_str(), "hello");
        assert_eq!(app.query_input.cursor(), 2);
    }

    #[test]
    fn query_cursor_defaults_to_end_when_absent() {
        // Older state file (no query_cursor) → caret restored at end of query.
        let boot = PersistedState {
            query_text: "hello".to_string(),
            query_cursor: None,
            ..PersistedState::default()
        };
        let app = App::from_boot(boot);
        assert_eq!(app.query_input.cursor(), 5);
    }

    #[test]
    fn query_cursor_clamped_when_beyond_value() {
        // A stale/oversized cursor is clamped to the query length by `with_cursor`.
        let boot = PersistedState {
            query_text: "hi".to_string(),
            query_cursor: Some(99),
            ..PersistedState::default()
        };
        let app = App::from_boot(boot);
        assert_eq!(app.query_input.cursor(), 2);
    }

    #[test]
    fn persisted_state_round_trips_query_cursor() {
        let mut app = App::new(String::new(), false);
        app.query_input = Input::new("hello world".to_string()).with_cursor(4);
        let ps = app.persisted_state();
        assert_eq!(ps.query_cursor, Some(4));
        assert_eq!(ps.query_text, "hello world");

        let app2 = App::from_boot(ps);
        assert_eq!(app2.query_str(), "hello world");
        assert_eq!(app2.query_input.cursor(), 4);
    }

    #[test]
    fn query_kill_to_start_removes_prefix_and_resets_caret() {
        let mut app = App::new(String::new(), false);
        app.query_input = Input::new("hello world".to_string()).with_cursor(6);
        app.query_kill_to_start();
        assert_eq!(app.query_str(), "world");
        assert_eq!(app.query_input.cursor(), 0);
    }
}
