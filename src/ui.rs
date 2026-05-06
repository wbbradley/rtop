use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    widgets::{Block, Paragraph},
};

use crate::{
    app::state::App,
    consts::{LOAD_VIEW_HEIGHT, MIN_COLS, MIN_ROWS, SEARCH_BOX_HEIGHT, STATUS_LINE_HEIGHT},
};

pub mod help_modal;
pub mod load_view;
pub mod search_box;
pub mod status_line;
pub mod tree_view;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if area.width < MIN_COLS || area.height < MIN_ROWS {
        render_too_small(frame, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SEARCH_BOX_HEIGHT),
            Constraint::Length(LOAD_VIEW_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(STATUS_LINE_HEIGHT),
        ])
        .split(area);

    search_box::render(frame, chunks[0], app);

    if app.latest.is_none() {
        // Bordered "loading" placeholder in the load pane until the first snapshot.
        let block = Block::bordered().title(" load ");
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);
        let p = Paragraph::new("loading…").alignment(Alignment::Center);
        frame.render_widget(p, inner);
    } else {
        load_view::render(frame, chunks[1], app);
    }

    tree_view::render(frame, chunks[2], app);
    status_line::render(frame, chunks[3], app);

    if app.help_open {
        help_modal::render(frame, area, app);
    }
}

fn render_too_small(frame: &mut Frame, area: Rect) {
    let msg = format!(
        "terminal too small ({}×{} < {}×{})",
        area.width, area.height, MIN_COLS, MIN_ROWS
    );
    let row = if area.height == 0 {
        return;
    } else {
        area.height / 2
    };
    let strip = Rect {
        x: area.x,
        y: area.y + row,
        width: area.width,
        height: 1,
    };
    let p = Paragraph::new(msg).alignment(Alignment::Center);
    frame.render_widget(p, strip);
}
