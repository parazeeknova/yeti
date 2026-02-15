use crate::args::Args;
use crate::cerebras;
use crate::config::{self, Config};
use crate::error::Result;
use crate::git::{GitRepo, StagedSummary};
use crate::prompt::{self, FileInfo};
use crate::tui::{Theme, Tui, draw_error, draw_files, draw_key_input};
use crossterm::event::{Event, KeyCode};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
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
        generated: String,
    },
    Done {
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

pub struct App {
    state: AppState,
    config: Config,
    api_key: Option<String>,
    dry_run: bool,
    theme: Theme,
    event_rx: Receiver<AppEvent>,
    event_tx: Sender<AppEvent>,
}

impl App {
    pub fn new(args: Args) -> Result<Self> {
        let config = config::load()?;
        let api_key = config::get_effective_api_key(&config);
        let (event_tx, event_rx) = mpsc::channel();

        let state = if args.reset_key || api_key.is_none() {
            AppState::ApiKeyInput {
                input: String::new(),
                cursor: 0,
                error: None,
            }
        } else {
            AppState::Init
        };

        Ok(Self {
            state,
            config,
            api_key,
            dry_run: args.dry_run,
            theme: Theme::gruvbox(),
            event_rx,
            event_tx,
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

            if let Some(event) = tui.poll_event(50)
                && let Event::Key(key) = event
                && key.kind == crossterm::event::KeyEventKind::Press
            {
                self.handle_key(key.code);
            }

            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_event(event);
            }

            tui.terminal().draw(|f| self.draw(f))?;
        }

        Ok(())
    }

    fn start_init(&mut self) {
        if matches!(self.state, AppState::Init) {
            let tx = self.event_tx.clone();
            thread::spawn(move || {
                if let Err(e) = (|| {
                    let repo = GitRepo::discover()?;
                    repo.stage_all()?;
                    let summary = repo.get_staged_summary()?;
                    let _ = tx.send(AppEvent::StagingComplete(summary));
                    Ok::<_, crate::error::YetiError>(())
                })() {
                    let _ = tx.send(AppEvent::StagingFailed(e.to_string()));
                }
            });
        }
    }

    fn start_generation(&mut self, summary: StagedSummary) {
        let Some(api_key) = self.api_key.clone() else {
            self.state = AppState::Error {
                message: "No API key".into(),
                retryable: true,
            };
            return;
        };

        let model = self.config.model().to_string();
        let branch = summary.branch.clone();
        let files = summary.files.clone();
        let user_prompt = prompt::build_user_prompt(&branch, &files);

        self.state = AppState::Generating {
            branch,
            files,
            generated: String::new(),
        };

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let result = cerebras::generate_commit_message(&api_key, &model, &user_prompt, |c| {
                let _ = tx.send(AppEvent::GenerationChunk(c.to_string()));
            });
            let _ = tx.send(match result {
                Ok(msg) => AppEvent::GenerationComplete(msg),
                Err(e) => AppEvent::GenerationFailed(e.to_string()),
            });
        });
    }

    fn handle_key(&mut self, code: KeyCode) {
        match &mut self.state {
            AppState::ApiKeyInput {
                input,
                cursor,
                error,
            } => {
                error.take();
                match code {
                    KeyCode::Char(c) => {
                        input.insert(*cursor, c);
                        *cursor += 1;
                    }
                    KeyCode::Backspace if *cursor > 0 => {
                        input.remove(*cursor - 1);
                        *cursor -= 1;
                    }
                    KeyCode::Delete if *cursor < input.len() => {
                        input.remove(*cursor);
                    }
                    KeyCode::Left if *cursor > 0 => {
                        *cursor -= 1;
                    }
                    KeyCode::Right if *cursor < input.len() => {
                        *cursor += 1;
                    }
                    KeyCode::Enter if !input.is_empty() => {
                        let _ = self.event_tx.send(AppEvent::ApiKeyEntered(input.clone()));
                    }
                    KeyCode::Esc => self.state = AppState::Exit,
                    _ => {}
                }
            }
            AppState::Error { retryable, .. } => match code {
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
                    self.state = AppState::Exit
                }
                _ => {}
            },
            AppState::Generating { .. } | AppState::Done { .. } => {
                if code == KeyCode::Esc {
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
                let tx = self.event_tx.clone();
                thread::spawn(move || {
                    let _ = tx.send(match cerebras::validate_api_key(&key) {
                        Ok(_) => AppEvent::ApiKeyValidated,
                        Err(e) => AppEvent::ApiKeyValidationFailed(e.to_string()),
                    });
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
            AppEvent::StagingComplete(summary) => self.start_generation(summary),
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
                let message = match &body {
                    Some(b) => format!("{}\n\n{}", title, b),
                    None => title.clone(),
                };

                if self.dry_run {
                    self.state = AppState::Done {
                        message,
                        done_at: Instant::now(),
                    };
                    return;
                }

                let title_for_commit = title.clone();
                let body_for_commit = body.clone();
                let tx = self.event_tx.clone();
                thread::spawn(move || {
                    let _ = tx.send(
                        match crate::git::commit_with_git_cli(
                            &title_for_commit,
                            body_for_commit.as_deref(),
                        ) {
                            Ok(_) => AppEvent::CommitComplete,
                            Err(e) => AppEvent::CommitFailed(e.to_string()),
                        },
                    );
                });

                self.state = AppState::Done {
                    message,
                    done_at: Instant::now(),
                };
            }
            AppEvent::GenerationFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: true,
                };
            }
            AppEvent::CommitFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: false,
                };
            }
            AppEvent::CommitComplete => {}
        }
    }

    fn draw(&self, f: &mut Frame) {
        match &self.state {
            AppState::ApiKeyInput {
                input,
                cursor,
                error,
            } => {
                draw_key_input(f, &self.theme, input, *cursor, error.as_deref());
            }
            AppState::Init => {
                let lines = vec![Line::from(Span::styled(
                    "staging...",
                    self.theme.dim_style(),
                ))];
                f.render_widget(Paragraph::new(lines), f.area());
            }
            AppState::Generating {
                branch,
                files,
                generated,
            } => {
                let area = f.area();
                let files_area = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: files.len().min(8) as u16 + 2,
                };
                let msg_area = Rect {
                    x: area.x,
                    y: files_area.y + files_area.height,
                    width: area.width,
                    height: area.height.saturating_sub(files_area.height),
                };

                draw_files(f, &self.theme, files, files_area);

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(branch.as_str(), self.theme.dim_style()),
                        Span::raw("  "),
                        Span::styled("generating...", self.theme.accent_style()),
                    ]),
                    Line::from(""),
                ];
                for line in generated.lines().take(5) {
                    lines.push(Line::from(Span::styled(line, self.theme.fg_style())));
                }
                if !generated.is_empty() && !generated.ends_with('\n') {
                    lines.push(Line::from(Span::styled("_", self.theme.accent_style())));
                }
                f.render_widget(Paragraph::new(lines), msg_area);
            }
            AppState::Done { message, .. } => {
                let mut lines = vec![
                    Line::from(Span::styled(
                        if self.dry_run { "dry run" } else { "committed" },
                        self.theme.green_style(),
                    )),
                    Line::from(""),
                ];
                for line in message.lines() {
                    lines.push(Line::from(Span::styled(line, self.theme.fg_style())));
                }
                f.render_widget(Paragraph::new(lines), f.area());
            }
            AppState::Error { message, retryable } => {
                draw_error(f, &self.theme, message, *retryable);
            }
            AppState::Exit => {}
        }
    }
}
