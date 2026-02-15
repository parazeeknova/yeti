use crate::prompt::FileInfo;
use crate::tui::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
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
        Line::from(Span::styled("API Key", theme.accent_style())),
        Line::from(""),
        Line::from(Span::styled(masked, theme.fg_style())),
    ];

    if let Some(e) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(e, theme.red_style())));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "cloud.cerebras.ai  ·  Enter to save  ·  Esc to cancel",
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
        Line::from(Span::styled("Error", theme.red_style())),
        Line::from(""),
        Line::from(Span::styled(message, theme.fg_style())),
        Line::from(""),
    ];

    let hint = if retryable {
        "R to retry  ·  K for new key  ·  Q to quit"
    } else {
        "Q to quit"
    };
    lines.push(Line::from(Span::styled(hint, theme.dim_style())));

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.red)),
    );
    f.render_widget(para, area);
}

pub fn draw_files(f: &mut Frame, theme: &Theme, files: &[FileInfo], area: Rect) {
    let mut lines = Vec::new();

    for file in files.iter().take(8) {
        let icon = file.status.as_str();
        let icon_style = match file.status {
            crate::prompt::FileStatus::Added => theme.green_style(),
            crate::prompt::FileStatus::Deleted => theme.red_style(),
            _ => theme.yellow_style(),
        };

        let add = if file.additions > 0 {
            format!("+{}", file.additions)
        } else {
            "".into()
        };
        let del = if file.deletions > 0 {
            format!("-{}", file.deletions)
        } else {
            "".into()
        };

        lines.push(Line::from(vec![
            Span::styled(icon, icon_style),
            Span::raw(" "),
            Span::styled(&file.path, theme.fg_style()),
            Span::raw(" "),
            Span::styled(add, theme.green_style()),
            Span::raw(" "),
            Span::styled(del, theme.red_style()),
        ]));
    }

    if files.len() > 8 {
        lines.push(Line::from(Span::styled(
            format!("  ... +{} more", files.len() - 8),
            theme.dim_style(),
        )));
    }

    let para = Paragraph::new(lines);
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
