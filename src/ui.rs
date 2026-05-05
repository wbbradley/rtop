use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Paragraph},
};

use crate::{
    app::state::App,
    consts::{LOAD_VIEW_HEIGHT, SEARCH_BOX_HEIGHT},
};

pub mod load_view;
pub mod search_box;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SEARCH_BOX_HEIGHT),
            Constraint::Length(LOAD_VIEW_HEIGHT),
            Constraint::Min(0),
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

    // Tree placeholder — full implementation in Phase 3.
    let tree_block = Block::bordered().title(" tree ");
    frame.render_widget(tree_block, chunks[2]);
}
