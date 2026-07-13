use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{event::Focus, state::App};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = crate::ui::focused_block(" search ", app.focus == Focus::Search);
    let inner = block.inner(area);

    let spans = highlight(app.query_str());

    // Horizontal scroll derived from the caret's display column (Unicode-width
    // correct). Reserve the last inner column for the caret so a caret at the
    // end of the text stays visible rather than sliding under the border.
    let text_w = (inner.width as usize).saturating_sub(1);
    let scroll = app.query_input.visual_scroll(text_w);

    let para = Paragraph::new(Line::from(spans))
        .block(block)
        .scroll((0, scroll as u16));
    frame.render_widget(para, area);

    if app.focus == Focus::Search {
        let x = inner.x + (app.query_input.visual_cursor().saturating_sub(scroll)) as u16;
        frame.set_cursor_position((x, inner.y));
    }
}

const PREFIXES: &[&str] = &["pid:", "ppid:", "user:", "name:", "cmd:", "state:"];

fn highlight(input: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if input.is_empty() {
        return spans;
    }
    let prefix_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let mut byte_pos = 0;
    while byte_pos < input.len() {
        // emit any leading whitespace verbatim
        let bytes = input.as_bytes();
        let ws_start = byte_pos;
        while byte_pos < input.len() && bytes[byte_pos].is_ascii_whitespace() {
            byte_pos += 1;
        }
        if byte_pos > ws_start {
            spans.push(Span::raw(input[ws_start..byte_pos].to_string()));
        }
        if byte_pos >= input.len() {
            break;
        }

        // consume one whitespace-delimited token
        let tok_start = byte_pos;
        while byte_pos < input.len() && !bytes[byte_pos].is_ascii_whitespace() {
            byte_pos += 1;
        }
        let token = &input[tok_start..byte_pos];

        // If token starts with a known prefix, highlight the prefix; rest plain.
        if let Some(&p) = PREFIXES.iter().find(|p| token.starts_with(*p)) {
            spans.push(Span::styled(p.to_string(), prefix_style));
            spans.push(Span::raw(token[p.len()..].to_string()));
        } else {
            spans.push(Span::raw(token.to_string()));
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Position};

    use super::*;
    use crate::{app::state::App, consts::SEARCH_BOX_HEIGHT};

    #[test]
    fn highlight_prefix_is_bold_cyan() {
        let spans = highlight("pid:1234");
        assert_eq!(spans[0].content, "pid:");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].content, "1234");
        assert_eq!(spans[1].style.fg, None);
    }

    /// Draw the search box into a `width`×`SEARCH_BOX_HEIGHT` buffer and return
    /// the reported cursor position (only meaningful while search-focused).
    fn render_cursor(app: &App, width: u16) -> Position {
        let mut terminal = Terminal::new(TestBackend::new(width, SEARCH_BOX_HEIGHT)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), app))
            .unwrap();
        terminal.get_cursor_position().unwrap()
    }

    #[test]
    fn cursor_tracks_caret_for_short_query() {
        // Caret at end of "pid:1234" (8 cols); inner starts at x=1, y=1.
        let app = App::new("pid:1234".to_string(), false);
        let pos = render_cursor(&app, 40);
        assert_eq!(pos, Position { x: 1 + 8, y: 1 });
    }

    #[test]
    fn cursor_stays_visible_when_query_overflows() {
        // A query far wider than the box scrolls so the caret sits on the last
        // usable inner column (never under the border) — Unicode-width correct.
        let app = App::new("a".repeat(100), false);
        let width = 40u16;
        let pos = render_cursor(&app, width);
        // inner spans x=1..=(width-2); the caret rides the final inner column.
        assert_eq!(pos.x, width - 2);
        assert_eq!(pos.y, 1);
    }
}
