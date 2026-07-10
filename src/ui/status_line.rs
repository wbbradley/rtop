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

const INVALID_REGEX_HINT: &str = "invalid regex";

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let left_spans = build_left(app);
    let left = Paragraph::new(Line::from(left_spans));
    frame.render_widget(left, area);

    let now = Instant::now();
    let right: Line = if let Some(msg) = flash_active(&app.flash, now) {
        // A transient kill-error flash wins the slot (red bold).
        Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else if app.compiled.has_invalid() {
        // Persistent low-key hint while the query has an uncompilable term.
        Line::from(Span::styled(
            INVALID_REGEX_HINT.to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ))
    } else {
        Line::from(Span::styled(
            hint_for(app.focus).to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ))
    };
    let right_para = Paragraph::new(right).alignment(Alignment::Right);
    frame.render_widget(right_para, area);
}

fn build_left(app: &App) -> Vec<Span<'static>> {
    let focus = app.focus.label();

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("[{focus}]  ")));

    match app.latest.as_deref() {
        Some(snap) => {
            let matched = if app.query.groups.is_empty() {
                snap.processes.len()
            } else {
                app.matched_pids.len()
            };
            let total = snap.processes.len();
            spans.push(Span::raw(format!("{matched}/{total} procs")));
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
            spans.push(Span::raw("—/— procs"));
            if app.paused {
                spans.push(Span::raw("  [paused]"));
            }
            spans.push(Span::raw("   …sampling"));
        }
    }
    spans
}
