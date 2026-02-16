mod args;
mod cerebras;
mod config;
mod error;
mod git;
mod prompt;
mod tui;

use args::Args;
use clap::Parser;
use error::Result;
use tui::{App, Tui};

fn main() {
    if let Err(err) = run() {
        eprintln!("yeti error: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Args = Args::parse();
    let mut tui: Tui = Tui::new()?;
    let mut app: App = App::new(args)?;
    app.run(&mut tui)?;

    if let Some(result) = app.get_result() {
        Tui::leave_and_print_history(result);
    }

    Ok(())
}
