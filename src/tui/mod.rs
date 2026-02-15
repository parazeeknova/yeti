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
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
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

        let term_width = size().map(|(w, _)| w as usize).unwrap_or(80);
        let table_width = term_width.saturating_sub(4).max(40);

        let status_col = 10;
        let changes_col = 12;
        let file_col = table_width.saturating_sub(status_col + changes_col + 4);

        let hline = "─".repeat(table_width);
        let top_border = format!("┌{}┐", hline);
        let bot_border = format!("└{}┘", hline);
        let row_sep = format!("├{}┼{}┼{}┤",
            "─".repeat(status_col + 1),
            "─".repeat(file_col + 1),
            "─".repeat(changes_col + 1)
        );

        println!();

        println!(
            "  {}{}yeti{} {}{}{}",
            bold, orange, reset,
            blue, result.branch, reset
        );

        println!();

        println!("  {}{}{}", dim, top_border, reset);

        println!(
            "  {}│{} {:width$}{}│{} {:width$}{}│{} {:width$} {}│{}",
            dim, reset,
            &format!("{}status{}", bold, reset),
            dim, reset,
            &format!("{}file{}", bold, reset),
            dim, reset,
            &format!("{}+/-{}", bold, reset),
            dim, reset,
            width = (table_width - 3) / 3
        );

        println!("  {}{}{}", dim, row_sep, reset);

        for file in result.files.iter().take(10) {
            let (icon, icon_color) = match file.status {
                crate::prompt::FileStatus::Added => ("added", green),
                crate::prompt::FileStatus::Deleted => ("deleted", red),
                crate::prompt::FileStatus::Renamed => ("renamed", yellow),
                crate::prompt::FileStatus::Modified => ("modified", orange),
            };

            let path_display = if file.path.len() > file_col {
                format!("...{}", &file.path[file.path.len() - file_col + 3..])
            } else {
                file.path.clone()
            };

            let changes = format!("+{}/-{}", file.additions, file.deletions);

            println!(
                "  {}│{} {}{:<width$}{} {}│{} {:<file_w$} {}│{} {:^changes_w$} {}│{}",
                dim, reset,
                icon_color, icon, reset,
                dim, reset,
                path_display,
                dim, reset,
                changes,
                dim, reset,
                width = status_col - 1,
                file_w = file_col,
                changes_w = changes_col
            );
        }

        if result.files.len() > 10 {
            println!(
                "  {}│{} {:status_w$} {}│{} {:file_w$} {}│{} {:changes_w$} {}│{}",
                dim, reset,
                "", dim, reset,
                &format!("{}... {} more files{}", dim, result.files.len() - 10, reset),
                dim, reset,
                "", dim, reset,
                status_w = status_col,
                file_w = file_col,
                changes_w = changes_col
            );
        }

        println!("  {}{}{}", dim, row_sep, reset);

        let total_changes = format!("+{} -{}", total_add, total_del);

        println!(
            "  {}│{} {:status_w$} {}│{} {:file_w$} {}│{} {:changes_w$} {}│{}",
            dim, reset,
            &format!("{}total{}", bold, reset),
            dim, reset,
            format!("{} files", result.files.len()),
            dim, reset,
            total_changes,
            dim, reset,
            status_w = status_col,
            file_w = file_col,
            changes_w = changes_col
        );

        println!("  {}{}{}", dim, bot_border, reset);

        println!();

        let status = if result.dry_run {
            format!("{}scent marked{}", yellow, reset)
        } else {
            format!("{}territory marked{}", green, reset)
        };

        println!("  {}", status);

        println!();

        let msg_lines: Vec<&str> = result.message.lines().collect();
        let max_msg_len = msg_lines.iter().map(|l| l.len()).max().unwrap_or(0).min(table_width);

        let msg_box_width = max_msg_len.max(20);
        let msg_hline = "─".repeat(msg_box_width);
        let msg_top = format!("┌{}┐", msg_hline);
        let msg_bot = format!("└{}┘", msg_hline);

        println!("  {}{}{}", dim, msg_top, reset);

        let mut first = true;
        for line in msg_lines.iter() {
            if first {
                println!(
                    "  {}│{} {}{}{}{} {}│{}",
                    dim, reset,
                    bold, line, reset,
                    " ".repeat(msg_box_width.saturating_sub(line.len() + 1)),
                    dim, reset
                );
                first = false;
            } else if line.is_empty() {
                println!(
                    "  {}│{} {:width$} {}│{}",
                    dim, reset,
                    "",
                    dim, reset,
                    width = msg_box_width - 1
                );
            } else {
                println!(
                    "  {}│{} {:width$} {}│{}",
                    dim, reset,
                    line,
                    dim, reset,
                    width = msg_box_width.saturating_sub(line.len() + 1)
                );
            }
        }

        println!("  {}{}{}", dim, msg_bot, reset);

        println!();
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}