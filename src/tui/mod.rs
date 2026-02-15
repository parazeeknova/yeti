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

        println!();

        println!(
            "\x1b[38;5;180m{}\x1b[0m  \x1b[38;5;144m{} files\x1b[0m  \x1b[38;5;142m+{}\x1b[0m \x1b[38;5;167m-{}\x1b[0m",
            result.branch,
            result.files.len(),
            total_add,
            total_del
        );

        for file in result.files.iter().take(10) {
            let icon = file.status.as_str();
            let icon_color = match file.status {
                crate::prompt::FileStatus::Added => "142",
                crate::prompt::FileStatus::Deleted => "167",
                _ => "214",
            };

            let add_s = if file.additions > 0 { format!("\x1b[38;5;142m+{}\x1b[0m", file.additions) } else { String::new() };
            let del_s = if file.deletions > 0 { format!("\x1b[38;5;167m-{}\x1b[0m", file.deletions) } else { String::new() };

            println!(
                "  \x1b[38;5;{}m{}\x1b[0m {} {} {}",
                icon_color, icon, file.path, add_s, del_s
            );
        }

        if result.files.len() > 10 {
            println!("  \x1b[38;5;246m... {} more\x1b[0m", result.files.len() - 10);
        }

        println!();

        if result.dry_run {
            println!("\x1b[38;5;214mdry run\x1b[0m");
        } else {
            println!("\x1b[38;5;142mcommitted\x1b[0m");
        }

        println!();
        for line in result.message.lines() {
            println!("  {}", line);
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
