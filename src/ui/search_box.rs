use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::{event::Focus, state::App};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().title(" search ");
    let inner = block.inner(area);

    let spans = highlight(&app.query_text);

    // Horizontal scroll: keep cursor (= end of string in Phase 2) visible.
    let inner_w = inner.width as usize;
    let total_len = app.query_text.chars().count();
    let scroll_x: u16 = if total_len + 1 > inner_w {
        ((total_len + 1).saturating_sub(inner_w)) as u16
    } else {
        0
    };

    let para = Paragraph::new(Line::from(spans))
        .block(block)
        .scroll((0, scroll_x));
    frame.render_widget(para, area);

    if app.focus == Focus::Search {
        let cursor_col_in_text = total_len.min(inner_w.saturating_sub(1));
        let x = inner.x + cursor_col_in_text as u16;
        let y = inner.y;
        frame.set_cursor_position((x, y));
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
