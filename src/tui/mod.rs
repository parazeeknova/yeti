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
            "  {}{}yeti{} {}{}{}",
            bold, orange, reset, blue, result.branch, reset
        );

        println!();

        let status_w = 8;

        let max_file_len = result
            .files
            .iter()
            .take(10)
            .map(|f| f.path.len().min(50))
            .max()
            .unwrap_or(10)
            .max(10);

        let changes_w = result
            .files
            .iter()
            .take(10)
            .map(|f| format!("+{}/-{}", f.additions, f.deletions).len())
            .max()
            .unwrap_or(8)
            .max(8);

        let total_changes = format!("+{} -{}", total_add, total_del);
        let total_changes_w = total_changes.len().max(changes_w);

        let table_w = status_w + max_file_len + total_changes_w + 4;

        println!("  {}┌{}┐{}", dim, "─".repeat(table_w), reset);

        println!(
            "  {}│{} {:sw$} {}│{} {:fw$} {}│{} {:cw$} {}│{}",
            dim,
            reset,
            "status",
            dim,
            reset,
            "file",
            dim,
            reset,
            "+/-",
            dim,
            reset,
            sw = status_w,
            fw = max_file_len,
            cw = total_changes_w
        );

        println!(
            "  {}├{}┼{}┼{}┤{}",
            dim,
            "─".repeat(status_w + 1),
            "─".repeat(max_file_len + 1),
            "─".repeat(total_changes_w + 1),
            reset
        );

        for file in result.files.iter().take(10) {
            let (icon, icon_color) = match file.status {
                crate::prompt::FileStatus::Added => ("added", green),
                crate::prompt::FileStatus::Deleted => ("deleted", red),
                crate::prompt::FileStatus::Renamed => ("renamed", yellow),
                crate::prompt::FileStatus::Modified => ("modified", orange),
            };

            let path_display = if file.path.len() > max_file_len {
                format!("...{}", &file.path[file.path.len() - max_file_len + 3..])
            } else {
                file.path.clone()
            };

            let changes = format!("+{}/-{}", file.additions, file.deletions);

            println!(
                "  {}│{} {}{:<sw$}{} {}│{} {:<fw$} {}│{} {:>cw$} {}│{}",
                dim,
                reset,
                icon_color,
                icon,
                reset,
                dim,
                reset,
                path_display,
                dim,
                reset,
                changes,
                dim,
                reset,
                sw = status_w,
                fw = max_file_len,
                cw = total_changes_w
            );
        }

        if result.files.len() > 10 {
            let more = format!("... {} more files", result.files.len() - 10);
            println!(
                "  {}│{} {:sw$} {}│{} {:fw$} {}│{} {:cw$} {}│{}",
                dim,
                reset,
                "",
                dim,
                reset,
                more,
                dim,
                reset,
                "",
                dim,
                reset,
                sw = status_w,
                fw = max_file_len,
                cw = total_changes_w
            );
        }

        println!(
            "  {}├{}┼{}┼{}┤{}",
            dim,
            "─".repeat(status_w + 1),
            "─".repeat(max_file_len + 1),
            "─".repeat(total_changes_w + 1),
            reset
        );

        println!(
            "  {}│{} {:sw$} {}│{} {:fw$} {}│{} {:>cw$} {}│{}",
            dim,
            reset,
            "total",
            dim,
            reset,
            format!("{} files", result.files.len()),
            dim,
            reset,
            total_changes,
            dim,
            reset,
            sw = status_w,
            fw = max_file_len,
            cw = total_changes_w
        );

        println!("  {}└{}┘{}", dim, "─".repeat(table_w), reset);

        println!();

        let status = if result.dry_run {
            format!("{}scent marked{}", yellow, reset)
        } else {
            format!("{}territory marked{}", green, reset)
        };

        println!("  {}", status);

        println!();

        let msg_lines: Vec<&str> = result.message.lines().collect();
        let msg_inner_w = table_w.saturating_sub(1);

        println!("  {}┌{}┐{}", dim, "─".repeat(table_w), reset);

        let mut first = true;
        for line in msg_lines.iter() {
            let line_len = line.chars().count();
            let padding = msg_inner_w.saturating_sub(line_len);

            if first {
                println!(
                    "  {}│{} {}{}{}{}{}│{}",
                    dim, reset,
                    bold, line, reset,
                    " ".repeat(padding),
                    dim, reset
                );
                first = false;
            } else if line.is_empty() {
                let spaces = " ".repeat(msg_inner_w);
                println!(
                    "  {}│{} {}│{}{}",
                    dim, reset,
                    spaces,
                    dim, reset
                );
            } else {
                let spaces = " ".repeat(padding);
                println!(
                    "  {}│{} {}{}│{}{}",
                    dim, reset,
                    line, spaces,
                    dim, reset
                );
            }
        }

        println!("  {}└{}┘{}", dim, "─".repeat(table_w), reset);

        println!();
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}
