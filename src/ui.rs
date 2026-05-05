use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::app::state::App;

pub mod load_view;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    match app.latest.as_deref() {
        None => {
            let p = Paragraph::new("loading…").alignment(Alignment::Center);
            frame.render_widget(p, area);
        }
        Some(snap) => {
            load_view::render(frame, area, snap);
        }
    }
}
