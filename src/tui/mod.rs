mod app;
mod theme;
mod widgets;

pub use app::App;
pub use theme::Theme;
pub use widgets::{ErrorPopup, FileList, KeyInputPopup};

use crate::error::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout};

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
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
