use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub green: Color,
    pub red: Color,
    pub yellow: Color,
}

impl Theme {
    pub fn gruvbox() -> Self {
        Self {
            fg: Color::Rgb(235, 219, 178),
            dim: Color::Rgb(146, 131, 116),
            accent: Color::Rgb(254, 128, 25),
            green: Color::Rgb(184, 187, 38),
            red: Color::Rgb(251, 73, 52),
            yellow: Color::Rgb(250, 189, 47),
        }
    }

    pub fn fg_style(&self) -> Style {
        Style::default().fg(self.fg)
    }

    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.dim)
    }

    pub fn accent_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn green_style(&self) -> Style {
        Style::default().fg(self.green)
    }

    pub fn red_style(&self) -> Style {
        Style::default().fg(self.red)
    }

    pub fn yellow_style(&self) -> Style {
        Style::default().fg(self.yellow)
    }
}
