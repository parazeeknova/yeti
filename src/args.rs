use clap::Parser;

pub fn print_help() {
    let orange = "\x1b[38;5;208m";
    let yellow = "\x1b[38;5;214m";
    let green = "\x1b[38;5;142m";
    let blue = "\x1b[38;5;109m";
    let dim = "\x1b[38;5;246m";
    let white = "\x1b[38;5;255m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    let o = orange;
    let b = bold;
    let r = reset;
    let w = white;
    let d = dim;

    // Face: all heavy box-drawing, no mixed weights
    // teeth sit directly between ┃ walls — no inner box needed
    //
    //  ┏━━━━━━━━━━━┓   title line 1
    //  ┃ ┌──┐ ┌──┐ ┃   title line 2
    //  ┃ │▓·│ │▓·│ ┃   title line 3
    //  ┃ └──┘ └──┘ ┃   title line 4
    //  ┃  ┌─┐ ┌─┐  ┃   title line 5
    //  ┃  └─┘ └─┘  ┃   title line 6
    //  ┣━━━━━━━━━━━┫   tagline     ← lip: flush heavy divider
    //  ┃ ▌▌▌▌▌▌▌▌▌ ┃               ← teeth between outer walls
    //  ┗━━━━━━━━━━━┛

    println!();
    println!("  {o}{b}┏━━━━━━━━━━━┓{r}  {o}{b}██╗   ██╗███████╗████████╗██╗{r}");
    println!("  {o}{b}┃ ┌──┐ ┌──┐ ┃{r}  {o}{b}╚██╗ ██╔╝██╔════╝╚══██╔══╝██║{r}");
    println!(
        "  {o}{b}┃ │{r}{w}▓·{r}{o}{b}│ │{r}{w}▓·{r}{o}{b}│ ┃{r}  {o}{b} ╚████╔╝ █████╗     ██║   ██║{r}"
    );
    println!("  {o}{b}┃ └──┘ └──┘ ┃{r}  {o}{b}  ╚██╔╝  ██╔══╝     ██║   ██║{r}");
    println!("  {o}{b}┃  ┌─┐ ┌─┐  ┃{r}  {o}{b}   ██║   ███████╗   ██║   ██║{r}");
    println!("  {o}{b}┃  └─┘ └─┘  ┃{r}  {o}{b}   ╚═╝   ╚══════╝   ╚═╝   ╚═╝{r}");
    println!("  {o}{b}┣━━━━━━━━━━━┫{r}  {d}domesticate your diff{r}");
    println!("  {o}{b}┃ ▌▌▌▌▌▌▌▌▌ ┃{r}  {d}AI-powered git commits{r}");
    println!("  {o}{b}┗━━━━━━━━━━━┛{r}");
    println!();

    // Usage
    println!("{b}  {o}USAGE{r}   {d}yeti{r} {b}[OPTIONS]{r}");
    println!();

    // Options — concise single-line each
    println!("{b}  {o}OPTIONS{r}");
    println!();
    println!(
        "  {g}{b}--dry-run{r}       {d}preview commit, no write{r}",
        g = green
    );
    println!(
        "  {y}{b}--reset-key{r}     {d}force API key re-entry{r}",
        y = yellow
    );
    println!(
        "  {y}{b}--reset-cache{r}   {d}wipe stored config{r}",
        y = yellow
    );
    println!(
        "  {b2}{b}-h, --help{r}      {d}show this screen{r}",
        b2 = blue
    );
    println!("  {b2}{b}-V, --version{r}   {d}print version{r}", b2 = blue);
    println!();

    // Footer
    println!("{d}  config → ~/.config/yeti/config.toml{r}");
    println!();
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "yeti",
    version,
    disable_help_flag = true,
    about = "domesticate your diff",
    long_about = "A beast that camps between your working directory and Git, sniffing through messy diffs and leaving behind clean, intentional history."
)]
pub struct Args {
    /// Show this help screen
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    pub help: bool,

    #[arg(long, help = "Sniff around without leaving tracks (preview only)")]
    pub dry_run: bool,

    #[arg(long, help = "Reset your scent (force API key re-entry)")]
    pub reset_key: bool,

    #[arg(
        long,
        help = "Clear local yeti cache/config (removes stored key and settings)"
    )]
    pub reset_cache: bool,
}
