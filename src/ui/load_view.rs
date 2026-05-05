use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Row, Table},
};

use crate::{
    format,
    process::{Process, Snapshot},
};

pub fn render(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let mut indices: Vec<usize> = (0..snap.processes.len()).collect();
    indices.sort_by(|&a, &b| {
        let ca = snap.processes[a].cpu_pct;
        let cb = snap.processes[b].cpu_pct;
        // None sorts last; Some sorted descending.
        match (ca, cb) {
            (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    let rows: Vec<Row> = indices
        .iter()
        .map(|&i| build_row(&snap.processes[i]))
        .collect();

    let header = Row::new(["PID", "USER", "CPU%", "RSS", "COMMAND"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let widths = [
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, area);
}

fn build_row(p: &Process) -> Row<'static> {
    let pid = p.id.pid.to_string();
    let user = p.user.clone();
    let cpu = match p.cpu_pct {
        None => "—".to_string(),
        Some(v) => format!("{v:.1}"),
    };
    let rss = format::bytes(p.rss_bytes);
    let cmd = render_command(p);
    Row::new([pid, user, cpu, rss, cmd])
}

fn render_command(p: &Process) -> String {
    if p.cmdline.is_empty() {
        format!("[{}]", p.name)
    } else {
        p.cmdline.join(" ")
    }
}
