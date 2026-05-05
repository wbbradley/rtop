use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Cell, Row, Table},
};

use crate::{
    app::state::App,
    consts::{LOAD_VIEW_VISIBLE_ROWS, SCROLLOFF},
    format,
    process::Process,
};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().title(" load ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = app.latest.as_deref() else {
        return;
    };

    let max_rows = inner.height.saturating_sub(1) as usize;
    let visible_rows = max_rows.min(LOAD_VIEW_VISIBLE_ROWS);
    let offset = clamp_offset(
        app.load_view_offset,
        app.load_cursor,
        app.filtered_indices.len(),
        visible_rows,
    );

    let header = Row::new([
        "PID", "USER", "CPU%", "RSS", "TIME+", "STATE", "AGE", "COMMAND",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let widths = [
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Min(10),
    ];

    let end = (offset + visible_rows).min(app.filtered_indices.len());
    let rows: Vec<Row> = app.filtered_indices[offset..end]
        .iter()
        .enumerate()
        .map(|(row_i, &proc_idx)| {
            let absolute_idx = offset + row_i;
            let p = &snap.processes[proc_idx];
            let mut row = build_row(p);
            if absolute_idx == app.load_cursor {
                row = row.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            row
        })
        .collect();

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
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

fn build_row(p: &Process) -> Row<'static> {
    let pid = format!("{:>7}", p.id.pid);
    let user = clip(&p.user, 10);
    let cpu = match p.cpu_pct {
        None => "—".to_string(),
        Some(v) => format!("{v:.1}"),
    };
    let rss = format::bytes(p.rss_bytes);
    let time_plus = format::time_plus(p.cpu_time_total);
    let age = format::time_plus(p.age);
    let state_cell = state_cell(p.state);
    let cmd = render_command(p);

    Row::new(vec![
        Cell::from(pid),
        Cell::from(user),
        Cell::from(cpu),
        Cell::from(rss),
        Cell::from(time_plus),
        state_cell,
        Cell::from(age),
        Cell::from(cmd),
    ])
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn state_cell(state: char) -> Cell<'static> {
    let style = match state {
        'R' => Style::default().fg(Color::Green),
        'D' => Style::default().fg(Color::Red),
        'Z' => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        'T' => Style::default().fg(Color::Yellow),
        _ => Style::default(),
    };
    Cell::from(Span::styled(state.to_string(), style))
}

fn render_command(p: &Process) -> String {
    if p.cmdline.is_empty() {
        format!("[{}]", p.name)
    } else {
        p.cmdline.join(" ")
    }
}
