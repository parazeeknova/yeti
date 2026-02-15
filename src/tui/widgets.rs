use crate::prompt::FileInfo;
use crate::tui::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub struct FileList<'a> {
    files: &'a [FileInfo],
    total_additions: usize,
    total_deletions: usize,
    theme: &'a Theme,
}

impl<'a> FileList<'a> {
    pub fn new(
        files: &'a [FileInfo],
        total_additions: usize,
        total_deletions: usize,
        theme: &'a Theme,
    ) -> Self {
        Self {
            files,
            total_additions,
            total_deletions,
            theme,
        }
    }

    pub fn render(self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .files
            .iter()
            .map(|file| {
                let status_icon = file.status.as_str();
                let status_style = match file.status {
                    crate::prompt::FileStatus::Added => self.theme.added_style(),
                    crate::prompt::FileStatus::Deleted => self.theme.deleted_style(),
                    crate::prompt::FileStatus::Modified | crate::prompt::FileStatus::Renamed => {
                        Style::default().fg(ratatui::style::Color::Yellow)
                    }
                };

                let additions = if file.additions > 0 {
                    format!("+{}", file.additions)
                } else {
                    String::new()
                };
                let deletions = if file.deletions > 0 {
                    format!("-{}", file.deletions)
                } else {
                    String::new()
                };

                let line = Line::from(vec![
                    Span::styled(format!("{} ", status_icon), status_style),
                    Span::styled(&file.path, self.theme.normal_style()),
                    Span::raw(" "),
                    Span::styled(additions, self.theme.added_style()),
                    Span::raw(" "),
                    Span::styled(deletions, self.theme.deleted_style()),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = format!(
            "STAGED FILES ({} files)    +{} -{}",
            self.files.len(),
            self.total_additions,
            self.total_deletions
        );

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(title, self.theme.title_style()))
                    .border_style(Style::default().fg(self.theme.border)),
            )
            .highlight_style(Style::default().add_modifier(ratatui::style::Modifier::REVERSED));

        f.render_widget(list, area);
    }
}

pub struct ErrorPopup<'a> {
    title: &'a str,
    message: &'a str,
    theme: &'a Theme,
}

impl<'a> ErrorPopup<'a> {
    pub fn new(title: &'a str, message: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            message,
            theme,
        }
    }

    pub fn render(self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 40, area);

        f.render_widget(Clear, popup_area);

        let text = vec![
            Line::from(""),
            Line::from(Span::styled(self.message, self.theme.error_style())),
            Line::from(""),
            Line::from(Span::styled(
                "[R] Retry    [K] Update Key    [Q] Quit",
                self.theme.dim_style(),
            )),
        ];

        let paragraph = Paragraph::new(text).block(
            Block::default()
                .title(Span::styled(
                    format!(" {} ", self.title),
                    self.theme.error_style(),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.error)),
        );

        f.render_widget(paragraph, popup_area);
    }
}

pub struct KeyInputPopup<'a> {
    input: &'a str,
    theme: &'a Theme,
    cursor_pos: usize,
    error: Option<&'a str>,
}

impl<'a> KeyInputPopup<'a> {
    pub fn new(
        input: &'a str,
        theme: &'a Theme,
        cursor_pos: usize,
        error: Option<&'a str>,
    ) -> Self {
        Self {
            input,
            theme,
            cursor_pos,
            error,
        }
    }

    pub fn render(self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(70, 50, area);

        f.render_widget(Clear, popup_area);

        let mut text = vec![
            Line::from(""),
            Line::from("Enter your Cerebras API key:"),
            Line::from(""),
        ];

        let masked: String = if self.input.is_empty() {
            "█".to_string()
        } else {
            let stars: String = "*".repeat(self.input.len());
            let cursor_offset = self.cursor_pos.min(self.input.len());
            if cursor_offset < self.input.len() {
                format!("{}█{}", &stars[..cursor_offset], &stars[cursor_offset..])
            } else {
                format!("{}█", stars)
            }
        };

        text.push(Line::from(Span::styled(
            format!("  {}", masked),
            self.theme.normal_style(),
        )));
        text.push(Line::from(""));

        if let Some(err) = self.error {
            text.push(Line::from(Span::styled(
                format!("Error: {}", err),
                self.theme.error_style(),
            )));
            text.push(Line::from(""));
        }

        text.push(Line::from(Span::styled(
            "Get your key at: cloud.cerebras.ai",
            self.theme.dim_style(),
        )));
        text.push(Line::from(""));
        text.push(Line::from(Span::styled(
            "[Enter] Save & Continue    [Esc] Cancel",
            self.theme.dim_style(),
        )));

        let paragraph = Paragraph::new(text).block(
            Block::default()
                .title(Span::styled(" YETI - Setup ", self.theme.title_style()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.border_focused)),
        );

        f.render_widget(paragraph, popup_area);
    }
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
