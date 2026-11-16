use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::Style,
    widgets::{Block, Paragraph},
};

use crate::{
    app::{event::Focus, state::App},
    consts::{FOCUS_ACCENT, SEARCH_BOX_HEIGHT, STATUS_LINE_HEIGHT},
};

pub fn focused_block(title: &'static str, focused: bool) -> Block<'static> {
    let block = Block::bordered().title(title);
    if focused {
        let style = Style::default().fg(FOCUS_ACCENT);
        block.border_style(style).title_style(style)
    } else {
        block
    }
}

pub mod help_modal;
pub mod search_box;
pub mod signal_modal;
pub mod status_line;
pub mod tree_view;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SEARCH_BOX_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(STATUS_LINE_HEIGHT),
        ])
        .split(area);

    search_box::render(frame, chunks[0], app);

    if app.latest.is_none() {
        let block = focused_block(" tree ", app.focus == Focus::Tree);
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);
        let p = Paragraph::new("loading…").alignment(Alignment::Center);
        frame.render_widget(p, inner);
    } else {
        tree_view::render(frame, chunks[1], app);
    }

    status_line::render(frame, chunks[2], app);

    if app.help_open {
        help_modal::render(frame, area, app);
    }
    if app.signal_modal.is_some() {
        signal_modal::render(frame, area, app);
    }
}
