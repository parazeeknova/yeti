mod app;
mod theme;
mod widgets;

pub use app::{App, AppResult};
pub use theme::Theme;
pub use widgets::{draw_error, draw_key_input};

use crate::error::Result;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout, Write};

pub struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Tui {
    pub fn new() -> Result<Self> {
        enable_raw_mode().map_err(|e| crate::error::YetiError::IoError(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)
            .map_err(|e| crate::error::YetiError::IoError(e.to_string()))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal =
            Terminal::new(backend).map_err(|e| crate::error::YetiError::IoError(e.to_string()))?;
        Ok(Self { terminal })
    }

    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    pub fn poll_event(&self, timeout_ms: u16) -> Option<Event> {
        if event::poll(std::time::Duration::from_millis(timeout_ms as u64)).ok()? {
            event::read().ok()
        } else {
            None
        }
    }

    pub fn leave_and_print_history(result: &AppResult) {
        let mut stdout = io::stdout();
        let _ = disable_raw_mode();
        let _ = execute!(stdout, LeaveAlternateScreen);
        let _ = stdout.flush();

        let total_add: usize = result.files.iter().map(|f| f.additions).sum();
        let total_del: usize = result.files.iter().map(|f| f.deletions).sum();

        let orange = Color::AnsiValue(208);
        let green = Color::AnsiValue(142);
        let red = Color::AnsiValue(167);
        let yellow = Color::AnsiValue(214);
        let dim = Color::AnsiValue(246);

        println!();
        println!("  \x1b[1m\x1b[38;5;208myeti\x1b[0m \x1b[38;5;109m{}\x1b[0m", result.branch);
        println!();

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("status").fg(dim).add_attribute(Attribute::Bold),
            Cell::new("file").fg(dim).add_attribute(Attribute::Bold),
            Cell::new("+/-").fg(dim).add_attribute(Attribute::Bold),
        ]);

        for file in result.files.iter().take(10) {
            let (status_text, status_color) = match file.status {
                crate::prompt::FileStatus::Added => ("added", green),
                crate::prompt::FileStatus::Deleted => ("deleted", red),
                crate::prompt::FileStatus::Renamed => ("renamed", yellow),
                crate::prompt::FileStatus::Modified => ("modified", orange),
            };

            let path_display = if file.path.len() > 50 {
                format!("...{}", &file.path[file.path.len() - 47..])
            } else {
                file.path.clone()
            };

            table.add_row(vec![
                Cell::new(status_text).fg(status_color),
                Cell::new(path_display),
                Cell::new(format!("\x1b[38;5;142m+{}\x1b[0m/\x1b[38;5;167m-{}\x1b[0m", file.additions, file.deletions)),
            ]);
        }

        if result.files.len() > 10 {
            table.add_row(vec![
                Cell::new(""),
                Cell::new(format!("... {} more files", result.files.len() - 10)).fg(dim),
                Cell::new(""),
            ]);
        }

        table.add_row(vec![
            Cell::new("total").add_attribute(Attribute::Bold),
            Cell::new(format!("{} files", result.files.len())).add_attribute(Attribute::Bold),
            Cell::new(format!("\x1b[38;5;142m+{}\x1b[0m \x1b[38;5;167m-{}\x1b[0m", total_add, total_del)).add_attribute(Attribute::Bold),
        ]);

        let table_str = format!("{table}");
        let table_width = table_str.lines().next().map(|l| l.chars().count()).unwrap_or(60);

        println!("{table_str}");

        println!();

        let status = if result.dry_run {
            "\x1b[38;5;214mscent marked\x1b[0m"
        } else {
            "\x1b[38;5;142mterritory marked\x1b[0m"
        };

        println!("  {}", status);

        println!();

        let inner_w = table_width.saturating_sub(2);
        let dim_code = "\x1b[38;5;246m";
        let reset = "\x1b[0m";
        let bold_code = "\x1b[1m";

        println!("  {}┌{}┐{}", dim_code, "─".repeat(inner_w), reset);

        for (i, line) in result.message.lines().enumerate() {
            let line_len = line.chars().count();
            let padding = inner_w.saturating_sub(line_len);
            if i == 0 {
                println!("  {}│{} {}{}{}{}│{}", dim_code, reset, bold_code, line, reset, " ".repeat(padding), dim_code);
            } else {
                println!("  {}│{} {}{}│{}", dim_code, reset, line, " ".repeat(padding), dim_code);
            }
        }

        println!("  {}└{}┘{}", dim_code, "─".repeat(inner_w), reset);

        println!();
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}
