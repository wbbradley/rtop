use std::{sync::Arc, time::Instant};

use crate::{
    app::event::{Focus, SortKey},
    consts::{ERROR_FLASH_DURATION, SCROLLOFF},
    process::{ProcessId, Snapshot},
    search::{Query, filter, parse},
    tree::{TreeNode, build_parent_to_children, build_pid_to_idx, build_visible},
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
    pub paused: bool,
    pub sort: SortKey,

    pub load_cursor: usize,
    pub load_view_offset: usize,

    pub filtered_indices: Vec<usize>,

    // Two-key chord tracking for `gg`. Reset on any key other than another `g`.
    pub pending_g: bool,

    pub tree_visible: Vec<TreeNode>,
    pub tree_cursor: usize,
    pub tree_offset: usize,
    /// (Arc<Snapshot> ptr as usize, the load-view-selected ProcessId). When this
    /// matches, no rebuild needed.
    pub tree_cache_key: Option<(usize, ProcessId)>,
    /// ProcessId currently under the tree cursor — used to re-anchor the cursor
    /// across snapshot rebuilds when the load-view selection didn't change.
    pub tree_cursor_id: Option<ProcessId>,

    pub help_open: bool,
    pub signal_modal: Option<SignalModal>,
    pub flash: Option<(String, Instant)>,
}

pub fn hint_for(focus: Focus) -> &'static str {
    match focus {
        Focus::Search => "type to filter | Enter→load | Tab→cycle | ?→help",
        Focus::Load => "j/k move | s sort | space pause | Enter drill | K signal | ?→help",
        Focus::Tree => "j/k move | Enter drill | Tab→search | ?→help",
    }
}

/// Returns true when sending a signal to this pid demands confirmation:
/// pid == 1 (init) or pid == self_pid (suicide).
pub fn needs_confirm(pid: i32, self_pid: i32) -> bool {
    pid == 1 || pid == self_pid
}

/// Target precedence: tree-focus uses tree cursor; otherwise load cursor.
/// Returns (pid, label) where label is "PID <pid>  <cmdline-or-comm>".
pub fn signal_target(app: &App) -> Option<(i32, String)> {
    let snap = app.latest.as_deref()?;
    let proc_idx = match app.focus {
        Focus::Tree => app.tree_visible.get(app.tree_cursor)?.proc_idx,
        Focus::Search | Focus::Load => *app.filtered_indices.get(app.load_cursor)?,
    };
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
    pub fn new(initial_filter: String) -> Self {
        let query = parse(&initial_filter);
        Self {
            latest: None,
            quit: false,
            focus: Focus::Search,
            query_text: initial_filter,
            query,
            paused: false,
            sort: SortKey::Cpu,
            load_cursor: 0,
            load_view_offset: 0,
            filtered_indices: Vec::new(),
            pending_g: false,
            tree_visible: Vec::new(),
            tree_cursor: 0,
            tree_offset: 0,
            tree_cache_key: None,
            tree_cursor_id: None,
            help_open: false,
            signal_modal: None,
            flash: None,
        }
    }

    pub fn set_flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), Instant::now()));
    }

    pub fn refilter(&mut self) {
        self.query = parse(&self.query_text);
        let Some(snap) = self.latest.as_deref() else {
            self.filtered_indices.clear();
            self.load_cursor = 0;
            self.load_view_offset = 0;
            return;
        };
        self.filtered_indices = filter(&self.query, snap, self.sort);

        if let Some(target_pid) = self.query.auto_select_pid
            && let Some(pos) = self.filtered_indices.iter().position(|&i| {
                self.latest
                    .as_deref()
                    .map(|s| s.processes[i].id.pid == target_pid)
                    .unwrap_or(false)
            })
        {
            self.load_cursor = pos;
        } else {
            let max = self.filtered_indices.len().saturating_sub(1);
            if self.load_cursor > max {
                self.load_cursor = max;
            }
        }
        self.adjust_offset_for_scrolloff(usize::MAX);
    }

    // Keep the cursor within scrolloff of the visible viewport.
    // `visible_rows` is the number of data rows the load view will render; pass
    // `usize::MAX` from contexts that don't yet know it (final clamp happens at render).
    pub fn adjust_offset_for_scrolloff(&mut self, visible_rows: usize) {
        if self.filtered_indices.is_empty() {
            self.load_view_offset = 0;
            return;
        }
        let so = SCROLLOFF;
        if self.load_cursor < self.load_view_offset + so {
            self.load_view_offset = self.load_cursor.saturating_sub(so);
        }
        if visible_rows != usize::MAX
            && self.load_cursor + so + 1 > self.load_view_offset + visible_rows
        {
            self.load_view_offset = (self.load_cursor + so + 1).saturating_sub(visible_rows);
        }
        let max_offset = self
            .filtered_indices
            .len()
            .saturating_sub(visible_rows.min(self.filtered_indices.len()));
        if self.load_view_offset > max_offset {
            self.load_view_offset = max_offset;
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

    pub fn selected_process_id(&self) -> Option<ProcessId> {
        let snap = self.latest.as_deref()?;
        let &idx = self.filtered_indices.get(self.load_cursor)?;
        snap.processes.get(idx).map(|p| p.id)
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
        let Some(selected_id) = self.selected_process_id() else {
            self.tree_visible.clear();
            self.tree_cursor = 0;
            self.tree_offset = 0;
            self.tree_cache_key = None;
            self.tree_cursor_id = None;
            return;
        };

        let key = (Arc::as_ptr(&snap) as usize, selected_id);
        if self.tree_cache_key == Some(key) {
            return;
        }

        let prev_selected = self.tree_cache_key.map(|(_, s)| s);

        let pid_to_idx = build_pid_to_idx(&snap);
        let parent_to_children = build_parent_to_children(&snap);
        self.tree_visible = build_visible(&snap, &parent_to_children, &pid_to_idx, selected_id);

        if self.tree_visible.is_empty() {
            self.tree_cursor = 0;
            self.tree_offset = 0;
            self.tree_cursor_id = None;
            self.tree_cache_key = Some(key);
            return;
        }

        let selection_changed = prev_selected != Some(selected_id);
        let new_cursor = if selection_changed {
            self.tree_visible
                .iter()
                .position(|n| snap.processes[n.proc_idx].id == selected_id)
                .unwrap_or(0)
        } else if let Some(prev_cursor_id) = self.tree_cursor_id {
            self.tree_visible
                .iter()
                .position(|n| snap.processes[n.proc_idx].id == prev_cursor_id)
                .or_else(|| {
                    self.tree_visible
                        .iter()
                        .position(|n| snap.processes[n.proc_idx].id == selected_id)
                })
                .unwrap_or(0)
        } else {
            self.tree_visible
                .iter()
                .position(|n| snap.processes[n.proc_idx].id == selected_id)
                .unwrap_or(0)
        };
        self.tree_cursor = new_cursor.min(self.tree_visible.len().saturating_sub(1));
        self.tree_cursor_id = Some(snap.processes[self.tree_visible[self.tree_cursor].proc_idx].id);
        self.tree_cache_key = Some(key);
        self.adjust_tree_offset_for_scrolloff(usize::MAX);
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

    #[test]
    fn cursor_resets_on_selection_change() {
        let s = snap();
        let mut app = App::new(String::new());
        app.latest = Some(s.clone());
        app.refilter();

        // First build: load cursor at index 0; depending on sort that resolves to
        // some process. Force it deterministically by selecting pid 1.
        let pid1_pos = app
            .filtered_indices
            .iter()
            .position(|&i| s.processes[i].id.pid == 1)
            .unwrap();
        app.load_cursor = pid1_pos;
        app.ensure_tree_built();
        // Tree visible for pid 1 is [1,2,3,4]; cursor lands on pid 1 (position 0).
        assert_eq!(app.tree_cursor, 0);
        let cursor_pid = s.processes[app.tree_visible[app.tree_cursor].proc_idx]
            .id
            .pid;
        assert_eq!(cursor_pid, 1);

        // Move user's tree cursor to pid 4 (last visible row).
        let pid4_pos = app
            .tree_visible
            .iter()
            .position(|n| s.processes[n.proc_idx].id.pid == 4)
            .unwrap();
        app.tree_cursor = pid4_pos;
        app.tree_cursor_id = Some(s.processes[app.tree_visible[app.tree_cursor].proc_idx].id);

        // Now change load selection to pid 3 — tree cursor should jump to the
        // newly selected node (pid 3), not stay on pid 4.
        let pid3_pos = app
            .filtered_indices
            .iter()
            .position(|&i| s.processes[i].id.pid == 3)
            .unwrap();
        app.load_cursor = pid3_pos;
        app.ensure_tree_built();
        let cursor_pid_after = s.processes[app.tree_visible[app.tree_cursor].proc_idx]
            .id
            .pid;
        assert_eq!(cursor_pid_after, 3);
    }

    #[test]
    fn cursor_preserved_across_snapshot_when_selection_unchanged() {
        let s1 = snap();
        let mut app = App::new(String::new());
        app.latest = Some(s1.clone());
        app.refilter();
        let pid1_pos = app
            .filtered_indices
            .iter()
            .position(|&i| s1.processes[i].id.pid == 1)
            .unwrap();
        app.load_cursor = pid1_pos;
        app.ensure_tree_built();
        // Move tree cursor onto pid 3.
        let pid3_idx = app
            .tree_visible
            .iter()
            .position(|n| s1.processes[n.proc_idx].id.pid == 3)
            .unwrap();
        app.tree_cursor = pid3_idx;
        app.tree_cursor_id = Some(s1.processes[app.tree_visible[app.tree_cursor].proc_idx].id);

        // New snapshot, same shape — re-anchor by id.
        let s2 = snap();
        app.latest = Some(s2.clone());
        app.refilter();
        let pid1_pos2 = app
            .filtered_indices
            .iter()
            .position(|&i| s2.processes[i].id.pid == 1)
            .unwrap();
        app.load_cursor = pid1_pos2;
        app.ensure_tree_built();
        let cursor_pid = s2.processes[app.tree_visible[app.tree_cursor].proc_idx]
            .id
            .pid;
        assert_eq!(cursor_pid, 3);
    }

    #[test]
    fn hint_for_each_focus() {
        let s = hint_for(Focus::Search);
        let l = hint_for(Focus::Load);
        let t = hint_for(Focus::Tree);
        assert!(!s.is_empty());
        assert!(!l.is_empty());
        assert!(!t.is_empty());
        assert_ne!(s, l);
        assert_ne!(l, t);
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

    fn pid_at_load_cursor(app: &App) -> i32 {
        let s = app.latest.as_deref().unwrap();
        s.processes[app.filtered_indices[app.load_cursor]].id.pid
    }

    #[test]
    fn signal_target_uses_load_cursor_when_focus_search() {
        let s = snap();
        let mut app = App::new(String::new());
        app.latest = Some(s.clone());
        app.refilter();
        app.load_cursor = 0;
        app.focus = Focus::Search;
        let expected_pid = pid_at_load_cursor(&app);
        let (pid, label) = signal_target(&app).unwrap();
        assert_eq!(pid, expected_pid);
        assert!(label.starts_with(&format!("PID {expected_pid}")));
    }

    #[test]
    fn signal_target_uses_load_cursor_when_focus_load() {
        let s = snap();
        let mut app = App::new(String::new());
        app.latest = Some(s.clone());
        app.refilter();
        app.load_cursor = 1;
        app.focus = Focus::Load;
        let expected_pid = pid_at_load_cursor(&app);
        let (pid, _) = signal_target(&app).unwrap();
        assert_eq!(pid, expected_pid);
    }

    #[test]
    fn signal_target_uses_tree_cursor_when_focus_tree() {
        let s = snap();
        let mut app = App::new(String::new());
        app.latest = Some(s.clone());
        app.refilter();
        let pid1_pos = app
            .filtered_indices
            .iter()
            .position(|&i| s.processes[i].id.pid == 1)
            .unwrap();
        app.load_cursor = pid1_pos;
        app.ensure_tree_built();
        // Move tree cursor to pid 4 explicitly.
        let pid4_pos = app
            .tree_visible
            .iter()
            .position(|n| s.processes[n.proc_idx].id.pid == 4)
            .unwrap();
        app.tree_cursor = pid4_pos;
        app.focus = Focus::Tree;
        let (pid, _) = signal_target(&app).unwrap();
        assert_eq!(pid, 4);
    }

    #[test]
    fn signal_target_none_without_snapshot() {
        let app = App::new(String::new());
        assert!(signal_target(&app).is_none());
    }
}
