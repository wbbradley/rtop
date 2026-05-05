use std::sync::Arc;

use crate::{
    app::event::{Focus, SortKey},
    consts::SCROLLOFF,
    process::Snapshot,
    search::{Query, filter, parse},
};

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
        }
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
}
