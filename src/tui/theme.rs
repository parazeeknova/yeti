use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub primary: Color,
    pub success: Color,
    pub error: Color,
    pub text: Color,
    pub text_dim: Color,
    pub border: Color,
    pub border_focused: Color,
}

impl Theme {
    pub fn default() -> Self {
        Self {
            primary: Color::Cyan,
            success: Color::Green,
            error: Color::Red,
            text: Color::White,
            text_dim: Color::Gray,
            border: Color::DarkGray,
            border_focused: Color::Cyan,
        }
    }

    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.primary)
            .add_modifier(Modifier::BOLD)
    }

    pub fn success_style(&self) -> Style {
        Style::default()
            .fg(self.success)
            .add_modifier(Modifier::BOLD)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error).add_modifier(Modifier::BOLD)
    }

    pub fn added_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn deleted_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.text_dim)
    }

    pub fn normal_style(&self) -> Style {
        Style::default().fg(self.text)
    }
}
