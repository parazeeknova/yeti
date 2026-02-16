use crate::args::Args;
use crate::cerebras;
use crate::config::{self, Config};
use crate::error::Result;
use crate::git::{GitRepo, StagedSummary};
use crate::prompt::{self, FileInfo};
use crate::tui::{Theme, Tui, draw_error, draw_key_input};
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
    ApiKeyInput {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    Staging {
        branch: String,
    },
    Generating {
        branch: String,
        files: Vec<FileInfo>,
        generated: String,
    },
    Committing {
        branch: String,
        files: Vec<FileInfo>,
        message: String,
    },
    Done {
        branch: String,
        files: Vec<FileInfo>,
        message: String,
        done_at: Instant,
    },
    Error {
        message: String,
        retryable: bool,
    },
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

pub struct AppResult {
    pub branch: String,
    pub files: Vec<FileInfo>,
    pub message: String,
    pub dry_run: bool,
}

pub struct App {
    state: AppState,
    config: Config,
    api_key: Option<String>,
    dry_run: bool,
    theme: Theme,
    event_rx: Receiver<AppEvent>,
    event_tx: Sender<AppEvent>,
    result: Option<AppResult>,
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
            AppState::Staging {
                branch: "unknown".into(),
            }
        };

        Ok(Self {
            state,
            config,
            api_key,
            dry_run: args.dry_run,
            theme: Theme::gruvbox(),
            event_rx,
            event_tx,
            result: None,
        })
    }

    pub fn run(&mut self, tui: &mut Tui) -> Result<()> {
        self.start_staging();

        loop {
            if let AppState::Done { done_at, .. } = &self.state
                && done_at.elapsed().as_secs() >= 3
            {
                break;
            }

            if let Some(event) = tui.poll_event(50)
                && let Event::Key(key) = event
                && key.kind == crossterm::event::KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    _ => self.handle_key(key.code),
                }
            }

            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_event(event);
            }

            tui.terminal().draw(|f| self.draw(f))?;
        }

        Ok(())
    }

    pub fn get_result(&self) -> Option<&AppResult> {
        self.result.as_ref()
    }

    fn start_staging(&mut self) {
        let tx = self.event_tx.clone();
        thread::spawn(move || {
            let result = (|| {
                let repo = GitRepo::discover()?;
                repo.stage_all()?;
                repo.get_staged_summary()
            })();

            match result {
                Ok(summary) => {
                    let _ = tx.send(AppEvent::StagingComplete(summary));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::StagingFailed(e.to_string()));
                }
            }
        });
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
            branch: branch.clone(),
            files: files.clone(),
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
                    _ => {}
                }
            }
            AppState::Error { retryable, .. } => match code {
                KeyCode::Char('r') | KeyCode::Char('R') if *retryable => {
                    self.state = AppState::Staging {
                        branch: "unknown".into(),
                    };
                    self.start_staging();
                }
                KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.state = AppState::ApiKeyInput {
                        input: String::new(),
                        cursor: 0,
                        error: None,
                    };
                }
                _ => {}
            },
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
                self.state = AppState::Staging {
                    branch: "unknown".into(),
                };
                self.start_staging();
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
                let message = match &body {
                    Some(b) => format!("{}\n\n{}", title, b),
                    None => title.clone(),
                };

                if self.dry_run {
                    if let AppState::Generating { branch, files, .. } = &self.state {
                        self.result = Some(AppResult {
                            branch: branch.clone(),
                            files: files.clone(),
                            message: message.clone(),
                            dry_run: true,
                        });
                        self.state = AppState::Done {
                            branch: branch.clone(),
                            files: files.clone(),
                            message,
                            done_at: Instant::now(),
                        };
                    }
                    return;
                }

                if let AppState::Generating { branch, files, .. } = &self.state {
                    let branch_clone = branch.clone();
                    let files_clone = files.clone();
                    let message_clone = message.clone();

                    self.state = AppState::Committing {
                        branch: branch.clone(),
                        files: files.clone(),
                        message: message.clone(),
                    };

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

                    self.result = Some(AppResult {
                        branch: branch_clone,
                        files: files_clone,
                        message: message_clone,
                        dry_run: false,
                    });
                }
            }
            AppEvent::GenerationFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: true,
                };
            }
            AppEvent::CommitComplete => {
                if let AppState::Committing {
                    branch,
                    files,
                    message,
                } = &self.state
                {
                    self.state = AppState::Done {
                        branch: branch.clone(),
                        files: files.clone(),
                        message: message.clone(),
                        done_at: Instant::now(),
                    };
                }
            }
            AppEvent::CommitFailed(err) => {
                self.state = AppState::Error {
                    message: err,
                    retryable: false,
                };
            }
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
            AppState::Staging { branch } => {
                let lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  yeti ", self.theme.accent_style()),
                        Span::styled(branch.as_str(), self.theme.fg_style()),
                    ]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "  sniffing out changes...",
                        self.theme.accent_style(),
                    )]),
                ];
                f.render_widget(Paragraph::new(lines), f.area());
            }
            AppState::Generating {
                branch,
                files,
                generated,
            } => {
                self.draw_main(f, branch, files, generated, "tracking...");
            }
            AppState::Committing {
                branch,
                files,
                message,
            } => {
                self.draw_main(f, branch, files, message, "marking territory...");
            }
            AppState::Done {
                branch,
                files,
                message,
                ..
            } => {
                let status = if self.dry_run {
                    "scent marked"
                } else {
                    "territory marked"
                };
                self.draw_main(f, branch, files, message, status);
            }
            AppState::Error { message, retryable } => {
                draw_error(f, &self.theme, message, *retryable);
            }
        }
    }

    fn draw_main(
        &self,
        f: &mut Frame,
        branch: &str,
        files: &[FileInfo],
        message: &str,
        status: &str,
    ) {
        let area = f.area();
        let files_height = (files.len().min(10) + 4) as u16;
        let files_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: files_height,
        };
        let msg_area = Rect {
            x: area.x,
            y: files_height,
            width: area.width,
            height: area.height.saturating_sub(files_height),
        };

        let total_add: usize = files.iter().map(|f| f.additions).sum();
        let total_del: usize = files.iter().map(|f| f.deletions).sum();

        let separator = "─"
            .repeat(area.width as usize)
            .chars()
            .take(area.width as usize)
            .collect::<String>();

        let mut header = vec![
            Span::styled("  yeti ", self.theme.accent_style()),
            Span::styled(branch, self.theme.fg_style()),
            Span::styled("  ", self.theme.fg_style()),
            Span::styled(format!("{} files", files.len()), self.theme.dim_style()),
            Span::styled("  ", self.theme.fg_style()),
            Span::styled(format!("+{}", total_add), self.theme.green_style()),
            Span::styled(" ", self.theme.fg_style()),
            Span::styled(format!("-{}", total_del), self.theme.red_style()),
        ];

        if status == "territory marked" || status == "scent marked" {
            header.push(Span::styled("    ", self.theme.fg_style()));
            header.push(Span::styled("done", self.theme.green_style()));
        }

        let mut file_lines = vec![
            Line::from(header),
            Line::from(Span::styled(separator.clone(), self.theme.dim_style())),
        ];

        let max_path_len = files
            .iter()
            .map(|f| f.path.len())
            .max()
            .unwrap_or(0)
            .min(40);

        for file in files.iter().take(10) {
            let (icon, icon_style) = match file.status {
                crate::prompt::FileStatus::Added => ("A", self.theme.green_style()),
                crate::prompt::FileStatus::Deleted => ("D", self.theme.red_style()),
                crate::prompt::FileStatus::Renamed => ("R", self.theme.yellow_style()),
                crate::prompt::FileStatus::Modified => ("M", self.theme.yellow_style()),
            };

            let path_display = if file.path.len() > 40 {
                format!("...{}", &file.path[file.path.len() - 37..])
            } else {
                format!("{:width$}", file.path, width = max_path_len)
            };

            let add_s = if file.additions > 0 {
                format!("+{}", file.additions)
            } else {
                String::new()
            };
            let del_s = if file.deletions > 0 {
                format!("-{}", file.deletions)
            } else {
                String::new()
            };

            file_lines.push(Line::from(vec![
                Span::styled("  ", self.theme.fg_style()),
                Span::styled(icon, icon_style),
                Span::styled("  ", self.theme.fg_style()),
                Span::styled(path_display, self.theme.fg_style()),
                Span::styled("  ", self.theme.fg_style()),
                Span::styled(add_s, self.theme.green_style()),
                Span::styled(" ", self.theme.fg_style()),
                Span::styled(del_s, self.theme.red_style()),
            ]));
        }

        if files.len() > 10 {
            file_lines.push(Line::from(vec![
                Span::styled("  ", self.theme.fg_style()),
                Span::styled(
                    format!("... {} more", files.len() - 10),
                    self.theme.dim_style(),
                ),
            ]));
        }

        f.render_widget(Paragraph::new(file_lines), files_area);

        let status_style = if status == "territory marked" || status == "scent marked" {
            self.theme.green_style()
        } else {
            self.theme.accent_style()
        };

        let mut msg_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ", self.theme.fg_style()),
                Span::styled(status, status_style),
            ]),
            Line::from(""),
        ];

        let mut first = true;
        for line in message.lines() {
            if first {
                msg_lines.push(Line::from(vec![
                    Span::styled("  ", self.theme.fg_style()),
                    Span::styled(line, self.theme.accent_style()),
                ]));
                first = false;
            } else if line.is_empty() {
                msg_lines.push(Line::from(""));
            } else {
                msg_lines.push(Line::from(vec![
                    Span::styled("  ", self.theme.fg_style()),
                    Span::styled(line, self.theme.fg_style()),
                ]));
            }
        }

        f.render_widget(Paragraph::new(msg_lines), msg_area);
    }
}
