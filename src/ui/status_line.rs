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
    let left = Line::from(build_left(app));
    let right = build_right(app, Instant::now());

    // The left stats and the right-aligned hint share one row but must never
    // overlap. Give the stats their exact width (capped to the row) and hand
    // the hint whatever remains: at narrow widths the hint truncates into — or
    // vanishes from — its own lane rather than clobbering the load/mem figures.
    let left_w = (left.width() as u16).min(area.width);
    let left_area = Rect {
        width: left_w,
        ..area
    };
    let right_area = Rect {
        x: area.x + left_w,
        width: area.width - left_w,
        ..area
    };

    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        right_area,
    );
}

/// The right-slot line: a transient kill-error flash (red bold) wins, then a
/// persistent "invalid regex" hint while the query has an uncompilable term,
/// otherwise the context hint for the focused pane (dim).
fn build_right(app: &App, now: Instant) -> Line<'static> {
    if let Some(msg) = flash_active(&app.flash, now) {
        Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else if app.compiled.has_invalid() {
        Line::from(Span::styled(
            INVALID_REGEX_HINT.to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ))
    } else {
        Line::from(Span::styled(
            hint_for(app.focus).to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ))
    }
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Instant};

    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;
    use crate::{
        app::state::App,
        process::{Snapshot, SystemStats},
    };

    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;

    /// An app with one populated snapshot: 512MiB/1.0GiB memory so the left
    /// stats end in a recognizable `mem: 512MiB/1.0GiB` figure. Focus stays on
    /// the search box (the longest, most clobber-prone hint).
    fn app_with_stats() -> App {
        let mut app = App::new(String::new(), false);
        app.latest = Some(Arc::new(Snapshot {
            processes: Vec::new(),
            by_id: HashMap::new(),
            sampled_at: Instant::now(),
            system: SystemStats {
                load_1: 0.0,
                load_5: 0.0,
                load_15: 0.0,
                mem_total_bytes: GIB,
                mem_used_bytes: 512 * MIB,
            },
        }));
        app
    }

    /// Render the status line into a single-row buffer of the given width and
    /// return that row as a string.
    fn render_row(app: &App, width: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), app))
            .unwrap();
        row_text(terminal.backend().buffer(), 0)
    }

    fn row_text(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    #[test]
    fn left_stats_survive_when_hint_would_overlap() {
        // At 100 cols the left stats (~63) plus the search hint (~48) exceed
        // the row, so the old single-area render clobbered the `mem:` figure.
        let app = app_with_stats();
        let row = render_row(&app, 100);
        assert!(
            row.contains("mem: 512MiB/1.0GiB"),
            "mem figure was clobbered: {row:?}"
        );
    }

    #[test]
    fn stats_and_hint_coexist_when_wide() {
        // With ample width both halves render into disjoint columns.
        let app = app_with_stats();
        let row = render_row(&app, 200);
        assert!(row.contains("mem: 512MiB/1.0GiB"), "missing stats: {row:?}");
        assert!(row.contains("?→help"), "missing hint tail: {row:?}");
    }

    #[test]
    fn narrow_row_never_panics_and_keeps_stats_prefix() {
        // Even when the row is too narrow for the full stats, rendering must not
        // panic and the left half owns the leading columns.
        let app = app_with_stats();
        let row = render_row(&app, 30);
        assert!(row.starts_with("[search]"), "left half displaced: {row:?}");
    }
}
