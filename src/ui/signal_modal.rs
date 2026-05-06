use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::{
    app::state::App,
    consts::{SIGNAL_MODAL_HEIGHT, SIGNAL_MODAL_WIDTH},
    signal::SIGNAL_CHOICES,
};

pub fn render(frame: &mut Frame, full_area: Rect, app: &App) {
    let Some(modal) = app.signal_modal.as_ref() else {
        return;
    };
    let rect = centered_rect(full_area, SIGNAL_MODAL_WIDTH, SIGNAL_MODAL_HEIGHT);
    frame.render_widget(Clear, rect);

    let title = if modal.awaiting_confirm {
        " confirm? (y/N) "
    } else {
        " send signal "
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let inner_w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(SIGNAL_CHOICES.len() + 3);

    let mut label = modal.target_label.clone();
    if label.chars().count() > inner_w {
        label = label.chars().take(inner_w).collect();
    }
    lines.push(Line::from(label));
    lines.push(Line::from(""));

    for (i, choice) in SIGNAL_CHOICES.iter().enumerate() {
        let is_cursor = i == modal.cursor;
        let prefix = if is_cursor { " > " } else { "   " };
        let row_text = format!("{prefix}SIG{}", choice.label);
        let style = if is_cursor {
            Style::default().add_modifier(Modifier::REVERSED)
        } else if choice.label == "TERM" {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(row_text, style)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k pick · Enter send · Esc cancel",
        Style::default().add_modifier(Modifier::DIM),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}
