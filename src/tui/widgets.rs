use crate::tui::Theme;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn draw_key_input(
    f: &mut Frame,
    theme: &Theme,
    input: &str,
    cursor: usize,
    error: Option<&str>,
) {
    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);

    let masked = if input.is_empty() {
        "_".into()
    } else {
        let stars: String = "*".repeat(input.len());
        let c = cursor.min(input.len());
        if c < input.len() {
            format!("{}_", &stars[..c])
        } else {
            format!("{}_", stars)
        }
    };

    let mut lines = vec![
        Line::from(Span::styled(
            "yeti  mark your territory",
            theme.accent_style(),
        )),
        Line::from(""),
        Line::from(Span::styled(masked, theme.fg_style())),
    ];

    if let Some(e) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(e, theme.red_style())));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "cloud.cerebras.ai  ·  Enter to save  ·  Esc to retreat",
        theme.dim_style(),
    )));

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.dim)),
    );
    f.render_widget(para, area);
}

pub fn draw_error(f: &mut Frame, theme: &Theme, message: &str, retryable: bool) {
    let area = centered_rect(50, 25, f.area());
    f.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(Span::styled("yeti  lost the scent", theme.red_style())),
        Line::from(""),
        Line::from(Span::styled(message, theme.fg_style())),
        Line::from(""),
    ];

    let hint = if retryable {
        "R to track again  ·  K for new key  ·  Q to retreat"
    } else {
        "Q to retreat"
    };
    lines.push(Line::from(Span::styled(hint, theme.dim_style())));

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.red)),
    );
    f.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
            ratatui::layout::Constraint::Percentage(percent_y),
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let popup_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
            ratatui::layout::Constraint::Percentage(percent_x),
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);

    popup_layout[1]
}
