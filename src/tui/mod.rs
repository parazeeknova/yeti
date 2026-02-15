mod app;
mod theme;
mod widgets;

pub use app::{App, AppResult};
pub use theme::Theme;
pub use widgets::{draw_error, draw_key_input};

use crate::error::Result;
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

        let dim = "\x1b[38;5;246m";
        let green = "\x1b[38;5;142m";
        let red = "\x1b[38;5;167m";
        let yellow = "\x1b[38;5;214m";
        let orange = "\x1b[38;5;208m";
        let blue = "\x1b[38;5;109m";
        let bold = "\x1b[1m";
        let reset = "\x1b[0m";

        println!();

        println!(
            "  {}{}{}{}  {}{} files{}  {}+{}{}  {}-{}{}",
            bold, blue, result.branch, reset,
            dim, result.files.len(), reset,
            green, total_add, reset,
            red, total_del, reset
        );

        println!("  {}{}{}", dim, "─".repeat(50), reset);

        let max_path_len = result.files.iter().map(|f| f.path.len()).max().unwrap_or(0).min(40);

        for file in result.files.iter().take(10) {
            let (icon, icon_color) = match file.status {
                crate::prompt::FileStatus::Added => ("A", green),
                crate::prompt::FileStatus::Deleted => ("D", red),
                crate::prompt::FileStatus::Renamed => ("R", yellow),
                crate::prompt::FileStatus::Modified => ("M", orange),
            };

            let path_display = if file.path.len() > 40 {
                format!("...{}", &file.path[file.path.len() - 37..])
            } else {
                format!("{:width$}", file.path, width = max_path_len)
            };

            let add_s = if file.additions > 0 {
                format!("{}+{}{}", green, file.additions, reset)
            } else {
                format!("{}   {}", dim, reset)
            };

            let del_s = if file.deletions > 0 {
                format!("{}-{}{}", red, file.deletions, reset)
            } else {
                String::new()
            };

            println!(
                "  {}{}{}  {}  {} {}",
                icon_color, icon, reset,
                path_display,
                add_s, del_s
            );
        }

        if result.files.len() > 10 {
            println!(
                "  {}  ... {} more{}",
                dim, result.files.len() - 10, reset
            );
        }

        println!();

        let status = if result.dry_run {
            format!("{}dry run{}", yellow, reset)
        } else {
            format!("{}committed{}", green, reset)
        };

        println!("  {}", status);
        println!();

        let mut first = true;
        for line in result.message.lines() {
            if first {
                println!("  {}{}{}", bold, line, reset);
                first = false;
            } else if line.is_empty() {
                println!();
            } else {
                println!("  {}", line);
            }
        }

        println!();
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}