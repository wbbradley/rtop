use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{event::Focus, state::App},
    consts::{CPU_DANGER_PCT, CPU_WARN_PCT, SCROLLOFF},
    format,
    process::Process,
    tree::{GutterKind, TreeNode},
};

const MAX_TREE_WRAP_LINES: usize = 3;
// pid (7) + " " (1) + cpu (5) + " " (1) + rss (8) + "  " (2) = 24.
const METADATA_PREFIX_WIDTH: usize = 24;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = crate::ui::focused_block(" tree ", app.focus == Focus::Tree);
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
    let pane_width = inner.width as usize;

    let heights: Vec<usize> = app
        .tree_visible
        .iter()
        .map(|node| {
            let p = &snap.processes[node.proc_idx];
            let parent_user = parent_user(snap, node, &app.tree_visible);
            row_visual_height(p, node, parent_user.as_deref(), pane_width)
        })
        .collect();

    let offset = clamp_offset(
        app.tree_offset,
        app.tree_cursor,
        app.tree_visible.len(),
        visible_rows,
        |i| heights[i],
    );

    let mut lines: Vec<Line> = Vec::with_capacity(visible_rows);
    let mut consumed = 0usize;
    let mut idx = offset;
    while idx < app.tree_visible.len() && consumed < visible_rows {
        let node = &app.tree_visible[idx];
        let p = &snap.processes[node.proc_idx];
        let parent_user = parent_user(snap, node, &app.tree_visible);
        let row_lines = build_row(p, node, parent_user.as_deref(), pane_width);
        let selected = idx == app.tree_cursor;
        for line in row_lines {
            if consumed >= visible_rows {
                break;
            }
            let line = if selected {
                line.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                line
            };
            lines.push(line);
            consumed += 1;
        }
        idx += 1;
    }

    frame.render_widget(Paragraph::new(lines), inner);
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

fn clamp_offset(
    offset_in: usize,
    cursor: usize,
    total: usize,
    visible_rows: usize,
    row_height: impl Fn(usize) -> usize,
) -> usize {
    if total == 0 || visible_rows == 0 {
        return 0;
    }
    let so = SCROLLOFF;
    let mut offset = offset_in;

    // Top scrolloff: bring offset down if cursor is too close to viewport top.
    if cursor < offset + so {
        offset = cursor.saturating_sub(so);
    }

    // Bottom: find smallest `min_offset` such that rows [min_offset..=last_idx]
    // fit within `visible_rows` visual lines. Walk upward from last_idx summing
    // heights.
    let last_idx = (cursor + so).min(total - 1);
    let mut used = 0usize;
    let mut min_offset: Option<usize> = None;
    for i in (0..=last_idx).rev() {
        let h = row_height(i);
        if used + h > visible_rows {
            break;
        }
        used += h;
        min_offset = Some(i);
    }
    // If even the bottom-most row didn't fit, or the cursor row didn't make it
    // into the fitting window, fall back to cursor (cursor row top-aligned,
    // continuations clipped at bottom).
    let min_offset = match min_offset {
        Some(m) if m <= cursor => m,
        _ => cursor,
    };
    if offset < min_offset {
        offset = min_offset;
    }

    // Cursor must always be visible.
    offset.min(cursor)
}

fn row_budgets(
    node: &TreeNode,
    show_user: bool,
    user_tag_w: usize,
    pane_width: usize,
) -> (usize, usize) {
    let gutter = build_gutter(node);
    let prefix_width = METADATA_PREFIX_WIDTH + gutter.chars().count();
    let avail = pane_width.saturating_sub(prefix_width);
    // 8 is a local floor: if the budget is too tight, let the user tag clip
    // rather than squeezing the command down to nothing.
    let first_budget = if show_user && avail > user_tag_w + 8 {
        avail - user_tag_w
    } else {
        avail
    };
    (first_budget, avail)
}

fn row_visual_height(
    p: &Process,
    node: &TreeNode,
    parent_user: Option<&str>,
    pane_width: usize,
) -> usize {
    if p.cmdline.is_empty() {
        return 1;
    }
    let show_user = match parent_user {
        Some(pu) => pu != p.user,
        None => false,
    };
    let user_tag_w = if show_user {
        format!(" [{}]", p.user).chars().count()
    } else {
        0
    };
    let (first_budget, cont_budget) = row_budgets(node, show_user, user_tag_w, pane_width);
    wrap_argv(&p.cmdline, first_budget, cont_budget, MAX_TREE_WRAP_LINES)
        .len()
        .max(1)
}

fn build_row(
    p: &Process,
    node: &TreeNode,
    parent_user: Option<&str>,
    pane_width: usize,
) -> Vec<Line<'static>> {
    let pid = format!("{:>7}", p.id.pid);
    let cpu = cpu_span(p.cpu_pct);
    let rss = format!("{:>8}", format::bytes(p.rss_bytes));
    let gutter = build_gutter(node);

    let show_user = match parent_user {
        Some(pu) => pu != p.user,
        None => false,
    };
    let user_tag = if show_user {
        format!(" [{}]", p.user)
    } else {
        String::new()
    };
    let user_tag_w = user_tag.chars().count();

    let (first_budget, cont_budget) = row_budgets(node, show_user, user_tag_w, pane_width);

    let wrapped: Vec<String> = if p.cmdline.is_empty() {
        vec![format!("[{}]", p.name)]
    } else {
        wrap_argv(&p.cmdline, first_budget, cont_budget, MAX_TREE_WRAP_LINES)
    };

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(wrapped.len());
    let mut iter = wrapped.into_iter();
    if let Some(first) = iter.next() {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(10);
        spans.push(Span::raw(pid));
        spans.push(Span::raw(" "));
        spans.push(cpu);
        spans.push(Span::raw(" "));
        spans.push(Span::raw(rss));
        spans.push(Span::raw("  "));
        spans.push(Span::raw(gutter));
        spans.push(Span::raw(first));
        if show_user {
            spans.push(Span::styled(user_tag, Style::default().fg(Color::Cyan)));
        }
        lines.push(apply_dim_if_kthread(Line::from(spans), p));
    }

    let cont_prefix = continuation_prefix(node);
    for segment in iter {
        let line = Line::from(vec![Span::raw(cont_prefix.clone()), Span::raw(segment)]);
        lines.push(apply_dim_if_kthread(line, p));
    }

    lines
}

fn apply_dim_if_kthread(line: Line<'static>, p: &Process) -> Line<'static> {
    if p.is_kernel_thread {
        line.style(Style::default().add_modifier(Modifier::DIM))
    } else {
        line
    }
}

fn continuation_prefix(node: &TreeNode) -> String {
    let mut s = String::with_capacity(METADATA_PREFIX_WIDTH + node.depth * 3 + 3);
    for _ in 0..METADATA_PREFIX_WIDTH {
        s.push(' ');
    }
    for &last in &node.ancestors_last {
        if last {
            s.push_str("   ");
        } else {
            s.push_str("│  ");
        }
    }
    if node.depth > 0 {
        // Blank where the first line had `└─ ` / `├─ `.
        s.push_str("   ");
    }
    s
}

/// Pack argv tokens greedily into lines bounded by `first_budget` (line 0) and
/// `cont_budget` (lines 1+). At most `max_lines` lines; trailing residue is
/// dropped and the last line is ellipsized. Tokens wider than the budget are
/// hard-broken at the column edge.
fn wrap_argv(
    tokens: &[String],
    first_budget: usize,
    cont_budget: usize,
    max_lines: usize,
) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let budget_at = |i: usize| if i == 0 { first_budget } else { cont_budget };

    let mut queue: VecDeque<String> = tokens.iter().cloned().collect();
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    let mut line_idx = 0usize;

    while let Some(tok) = queue.pop_front() {
        let tok_len = tok.chars().count();
        let budget = budget_at(line_idx);

        if current.is_empty() {
            if tok_len <= budget {
                current.push_str(&tok);
                current_len = tok_len;
            } else if budget == 0 {
                // Cannot place anything on this line — advance.
                queue.push_front(tok);
                lines.push(std::mem::take(&mut current));
                current_len = 0;
                line_idx += 1;
                if lines.len() >= max_lines {
                    break;
                }
            } else {
                // Hard-break the token at the column edge.
                let head: String = tok.chars().take(budget).collect();
                let tail: String = tok.chars().skip(budget).collect();
                current_len = head.chars().count();
                current.push_str(&head);
                if !tail.is_empty() {
                    queue.push_front(tail);
                }
            }
        } else {
            let needed = current_len + 1 + tok_len;
            if needed <= budget {
                current.push(' ');
                current.push_str(&tok);
                current_len = needed;
            } else {
                lines.push(std::mem::take(&mut current));
                current_len = 0;
                line_idx += 1;
                queue.push_front(tok);
                if lines.len() >= max_lines {
                    break;
                }
            }
        }
    }

    // Flush remaining current line if room.
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(std::mem::take(&mut current));
    } else if lines.is_empty() && max_lines > 0 {
        // Edge case: nothing produced (e.g. empty input). Surface an empty
        // line so callers can rely on len() >= 1 for any non-zero max_lines
        // with at least one input token.
        if !tokens.is_empty() {
            lines.push(String::new());
        }
    }

    // Truncate with ellipsis if there's residual content.
    if !queue.is_empty() && !lines.is_empty() {
        let last_budget = budget_at(lines.len() - 1);
        let last = lines.last_mut().expect("non-empty checked above");
        while last.chars().count() + 1 > last_budget && !last.is_empty() {
            last.pop();
        }
        last.push('…');
    }

    lines
}

fn cpu_span(cpu_pct: Option<f32>) -> Span<'static> {
    match cpu_pct {
        None => Span::styled("    —", Style::default().add_modifier(Modifier::DIM)),
        Some(v) => {
            let style = if v >= CPU_DANGER_PCT {
                Style::default().fg(Color::Red)
            } else if v >= CPU_WARN_PCT {
                Style::default().fg(Color::Yellow)
            } else if v < 1.0 {
                Style::default().add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };
            Span::styled(format!("{v:>5.1}"), style)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> String {
        x.to_string()
    }

    // ---- wrap_argv ----

    #[test]
    fn wrap_argv_single_short_token_one_line() {
        let toks = vec![s("ls")];
        assert_eq!(wrap_argv(&toks, 10, 10, 3), vec![s("ls")]);
    }

    #[test]
    fn wrap_argv_packs_until_budget() {
        let toks = vec![s("abc"), s("de"), s("fgh")];
        assert_eq!(wrap_argv(&toks, 10, 10, 3), vec![s("abc de fgh")]);
    }

    #[test]
    fn wrap_argv_wraps_on_token_boundary() {
        let toks = vec![s("aaaa"), s("bbbb"), s("cccc")];
        assert_eq!(
            wrap_argv(&toks, 5, 5, 3),
            vec![s("aaaa"), s("bbbb"), s("cccc")]
        );
    }

    #[test]
    fn wrap_argv_hard_breaks_long_token() {
        let toks = vec![s("xxxxxxxxxx")];
        assert_eq!(
            wrap_argv(&toks, 4, 4, 3),
            vec![s("xxxx"), s("xxxx"), s("xx")]
        );
    }

    #[test]
    fn wrap_argv_hard_breaks_long_token_with_truncation() {
        let tok = "x".repeat(20);
        let result = wrap_argv(std::slice::from_ref(&tok), 4, 4, 3);
        assert_eq!(result, vec![s("xxxx"), s("xxxx"), s("xxx…")]);
    }

    #[test]
    fn wrap_argv_caps_at_three_lines_with_ellipsis() {
        let toks: Vec<String> = (0..20).map(|_| s("a")).collect();
        let result = wrap_argv(&toks, 3, 3, 3);
        assert_eq!(result.len(), 3);
        assert!(result[2].ends_with('…'), "got {:?}", result);
    }

    #[test]
    fn wrap_argv_first_vs_cont_budget() {
        let toks = vec![s("aaa"), s("bb"), s("ccc")];
        let result = wrap_argv(&toks, 3, 10, 3);
        assert_eq!(result, vec![s("aaa"), s("bb ccc")]);
    }

    #[test]
    fn wrap_argv_zero_first_budget_uses_continuation() {
        let toks = vec![s("abc")];
        let result = wrap_argv(&toks, 0, 10, 3);
        assert_eq!(result, vec![s(""), s("abc")]);
    }

    // ---- clamp_offset ----

    #[test]
    fn clamp_offset_empty() {
        assert_eq!(clamp_offset(0, 0, 0, 10, |_| 1), 0);
    }

    #[test]
    fn clamp_offset_uniform_heights_top_scrolloff() {
        // cursor=2 near top with stored offset=10 → snap down to 0.
        let r = clamp_offset(10, 2, 20, 10, |_| 1);
        assert_eq!(r, 0);
    }

    #[test]
    fn clamp_offset_uniform_heights_bottom_scrolloff() {
        // cursor=15, visible=10, total=20, SCROLLOFF=3:
        // (cursor + SCROLLOFF + 1) - visible = 9.
        let r = clamp_offset(0, 15, 20, 10, |_| 1);
        assert_eq!(r, 9);
    }

    #[test]
    fn clamp_offset_tall_row_forces_higher_offset() {
        // Heights mostly 1, but index 4 has height 3 — just below cursor=3.
        let mut heights = [1usize; 20];
        heights[4] = 3;
        let r_tall = clamp_offset(0, 3, heights.len(), 5, |i| heights[i]);
        let r_uniform = clamp_offset(0, 3, heights.len(), 5, |_| 1);
        assert!(
            r_tall > r_uniform,
            "tall={} should exceed uniform={}",
            r_tall,
            r_uniform
        );
    }

    #[test]
    fn clamp_offset_cursor_row_taller_than_viewport() {
        // cursor row's own height exceeds visible_rows → top-align cursor.
        let mut heights = [1usize; 20];
        heights[5] = 5;
        let r = clamp_offset(0, 5, 20, 3, |i| heights[i]);
        assert_eq!(r, 5);
    }
}
