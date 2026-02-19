use crate::args::Args;
use crate::cerebras;
use crate::config::{self, Config};
use crate::error::Result;
use crate::git::{GitRepo, StagedSummary, unstage_all_with_git_cli};
use crate::prompt::{self, FileInfo};
use crate::tui::{Theme, Tui, draw_error, draw_key_input};
use crossterm::event::{Event, KeyCode};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph, Wrap},
};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

const NO_CHUNK_TIMEOUT_SECS: u64 = 45;
const MAX_GENERATION_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub enum AppState {
    ApiKeyInput {
        input: String,
        cursor: usize,
        error: Option<String>,
    },
    ApiKeyValidating,
    Staging {
        branch: String,
    },
    Generating {
        branch: String,
        files: Vec<FileInfo>,
        generated: String,
        started_at: Instant,
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
        if args.reset_cache {
            config::clear_local_cache()?;
        }

        let config = config::load()?;
        let api_key = config::get_effective_api_key(&config);
        let (event_tx, event_rx) = mpsc::channel();

        let state = if args.reset_cache || args.reset_key || api_key.is_none() {
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
        if matches!(self.state, AppState::Staging { .. }) {
            self.start_staging();
        }

        loop {
            if let AppState::Done { done_at, .. } = &self.state
                && done_at.elapsed().as_secs() >= 3
            {
                break;
            }

            let generation_timed_out = matches!(
                &self.state,
                AppState::Generating {
                    started_at,
                    generated,
                    ..
                } if (generated.is_empty() && started_at.elapsed().as_secs() >= NO_CHUNK_TIMEOUT_SECS)
                    || started_at.elapsed().as_secs() >= MAX_GENERATION_TIMEOUT_SECS
            );
            if generation_timed_out {
                self.fail_with_cleanup(
                    "Provider timed out while generating commit message. Press R to retry or K to re-enter API key."
                        .into(),
                    true,
                );
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
            started_at: Instant::now(),
        };

        let tx = self.event_tx.clone();
        thread::spawn(move || {
            if let Err(e) = cerebras::validate_api_key(&api_key) {
                let _ = tx.send(AppEvent::GenerationFailed(format!(
                    "API key validation failed before generation: {}",
                    e
                )));
                return;
            }
            if let Err(e) = cerebras::check_provider_ready(&api_key, &model) {
                let _ = tx.send(AppEvent::GenerationFailed(format!(
                    "Provider readiness check failed: {}",
                    e
                )));
                return;
            }

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
            AppState::ApiKeyValidating => {}
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
                self.state = AppState::ApiKeyValidating;
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
                self.fail_with_cleanup(err, false);
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
                self.fail_with_cleanup(err, true);
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
                self.fail_with_cleanup(err, false);
            }
        }
    }

    fn fail_with_cleanup(&mut self, message: String, retryable: bool) {
        let should_unstage = matches!(
            self.state,
            AppState::Staging { .. } | AppState::Generating { .. } | AppState::Committing { .. }
        );
        let final_message = if should_unstage {
            match unstage_all_with_git_cli() {
                Ok(_) => message,
                Err(e) => format!("{}\nAlso failed to unstage changes: {}", message, e),
            }
        } else {
            message
        };

        self.state = AppState::Error {
            message: final_message,
            retryable,
        };
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
            AppState::ApiKeyValidating => {
                let lines = vec![
                    Line::from(""),
                    Line::from(vec![Span::styled("  yeti ", self.theme.accent_style())]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "  validating API key...",
                        self.theme.accent_style(),
                    )]),
                ];
                f.render_widget(Paragraph::new(lines), f.area());
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
                started_at,
            } => {
                let status = generation_status(*started_at, generated);
                self.draw_main(f, branch, files, generated, &status);
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
        let total_add: usize = files.iter().map(|f| f.additions).sum();
        let total_del: usize = files.iter().map(|f| f.deletions).sum();
        let is_done = status == "territory marked" || status == "scent marked";
        let status_style = if is_done {
            self.theme.green_style()
        } else {
            self.theme.accent_style()
        };

        let [header_area, body_area, footer_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .areas(f.area());
        let [files_area, msg_area] =
            Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)])
                .areas(body_area);

        let header_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.theme.accent_style())
            .padding(Padding::horizontal(1));
        let header_inner = header_block.inner(header_area);
        f.render_widget(header_block, header_area);
        let header_line = Line::from(vec![
            Span::styled("yeti", self.theme.accent_style()),
            Span::styled("   ", self.theme.dim_style()),
            Span::styled(branch, self.theme.fg_style()),
            Span::styled("   ", self.theme.fg_style()),
            Span::styled(format!("{} files", files.len()), self.theme.dim_style()),
            Span::styled("  ", self.theme.dim_style()),
            Span::styled(format!("+{}", total_add), self.theme.green_style()),
            Span::styled(" ", self.theme.dim_style()),
            Span::styled(format!("-{}", total_del), self.theme.red_style()),
            Span::styled("   ", self.theme.dim_style()),
            Span::styled(status, status_style),
        ]);
        f.render_widget(Paragraph::new(header_line), header_inner);

        let files_block = Block::bordered()
            .title(Span::styled(" changes ", self.theme.dim_style()))
            .border_type(BorderType::Rounded)
            .border_style(self.theme.dim_style())
            .padding(Padding::new(1, 1, 0, 0));
        let files_inner = files_block.inner(files_area);
        f.render_widget(files_block, files_area);

        let path_width = (files_inner.width.saturating_sub(14) as usize).clamp(16, 52);
        let mut file_lines = vec![Line::from(vec![
            Span::styled("st ", self.theme.dim_style()),
            Span::styled(
                format!("{:<width$}", "file", width = path_width),
                self.theme.dim_style(),
            ),
            Span::styled(" +", self.theme.dim_style()),
            Span::styled("  -", self.theme.dim_style()),
        ])];

        for file in files.iter().take(10) {
            let (status_tag, status_style) = match file.status {
                crate::prompt::FileStatus::Added => ("A", self.theme.green_style()),
                crate::prompt::FileStatus::Deleted => ("D", self.theme.red_style()),
                crate::prompt::FileStatus::Renamed => ("R", self.theme.accent_style()),
                crate::prompt::FileStatus::Modified => ("M", self.theme.yellow_style()),
            };
            let path_display = ellipsize_path(&file.path, path_width);
            let add_text = if file.additions > 0 {
                format!("+{}", file.additions)
            } else {
                "-".to_string()
            };
            let del_text = if file.deletions > 0 {
                format!("-{}", file.deletions)
            } else {
                "-".to_string()
            };
            let add_style = if file.additions > 0 {
                self.theme.green_style()
            } else {
                self.theme.dim_style()
            };
            let del_style = if file.deletions > 0 {
                self.theme.red_style()
            } else {
                self.theme.dim_style()
            };

            file_lines.push(Line::from(vec![
                Span::styled(format!("{:<2} ", status_tag), status_style),
                Span::styled(
                    format!("{:<width$}", path_display, width = path_width),
                    self.theme.fg_style(),
                ),
                Span::styled(format!("{:>3}", add_text), add_style),
                Span::styled(format!("{:>4}", del_text), del_style),
            ]));
        }

        if files.len() > 10 {
            file_lines.push(Line::from(vec![Span::styled(
                format!("... {} more files", files.len() - 10),
                self.theme.dim_style(),
            )]));
        }
        f.render_widget(
            Paragraph::new(file_lines).wrap(Wrap { trim: true }),
            files_inner,
        );

        let msg_block = Block::bordered()
            .title(Span::styled(" commit message ", self.theme.dim_style()))
            .border_type(BorderType::Rounded)
            .border_style(self.theme.dim_style())
            .padding(Padding::new(1, 1, 0, 0));
        let msg_inner = msg_block.inner(msg_area);
        f.render_widget(msg_block, msg_area);

        let mut msg_lines = Vec::new();
        let mut first = true;
        for line in message.lines().take(12) {
            if first {
                msg_lines.push(Line::from(vec![Span::styled(
                    line,
                    self.theme.accent_style(),
                )]));
                first = false;
            } else if line.is_empty() {
                msg_lines.push(Line::from(""));
            } else {
                msg_lines.push(Line::from(vec![Span::styled(line, self.theme.fg_style())]));
            }
        }
        if msg_lines.is_empty() {
            msg_lines.push(Line::from(Span::styled(
                "waiting for generated message...",
                self.theme.dim_style(),
            )));
        }
        f.render_widget(
            Paragraph::new(msg_lines).wrap(Wrap { trim: false }),
            msg_inner,
        );

        let footer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(status_style)
            .padding(Padding::horizontal(1));
        let footer_inner = footer_block.inner(footer_area);
        f.render_widget(footer_block, footer_area);
        let footer_line = Line::from(vec![
            Span::styled(status, status_style),
            Span::styled("  |  ", self.theme.dim_style()),
            Span::styled("Esc/Q exit", self.theme.dim_style()),
        ]);
        f.render_widget(Paragraph::new(footer_line), footer_inner);
    }
}

fn generation_status(started_at: Instant, generated: &str) -> String {
    const FRAMES: [&str; 8] = ["⠋", "⠙", "⠚", "⠞", "⠖", "⠦", "⠴", "⠸"];
    let elapsed = started_at.elapsed();
    let frame = FRAMES[((elapsed.as_millis() / 200) as usize) % FRAMES.len()];
    let elapsed_s = elapsed.as_secs();

    if generated.is_empty() {
        if elapsed_s >= 15 {
            format!(
                "tracking... {} {}s (waiting for provider response)",
                frame, elapsed_s
            )
        } else {
            format!("tracking... {} {}s", frame, elapsed_s)
        }
    } else {
        format!(
            "tracking... {} {}s, {} chars received",
            frame,
            elapsed_s,
            generated.chars().count()
        )
    }
}

fn ellipsize_path(path: &str, max_chars: usize) -> String {
    if max_chars == 0 || path.chars().count() <= max_chars {
        return path.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let tail_len = max_chars - 3;
    let mut tail: Vec<char> = path.chars().rev().take(tail_len).collect();
    tail.reverse();
    format!("...{}", tail.into_iter().collect::<String>())
}
