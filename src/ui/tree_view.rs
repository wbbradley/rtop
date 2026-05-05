use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::{
    app::state::App,
    consts::SCROLLOFF,
    format,
    process::Process,
    tree::{GutterKind, TreeNode},
};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().title(" tree ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = app.latest.as_deref() else {
        return;
    };
    if app.tree_visible.is_empty() {
        return;
    }

    let visible_rows = inner.height as usize;
    if visible_rows == 0 {
        return;
    }

    let offset = clamp_offset(
        app.tree_offset,
        app.tree_cursor,
        app.tree_visible.len(),
        visible_rows,
    );
    let end = (offset + visible_rows).min(app.tree_visible.len());

    let lines: Vec<Line> = app.tree_visible[offset..end]
        .iter()
        .enumerate()
        .map(|(row_i, node)| {
            let absolute_idx = offset + row_i;
            let p = &snap.processes[node.proc_idx];
            let parent_user = parent_user(snap, node, &app.tree_visible);
            let mut line = build_line(p, node, parent_user.as_deref());
            if absolute_idx == app.tree_cursor {
                line = line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            line
        })
        .collect();

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}

fn parent_user(
    snap: &crate::process::Snapshot,
    node: &TreeNode,
    visible: &[TreeNode],
) -> Option<String> {
    if node.depth == 0 {
        return None;
    }
    let p = &snap.processes[node.proc_idx];
    let parent_idx = snap
        .processes
        .iter()
        .position(|q| q.id.pid == p.ppid)
        .or_else(|| {
            // fall back to walking the visible list backwards
            visible
                .iter()
                .rev()
                .find(|n| snap.processes[n.proc_idx].id.pid == p.ppid)
                .map(|n| n.proc_idx)
        })?;
    Some(snap.processes[parent_idx].user.clone())
}

fn clamp_offset(mut offset: usize, cursor: usize, total: usize, visible: usize) -> usize {
    if total == 0 || visible == 0 {
        return 0;
    }
    let so = SCROLLOFF;
    if cursor < offset + so {
        offset = cursor.saturating_sub(so);
    }
    if cursor + so + 1 > offset + visible {
        offset = (cursor + so + 1).saturating_sub(visible);
    }
    let max_offset = total.saturating_sub(visible);
    if offset > max_offset {
        offset = max_offset;
    }
    offset
}

fn build_line(p: &Process, node: &TreeNode, parent_user: Option<&str>) -> Line<'static> {
    let pid = format!("{:>7}", p.id.pid);
    let cpu = match p.cpu_pct {
        None => "    —".to_string(),
        Some(v) => format!("{v:>5.1}"),
    };
    let rss = format!("{:>8}", format::bytes(p.rss_bytes));

    let gutter = build_gutter(node);
    let cmd = render_command(p);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(8);
    spans.push(Span::raw(pid));
    spans.push(Span::raw(" "));
    spans.push(Span::raw(cpu));
    spans.push(Span::raw(" "));
    spans.push(Span::raw(rss));
    spans.push(Span::raw("  "));
    spans.push(Span::raw(gutter));
    spans.push(Span::raw(cmd));

    let show_user = match parent_user {
        Some(pu) => pu != p.user,
        None => false,
    };
    if show_user {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("[{}]", p.user),
            Style::default().fg(Color::Cyan),
        ));
    }
    Line::from(spans)
}

fn build_gutter(node: &TreeNode) -> String {
    let mut s = String::with_capacity(node.depth * 3 + 3);
    for &last in &node.ancestors_last {
        if last {
            s.push_str("   ");
        } else {
            s.push_str("│  ");
        }
    }
    if node.depth == 0 {
        return s;
    }
    match node.gutter_kind {
        GutterKind::Spine => s.push_str("└─ "),
        GutterKind::Branch => s.push_str("├─ "),
        GutterKind::Leaf => s.push_str("└─ "),
    }
    s
}

fn render_command(p: &Process) -> String {
    if p.cmdline.is_empty() {
        format!("[{}]", p.name)
    } else {
        p.cmdline.join(" ")
    }
}
