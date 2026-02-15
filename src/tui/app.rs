use crate::args::Args;
use crate::cerebras;
use crate::config::{self, Config};
use crate::error::Result;
use crate::git::{GitRepo, StagedSummary};
use crate::prompt::{self, FileInfo};
use crate::tui::{Theme, Tui};
use crossterm::event::{Event, KeyCode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

#[derive(Debug, Clone)]
pub enum AppState {
    Init,
    ApiKeyInput {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    Generating {
        branch: String,
        files: Vec<FileInfo>,
        total_additions: usize,
        total_deletions: usize,
        generated: String,
    },
    Done {
        branch: String,
        files: Vec<FileInfo>,
        total_additions: usize,
        total_deletions: usize,
        message: String,
        done_at: Instant,
    },
    Error {
        message: String,
        retryable: bool,
    },
    Exit,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    ApiKeyEntered(String),
    ApiKeyValidated,
    ApiKeyValidationFailed(String),
    StagingComplete(StagedSummary),
    StagingFailed(String),
    GenerationChunk(String),
    GenerationComplete(String),
    GenerationFailed(String),
    CommitComplete,
    CommitFailed(String),
}

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub code: KeyCode,
}

impl From<crossterm::event::KeyEvent> for KeyEvent {
    fn from(key: crossterm::event::KeyEvent) -> Self {
        KeyEvent { code: key.code }
    }
}

pub struct App {
    state: AppState,
    config: Config,
    api_key: Option<String>,
    args: Args,
    theme: Theme,
    event_rx: Receiver<AppEvent>,
    event_tx: Sender<AppEvent>,
    dry_run: bool,
}

impl App {
    pub fn new(args: Args) -> Result<Self> {
        let config = config::load()?;
        let api_key = config::get_effective_api_key(&config);

        let (event_tx, event_rx) = mpsc::channel();

        let state = if args.reset_key {
            AppState::ApiKeyInput {
                input: String::new(),
                cursor: 0,
                error: None,
            }
        } else if api_key.is_some() {
            AppState::Init
        } else {
            AppState::ApiKeyInput {
                input: String::new(),
                cursor: 0,
                error: None,
            }
        };

        Ok(Self {
            state,
            config,
            api_key,
            args: args.clone(),
            theme: Theme::default(),
            event_rx,
            event_tx,
            dry_run: args.dry_run,
        })
    }

    pub fn run(&mut self, tui: &mut Tui) -> Result<()> {
        self.start_init();

        loop {
            if matches!(self.state, AppState::Exit) {
                break;
            }

            if let AppState::Done { done_at, .. } = &self.state
                && done_at.elapsed().as_secs() >= 2
            {
                break;
            }

            if let Some(event) = tui.poll_event(50) {
                match event {
                    Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                        self.handle_key(KeyEvent::from(key));
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_event(event);
            }

            self.render(tui)?;
        }

        Ok(())
    }

    fn start_init(&mut self) {
        if matches!(self.state, AppState::Init) {
            let tx = self.event_tx.clone();
            thread::spawn(move || {
                if let Err(e) = Self::do_staging(&tx) {
                    let _ = tx.send(AppEvent::StagingFailed(e.to_string()));
                }
            });
        }
    }

    fn do_staging(tx: &Sender<AppEvent>) -> Result<()> {
        let repo = GitRepo::discover()?;
        repo.stage_all()?;

        let summary = repo.get_staged_summary()?;

        let _ = tx.send(AppEvent::StagingComplete(summary));
        Ok(())
    }

    fn start_generation(&mut self, summary: StagedSummary) {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                self.state = AppState::Error {
                    message: "No API key available".to_string(),
                    retryable: true,
                };
                return;
            }
        };

        let model = self.config.model().to_string();
        let branch = summary.branch.clone();
        let files = summary.files.clone();
        let total_additions = summary.total_additions;
        let total_deletions = summary.total_deletions;

        let user_prompt = prompt::build_user_prompt(&branch, &files);

        self.state = AppState::Generating {
            branch,
            files,
            total_additions,
            total_deletions,
            generated: String::new(),
        };

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let result =
                cerebras::generate_commit_message(&api_key, &model, &user_prompt, |chunk| {
                    let _ = tx.send(AppEvent::GenerationChunk(chunk.to_string()));
                });

            match result {
                Ok(msg) => {
                    let _ = tx.send(AppEvent::GenerationComplete(msg));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::GenerationFailed(e.to_string()));
                }
            }
        });
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match &mut self.state {
            AppState::ApiKeyInput {
                input,
                cursor,
                error,
            } => {
                error.take();
                match key.code {
                    KeyCode::Char(c) => {
                        input.insert(*cursor, c);
                        *cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            input.remove(*cursor - 1);
                            *cursor -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if *cursor < input.len() {
                            input.remove(*cursor);
                        }
                    }
                    KeyCode::Left => {
                        if *cursor > 0 {
                            *cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if *cursor < input.len() {
                            *cursor += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if !input.is_empty() {
                            let key_str = input.clone();
                            let _ = self.event_tx.send(AppEvent::ApiKeyEntered(key_str));
                        }
                    }
                    KeyCode::Esc => {
                        self.state = AppState::Exit;
                    }
                    _ => {}
                }
            }
            AppState::Error { retryable, .. } => match key.code {
                KeyCode::Char('r') | KeyCode::Char('R') if *retryable => {
                    self.state = AppState::Init;
                    self.start_init();
                }
                KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.state = AppState::ApiKeyInput {
                        input: String::new(),
                        cursor: 0,
                        error: None,
                    };
                }
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                    self.state = AppState::Exit;
                }
                _ => {}
            },
            AppState::Generating { .. } => {
                if key.code == KeyCode::Esc {
                    self.state = AppState::Exit;
                }
            }
            AppState::Done { .. } => {
                if key.code == KeyCode::Esc {
                    self.state = AppState::Exit;
                }
            }
            _ => {}
        }
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::ApiKeyEntered(key) => {
                self.api_key = Some(key.clone());
                self.state = AppState::ApiKeyInput {
                    input: key.clone(),
                    cursor: key.len(),
                    error: None,
                };

                let tx = self.event_tx.clone();
                thread::spawn(move || match cerebras::validate_api_key(&key) {
                    Ok(_) => {
                        let _ = tx.send(AppEvent::ApiKeyValidated);
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::ApiKeyValidationFailed(e.to_string()));
                    }
                });
            }
            AppEvent::ApiKeyValidated => {
                if let Some(ref key) = self.api_key {
                    let _ = config::save_api_key(key);
                }
                self.state = AppState::Init;
                self.start_init();
            }
            AppEvent::ApiKeyValidationFailed(err) => {
                self.state = AppState::ApiKeyInput {
                    input: String::new(),
                    cursor: 0,
                    error: Some(err),
                };
            }
            AppEvent::StagingComplete(summary) => {
                self.start_generation(summary);
            }
            AppEvent::StagingFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: false,
                };
            }
            AppEvent::GenerationChunk(chunk) => {
                if let AppState::Generating { generated, .. } = &mut self.state {
                    generated.push_str(&chunk);
                }
            }
            AppEvent::GenerationComplete(raw) => {
                let (title, body) = cerebras::parse_commit_message(&raw);

                if self.dry_run {
                    if let AppState::Generating {
                        branch,
                        files,
                        total_additions,
                        total_deletions,
                        generated: _,
                    } = &self.state
                    {
                        self.state = AppState::Done {
                            branch: branch.clone(),
                            files: files.clone(),
                            total_additions: *total_additions,
                            total_deletions: *total_deletions,
                            message: format!("{}\n\n{}", title, body.unwrap_or_default()),
                            done_at: Instant::now(),
                        };
                    }
                    return;
                }

                if let AppState::Generating {
                    branch,
                    files,
                    total_additions,
                    total_deletions,
                    ..
                } = &self.state
                {
                    let branch_clone = branch.clone();
                    let files_clone = files.clone();
                    let total_add = *total_additions;
                    let total_del = *total_deletions;
                    let title_clone = title.clone();
                    let body_clone = body.clone();
                    let tx = self.event_tx.clone();

                    thread::spawn(move || {
                        match crate::git::commit_with_git_cli(&title_clone, body_clone.as_deref()) {
                            Ok(_) => {
                                let _ = tx.send(AppEvent::CommitComplete);
                            }
                            Err(e) => {
                                let _ = tx.send(AppEvent::CommitFailed(e.to_string()));
                            }
                        }
                    });

                    self.state = AppState::Done {
                        branch: branch_clone,
                        files: files_clone,
                        total_additions: total_add,
                        total_deletions: total_del,
                        message: title,
                        done_at: Instant::now(),
                    };
                }
            }
            AppEvent::GenerationFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: true,
                };
            }
            AppEvent::CommitComplete => {
                // Already in Done state
            }
            AppEvent::CommitFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: false,
                };
            }
        }
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.terminal().draw(|f| {
            self.draw(f);
        })?;
        Ok(())
    }

    fn draw(&self, f: &mut Frame) {
        match &self.state {
            AppState::ApiKeyInput {
                input,
                cursor,
                error,
            } => {
                crate::tui::KeyInputPopup::new(input, &self.theme, *cursor, error.as_deref())
                    .render(f, f.area());
            }
            AppState::Init => {
                self.draw_loading(f);
            }
            AppState::Generating {
                branch,
                files,
                total_additions,
                total_deletions,
                generated,
            } => {
                self.draw_generating(
                    f,
                    branch,
                    files,
                    *total_additions,
                    *total_deletions,
                    generated,
                );
            }
            AppState::Done {
                branch,
                files,
                total_additions,
                total_deletions,
                message,
                done_at: _,
            } => {
                self.draw_done(
                    f,
                    branch,
                    files,
                    *total_additions,
                    *total_deletions,
                    message,
                );
            }
            AppState::Error { message, retryable } => {
                self.draw_error(f, message, *retryable);
            }
            AppState::Exit => {}
        }
    }

    fn draw_loading(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(f.area());

        let header = Paragraph::new("Initializing...").block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" YETI ", self.theme.title_style()))
                .border_style(ratatui::style::Style::default().fg(self.theme.border)),
        );

        f.render_widget(header, chunks[0]);
    }

    fn draw_generating(
        &self,
        f: &mut Frame,
        branch: &str,
        files: &[FileInfo],
        total_additions: usize,
        total_deletions: usize,
        generated: &str,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(8),
                Constraint::Min(5),
            ])
            .split(f.area());

        let header = Paragraph::new(Line::from(vec![
            Span::styled("Branch: ", self.theme.dim_style()),
            Span::styled(branch, self.theme.normal_style()),
            Span::raw("    "),
            Span::styled(
                format!("{} files staged", files.len()),
                self.theme.dim_style(),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" YETI ", self.theme.title_style()))
                .border_style(ratatui::style::Style::default().fg(self.theme.border)),
        );

        f.render_widget(header, chunks[0]);

        crate::tui::FileList::new(files, total_additions, total_deletions, &self.theme)
            .render(f, chunks[1]);

        let mut lines = vec![
            Line::from(Span::styled("GENERATING...", self.theme.title_style())),
            Line::from(""),
        ];

        for line in generated.lines().take(5) {
            lines.push(Line::from(Span::styled(line, self.theme.normal_style())));
        }

        if !generated.is_empty() && !generated.ends_with('\n') {
            lines.push(Line::from(Span::raw("█")));
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(ratatui::style::Style::default().fg(self.theme.border)),
        );

        f.render_widget(paragraph, chunks[2]);
    }

    fn draw_done(
        &self,
        f: &mut Frame,
        branch: &str,
        files: &[FileInfo],
        total_additions: usize,
        total_deletions: usize,
        message: &str,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(8),
                Constraint::Min(5),
            ])
            .split(f.area());

        let header = Paragraph::new(Line::from(vec![
            Span::styled("Branch: ", self.theme.dim_style()),
            Span::styled(branch, self.theme.normal_style()),
            Span::raw("    "),
            Span::styled(
                format!("{} files staged", files.len()),
                self.theme.dim_style(),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" YETI ", self.theme.title_style()))
                .title_bottom(Span::styled(" ✓ ", self.theme.success_style()))
                .border_style(ratatui::style::Style::default().fg(self.theme.success))
                .border_style(ratatui::style::Style::default().fg(self.theme.border)),
        );

        f.render_widget(header, chunks[0]);

        crate::tui::FileList::new(files, total_additions, total_deletions, &self.theme)
            .render(f, chunks[1]);

        let mut lines = vec![
            Line::from(Span::styled(
                if self.args.dry_run {
                    "DRY RUN - Not committed"
                } else {
                    "COMMITTED"
                },
                self.theme.success_style(),
            )),
            Line::from(""),
        ];

        for line in message.lines() {
            lines.push(Line::from(Span::styled(line, self.theme.normal_style())));
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(ratatui::style::Style::default().fg(self.theme.success)),
        );

        f.render_widget(paragraph, chunks[2]);
    }

    fn draw_error(&self, f: &mut Frame, message: &str, retryable: bool) {
        crate::tui::ErrorPopup::new(
            if retryable { "ERROR" } else { "FATAL ERROR" },
            message,
            &self.theme,
        )
        .render(f, f.area());
    }
}
