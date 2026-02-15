use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use git2::{IndexAddOption, Repository, StatusOptions};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const OLLAMA_SYSTEM_PROMPT: &str = "You are Yeet, a local git assistant. Generate only a valid conventional commit message with a concise title and a short explanatory body.";

#[derive(Parser, Debug)]
#[command(
    name = "yeet",
    version,
    about = "Generate intentional commits with local Ollama"
)]
struct Args {
    #[arg(long, help = "Deprecated: commits are automatic unless --dry-run")]
    yes: bool,
    #[arg(long, help = "Preview only, do not commit")]
    dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AppConfig {
    default_model: Option<String>,
}

#[derive(Debug)]
struct GeneratedMessage {
    title: String,
    body: String,
}

#[derive(Debug)]
struct ChangeSummary {
    branch: String,
    files: Vec<String>,
}

enum Ui {
    Tui(Tui),
    Plain,
}

impl Ui {
    fn new() -> Result<Self> {
        if io::stdout().is_terminal() {
            Ok(Self::Tui(Tui::new()?))
        } else {
            Ok(Self::Plain)
        }
    }

    fn status(&mut self, text: &str) -> Result<()> {
        match self {
            Ui::Tui(tui) => tui.draw_status(text),
            Ui::Plain => {
                println!("[yeet] {text}");
                Ok(())
            }
        }
    }

    fn pick_model(&mut self, models: &[String], default: Option<&str>) -> Result<String> {
        match self {
            Ui::Tui(tui) => tui.pick_model(models, default),
            Ui::Plain => pick_model_plain(models, default),
        }
    }

    fn confirm(&mut self, question: &str, default_yes: bool) -> Result<bool> {
        match self {
            Ui::Tui(tui) => tui.confirm(question, default_yes),
            Ui::Plain => confirm_plain(question, default_yes),
        }
    }

    fn leave_tui(&mut self) {
        if matches!(self, Ui::Tui(_)) {
            *self = Ui::Plain;
        }
    }
}

struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl Tui {
    fn new() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).context("failed to create terminal")?;
        Ok(Self { terminal })
    }

    fn draw_status(&mut self, text: &str) -> Result<()> {
        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(f.area());
            let p = Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Yeet Progress"),
            );
            f.render_widget(p, chunks[0]);
        })?;
        Ok(())
    }

    fn confirm(&mut self, question: &str, default_yes: bool) -> Result<bool> {
        loop {
            let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
            let prompt = format!("{question} {suffix} (y/n, Enter for default)");
            self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(1)])
                    .split(f.area());
                let p = Paragraph::new(prompt.as_str()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Yeet Confirmation"),
                );
                f.render_widget(p, chunks[0]);
            })?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                    KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
                    KeyCode::Enter => return Ok(default_yes),
                    KeyCode::Esc => return Ok(false),
                    _ => {}
                }
            }
        }
    }

    fn pick_model(&mut self, models: &[String], default: Option<&str>) -> Result<String> {
        if models.is_empty() {
            bail!("no ollama models found");
        }
        let mut state = ListState::default();
        let mut selected = default
            .and_then(|d| models.iter().position(|m| m == d))
            .unwrap_or(0);
        state.select(Some(selected));

        loop {
            self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(5)])
                    .split(f.area());

                let header = Paragraph::new("Select Ollama model (↑/↓, Enter)")
                    .block(Block::default().borders(Borders::ALL).title("Yeet"));
                f.render_widget(header, chunks[0]);

                let items = models
                    .iter()
                    .map(|m| ListItem::new(m.clone()))
                    .collect::<Vec<_>>();
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Available Models"),
                    )
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                    .highlight_symbol("> ");
                f.render_stateful_widget(list, chunks[1], &mut state);
            })?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up => {
                        if selected == 0 {
                            selected = models.len() - 1;
                        } else {
                            selected -= 1;
                        }
                        state.select(Some(selected));
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % models.len();
                        state.select(Some(selected));
                    }
                    KeyCode::Enter => return Ok(models[selected].clone()),
                    KeyCode::Esc | KeyCode::Char('q') => bail!("model selection canceled"),
                    _ => {}
                }
            }
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

fn main() {
    if let Err(err) = run() {
        eprintln!("yeet error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let mut ui = Ui::new()?;

    ui.status("running preflight checks")?;
    ensure_command("git")?;
    ensure_command("ollama")?;
    let repo = Repository::discover(".").context("not inside a git repository")?;

    if !is_ollama_running() {
        ui.status("ollama not running, starting local service")?;
        start_ollama_service()?;
        wait_for_ollama(Duration::from_secs(15))?;
    }

    ui.status("discovering local ollama models")?;
    let models = list_ollama_models()?;
    if models.is_empty() {
        bail!("no local ollama models found. run: ollama pull <model>");
    }

    let mut config = load_config()?;
    let selected = if models.len() == 1 {
        models[0].clone()
    } else {
        match config.default_model.as_ref() {
            Some(m) if models.contains(m) => m.clone(),
            _ => ui.pick_model(&models, config.default_model.as_deref())?,
        }
    };

    if !matches!(config.default_model.as_deref(), Some(m) if m == selected)
        && ui.confirm("save selected model as default?", false)?
    {
        config.default_model = Some(selected.clone());
        save_config(&config)?;
    }

    ui.status("staging all repository changes")?;
    stage_all(&repo)?;

    let summary = summarize_staged_changes(&repo)?;
    if summary.files.is_empty() {
        bail!("no staged changes to commit");
    }

    ui.status("generating commit message with ollama")?;
    let generated = generate_commit_message(&selected, &summary, |line| {
        ui.status(&format!("generating commit message: {line}"))
    })?;
    ui.status(&format!("generated commit title: {}", generated.title))?;

    ui.leave_tui();

    println!(
        "\nProposed commit message:
"
    );
    println!("{}", generated.title);
    if !generated.body.is_empty() {
        println!("\n{}", generated.body);
    }
    println!("\nChanged files staged: {}", summary.files.len());

    if args.dry_run {
        ui.status("dry-run complete; no commit created")?;
        return Ok(());
    }

    if !args.yes {
        println!(
            "
Auto-committing with generated message (use --dry-run to preview only).
"
        );
    }

    ui.status("creating commit (git may prompt for signing passphrase)")?;
    commit_with_git(&generated)?;
    ui.status("commit created successfully")?;
    Ok(())
}

fn ensure_command(name: &str) -> Result<()> {
    let status = Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => bail!("required command not found or not runnable: {name}"),
    }
}

fn is_ollama_running() -> bool {
    let addr: SocketAddr = "127.0.0.1:11434".parse().expect("valid socket addr");
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

fn start_ollama_service() -> Result<()> {
    Command::new("ollama")
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start ollama serve")?;
    Ok(())
}

fn wait_for_ollama(timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_ollama_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    bail!("ollama service did not become ready in time")
}

fn list_ollama_models() -> Result<Vec<String>> {
    let output = Command::new("ollama")
        .arg("ls")
        .output()
        .context("failed to run ollama ls")?;
    if !output.status.success() {
        bail!("ollama ls failed")
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    for line in stdout.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(name) = trimmed.split_whitespace().next() {
            models.push(name.to_string());
        }
    }
    models.sort();
    models.dedup();
    Ok(models)
}

fn stage_all(repo: &Repository) -> Result<()> {
    let mut index = repo.index().context("failed to open index")?;
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .context("failed to stage changes")?;
    index.write().context("failed to write index")?;
    Ok(())
}

fn summarize_staged_changes(repo: &Repository) -> Result<ChangeSummary> {
    let head_name = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()))
        .unwrap_or_else(|| "HEAD".to_string());

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false);

    let statuses = repo.statuses(Some(&mut opts))?;
    let files = statuses
        .iter()
        .filter_map(|e| e.path().map(ToString::to_string))
        .collect::<Vec<_>>();

    Ok(ChangeSummary {
        branch: head_name,
        files,
    })
}

fn generate_commit_message<F>(
    model: &str,
    summary: &ChangeSummary,
    mut on_progress: F,
) -> Result<GeneratedMessage>
where
    F: FnMut(&str) -> Result<()>,
{
    let file_list = summary
        .files
        .iter()
        .take(40)
        .map(|f| format!("- {f}"))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "System instructions:
{}

User task:
Generate a git commit message.
Output format:
Line 1: conventional commit title under 72 chars
Line 2+: short body in 2-4 lines.

Branch: {}
Changed files:
{}
",
        OLLAMA_SYSTEM_PROMPT, summary.branch, file_list
    );

    let mut child = Command::new("ollama")
        .arg("run")
        .arg(model)
        .arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to run ollama model")?;

    let mut stdout = child
        .stdout
        .take()
        .context("failed to capture ollama stdout")?;
    let mut full_output = String::new();
    let mut buf = [0u8; 512];

    loop {
        let read = stdout
            .read(&mut buf)
            .context("failed to read ollama output")?;
        if read == 0 {
            break;
        }
        let chunk = String::from_utf8_lossy(&buf[..read]);
        full_output.push_str(&chunk);

        let cleaned = full_output
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\r')
            .collect::<String>();
        let single_line_raw = cleaned
            .lines()
            .last()
            .unwrap_or("")
            .replace('\r', " ")
            .chars()
            .filter(|c| {
                c.is_ascii_alphanumeric()
                    || c.is_ascii_whitespace()
                    || matches!(*c, ':' | '-' | ',' | '.' | '(' | ')' | '/' | '_')
            })
            .collect::<String>();
        let single_line = single_line_raw
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(96)
            .collect::<String>();
        if !single_line.is_empty() {
            on_progress(single_line.as_str())?;
        }
    }

    let status = child.wait().context("failed waiting for ollama process")?;
    if !status.success() {
        bail!("ollama failed to generate commit message")
    }

    Ok(sanitize_message(&full_output, summary))
}

fn sanitize_message(raw: &str, summary: &ChangeSummary) -> GeneratedMessage {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>();

    let fallback = || {
        let scope = summary
            .files
            .first()
            .and_then(|f| f.split('/').next())
            .unwrap_or("repo");
        let title = format!("chore({scope}): update {} files", summary.files.len());
        let body = format!("Staged updates on branch {}.", summary.branch);
        GeneratedMessage { title, body }
    };

    if lines.is_empty() {
        return fallback();
    }

    let mut title = lines[0].to_string();
    if title.len() > 72 {
        title.truncate(72);
    }
    if !title.contains(':') {
        let scope = summary
            .files
            .first()
            .and_then(|f| f.split('/').next())
            .unwrap_or("repo");
        title = format!("chore({scope}): {title}");
        if title.len() > 72 {
            title.truncate(72);
        }
    }

    let body = if lines.len() > 1 {
        lines[1..].join("\n")
    } else {
        format!("Updates staged files on branch {}.", summary.branch)
    };

    GeneratedMessage { title, body }
}

fn commit_with_git(msg: &GeneratedMessage) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("commit").arg("-m").arg(&msg.title);
    if !msg.body.trim().is_empty() {
        cmd.arg("-m").arg(&msg.body);
    }
    let status = cmd.status().context("failed to run git commit")?;
    if !status.success() {
        bail!("git commit failed")
    }
    Ok(())
}

fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("unable to locate config directory")?;
    Ok(base.join("yeet").join("config.toml"))
}

fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(toml::from_str(&text).unwrap_or_default())
}

fn save_config(config: &AppConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let text = toml::to_string(config).context("failed to serialize config")?;
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn pick_model_plain(models: &[String], default: Option<&str>) -> Result<String> {
    println!("Available Ollama models:");
    for (i, model) in models.iter().enumerate() {
        if Some(model.as_str()) == default {
            println!("  {}. {} (default)", i + 1, model);
        } else {
            println!("  {}. {}", i + 1, model);
        }
    }
    print!("Select model number: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let idx = input
        .trim()
        .parse::<usize>()
        .context("invalid model selection")?;
    if idx == 0 || idx > models.len() {
        bail!("selected model index out of range")
    }
    Ok(models[idx - 1].clone())
}

fn confirm_plain(question: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{question} {suffix}: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_lowercase();

    if normalized.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(normalized.as_str(), "y" | "yes"))
}
