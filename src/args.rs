use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "yeti",
    version,
    about = "domesticate your diff",
    long_about = "A beast that camps between your working directory and Git, sniffing through messy diffs and leaving behind clean, intentional history."
)]
pub struct Args {
    #[arg(long, help = "Sniff around without leaving tracks (preview only)")]
    pub dry_run: bool,

    #[arg(long, help = "Reset your scent (force API key re-entry)")]
    pub reset_key: bool,
}
