use crate::args::{MASCOT_LINES, MASCOT_MINI};
use crate::tui::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Padding, Paragraph, Wrap},
};

pub fn draw_key_input(
    f: &mut Frame,
    theme: &Theme,
    input: &str,
    cursor: usize,
    error: Option<&str>,
) {
    let area = centered_rect(66, 42, f.area());
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
            format!("{}  yeti setup", MASCOT_MINI),
            theme.accent_style(),
        )),
        Line::from(Span::styled(MASCOT_LINES[1], theme.dim_style())),
        Line::from(Span::styled(
            "No API key found. Add your Cerebras key to start generating commit messages.",
            theme.fg_style(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("key ", theme.dim_style()),
            Span::styled(masked, theme.fg_style()),
        ]),
    ];

    if let Some(e) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(e, theme.red_style())));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "cloud.cerebras.ai/account/api-keys",
        theme.dim_style(),
    )));
    lines.push(Line::from(Span::styled(
        "Enter save  ·  Esc cancel",
        theme.dim_style(),
    )));

    let para = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::bordered()
            .title(Span::styled(" first run ", theme.accent_style()))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent))
            .padding(Padding::new(1, 1, 0, 0)),
    );
    f.render_widget(para, area);
}

pub fn draw_error(f: &mut Frame, theme: &Theme, message: &str, retryable: bool) {
    let area = centered_rect(66, 38, f.area());
    f.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(Span::styled(
            format!("{}  yeti hit a snag", MASCOT_MINI),
            theme.red_style(),
        )),
        Line::from(Span::styled(MASCOT_LINES[7], theme.dim_style())),
        Line::from(""),
    ];
    lines.extend(
        message
            .lines()
            .map(|line| Line::from(Span::styled(line, theme.fg_style()))),
    );
    lines.push(Line::from(""));

    let hint = if retryable {
        "R retry  ·  K new key  ·  Q exit"
    } else {
        "Q exit"
    };
    lines.push(Line::from(Span::styled(hint, theme.dim_style())));

    let para = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::bordered()
            .title(Span::styled(" operation failed ", theme.red_style()))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.red))
            .padding(Padding::new(1, 1, 0, 0)),
    );
    f.render_widget(para, area);
}

pub fn draw_status_panel(
    f: &mut Frame,
    theme: &Theme,
    panel_title: &str,
    headline: &str,
    detail: &str,
    hint: &str,
) {
    let area = centered_rect(66, 34, f.area());
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .title(Span::styled(panel_title, theme.accent_style()))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            format!("{}  {}", MASCOT_MINI, headline),
            theme.accent_style(),
        )),
        Line::from(Span::styled(MASCOT_LINES[6], theme.dim_style())),
        Line::from(""),
        Line::from(Span::styled(detail, theme.fg_style())),
        Line::from(""),
        Line::from(Span::styled(hint, theme.dim_style())),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
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
