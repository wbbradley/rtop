use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::{
    app::state::App,
    consts::{HELP_MODAL_HEIGHT, HELP_MODAL_WIDTH},
};

pub fn render(frame: &mut Frame, full_area: Rect, _app: &App) {
    let rect = centered_rect(full_area, HELP_MODAL_WIDTH, HELP_MODAL_HEIGHT);
    frame.render_widget(Clear, rect);

    let block = Block::bordered().title(" help ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let para = Paragraph::new(content());
    frame.render_widget(para, inner);
}

fn content() -> Vec<Line<'static>> {
    vec![
        section("[ search ]"),
        binding("Tab/S-Tab", "focus tree"),
        binding("Enter", "focus tree (first match)"),
        binding("Esc", "clear query"),
        binding("Ctrl-n/p", "move tree cursor"),
        binding(",", "OR groups"),
        binding("←/→ ^b/^f", "move by char"),
        binding("M-b/f ^←/→", "move by word"),
        binding("Home/End", "line start / end"),
        binding("^w", "delete word back"),
        binding("^k / ^u", "kill to end / start"),
        binding("?", "types literally (F1 = help)"),
        Line::from(""),
        section("[ tree ]"),
        binding("j/k gg G", "navigate"),
        binding("Ctrl-d/u", "half-page viewport"),
        binding("Enter", "drill (pid:<X>)"),
        binding("K", "signal modal"),
        binding("space", "pause sampling"),
        binding("/", "clear query, focus search"),
        binding("Esc/Tab", "focus search"),
        binding("?", "open this help"),
        section("[ any ]:  F1 help   Ctrl-C quit"),
    ]
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

fn binding(key: &str, action: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("   "),
        Span::styled(
            format!("{key:<12}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(action.to_string()),
    ])
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
