use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::state::{App, flash_active, hint_for},
    format,
};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let left_spans = build_left(app);
    let left = Paragraph::new(Line::from(left_spans));
    frame.render_widget(left, area);

    let now = Instant::now();
    let right: Line = match flash_active(&app.flash, now) {
        Some(msg) => Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        None => Line::from(Span::styled(
            hint_for(app.focus).to_string(),
            Style::default().add_modifier(Modifier::DIM),
        )),
    };
    let right_para = Paragraph::new(right).alignment(Alignment::Right);
    frame.render_widget(right_para, area);
}

fn build_left(app: &App) -> Vec<Span<'static>> {
    let focus = app.focus.label();
    let sort = app.sort.label();

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("[{focus}]  ")));

    match app.latest.as_deref() {
        Some(snap) => {
            let n = app.filtered_indices.len();
            let m = snap.processes.len();
            spans.push(Span::raw(format!("{n}/{m} procs  ")));
            spans.push(Span::raw(format!("[sort: {sort}]")));
            if app.paused {
                spans.push(Span::raw("  [paused]"));
            }
            spans.push(Span::raw(format!(
                "   load: {:.2} {:.2} {:.2}   mem: {}/{}",
                snap.system.load_1,
                snap.system.load_5,
                snap.system.load_15,
                format::bytes(snap.system.mem_used_bytes),
                format::bytes(snap.system.mem_total_bytes),
            )));
        }
        None => {
            spans.push(Span::raw("—/— procs  "));
            spans.push(Span::raw(format!("[sort: {sort}]")));
            if app.paused {
                spans.push(Span::raw("  [paused]"));
            }
            spans.push(Span::raw("   …sampling"));
        }
    }
    spans
}
