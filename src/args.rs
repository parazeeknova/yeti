use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "yeti", version, about = "Fast git commits with Cerebras AI")]
pub struct Args {
    #[arg(long, help = "Preview only, do not commit")]
    pub dry_run: bool,

    #[arg(long, help = "Force API key re-entry")]
    pub reset_key: bool,
}
