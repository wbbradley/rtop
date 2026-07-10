use std::{collections::HashSet, sync::Arc, time::Instant};

use crate::{
    app::event::Focus,
    consts::{ERROR_FLASH_DURATION, SCROLLOFF},
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
    pub query_text: String,
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
    /// (Arc<Snapshot> ptr as usize, query_text). When this matches, no rebuild.
    pub tree_cache_key: Option<(usize, String)>,
    /// ProcessId currently under the tree cursor — used to re-anchor across
    /// rebuilds when the same PID is still visible.
    pub tree_cursor_id: Option<ProcessId>,

    pub help_open: bool,
    pub signal_modal: Option<SignalModal>,
    pub flash: Option<(String, Instant)>,

    pub hide_kernel_threads: bool,
}

pub fn hint_for(focus: Focus) -> &'static str {
    match focus {
        Focus::Search => "type to filter | Esc reset | Enter→tree | ?→help",
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
    pub fn new(initial_filter: String, hide_kernel_threads: bool) -> Self {
        let query = parse(&initial_filter);
        let compiled = CompiledQuery::compile(&query);
        Self {
            latest: None,
            quit: false,
            focus: Focus::Search,
            query_text: initial_filter,
            query,
            compiled,
            paused: false,
            matched_pids: HashSet::new(),
            pending_g: false,
            tree_visible: Vec::new(),
            tree_cursor: 0,
            tree_offset: 0,
            tree_cache_key: None,
            tree_cursor_id: None,
            help_open: false,
            signal_modal: None,
            flash: None,
            hide_kernel_threads,
        }
    }

    pub fn set_flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), Instant::now()));
    }

    /// Recompute the matched-PID set from the current query text.
    pub fn refilter(&mut self) {
        self.query = parse(&self.query_text);
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

        let key = (Arc::as_ptr(&snap) as usize, self.query_text.clone());
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
        // first matched node (or row 0 if none).
        let new_cursor = if let Some(prev_id) = self.tree_cursor_id
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
        app.query_text = "pid:4".into();
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
}
