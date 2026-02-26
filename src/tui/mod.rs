mod widgets;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::thread;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph};

use crate::config;
use crate::error::SyncroError;
use crate::git::{self, FileChange, RepoStatus, SyncResult};
use widgets::{
    ChangedFilesWidget, CommitFilesWidget, DiffViewWidget, FocusedPane, HelpBar, RepoDetailWidget,
    RepoListWidget, UnpushedCommitsWidget,
};

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    Browsing,
    Syncing,
    Results,
}

struct App {
    repos: Vec<RepoStatus>,
    selected: Vec<bool>,
    cursor: usize,
    state: AppState,
    sync_results: Vec<SyncResult>,
    focused_pane: FocusedPane,
    commit_cursor: usize,
    file_cursor: usize,
    commit_files_cache: HashMap<String, Vec<FileChange>>,
    diff_cache: HashMap<(PathBuf, String), String>,
}

impl App {
    fn new(repos: Vec<RepoStatus>) -> Self {
        let selected: Vec<bool> = repos
            .iter()
            .map(|r| r.has_changes() || !r.unpushed_commits.is_empty())
            .collect();
        App {
            repos,
            selected,
            cursor: 0,
            state: AppState::Browsing,
            sync_results: Vec::new(),
            focused_pane: FocusedPane::Repos,
            commit_cursor: 0,
            file_cursor: 0,
            commit_files_cache: HashMap::new(),
            diff_cache: HashMap::new(),
        }
    }

    fn current_repo(&self) -> Option<&RepoStatus> {
        self.repos.get(self.cursor)
    }

    fn move_down(&mut self) {
        match self.focused_pane {
            FocusedPane::Repos => {
                if !self.repos.is_empty() && self.cursor < self.repos.len() - 1 {
                    self.cursor += 1;
                    self.commit_cursor = 0;
                    self.file_cursor = 0;
                }
            }
            FocusedPane::Commits => {
                if let Some(repo) = self.current_repo()
                    && !repo.unpushed_commits.is_empty()
                    && self.commit_cursor < repo.unpushed_commits.len() - 1
                {
                    self.commit_cursor += 1;
                    self.ensure_commit_files_cached();
                }
            }
            FocusedPane::Files => {
                if let Some(repo) = self.current_repo()
                    && !repo.changed_files.is_empty()
                    && self.file_cursor < repo.changed_files.len() - 1
                {
                    self.file_cursor += 1;
                    self.ensure_diff_cached();
                }
            }
        }
    }

    fn move_up(&mut self) {
        match self.focused_pane {
            FocusedPane::Repos => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.commit_cursor = 0;
                    self.file_cursor = 0;
                }
            }
            FocusedPane::Commits => {
                if self.commit_cursor > 0 {
                    self.commit_cursor -= 1;
                    self.ensure_commit_files_cached();
                }
            }
            FocusedPane::Files => {
                if self.file_cursor > 0 {
                    self.file_cursor -= 1;
                    self.ensure_diff_cached();
                }
            }
        }
    }

    fn jump_top(&mut self) {
        match self.focused_pane {
            FocusedPane::Repos => {
                self.cursor = 0;
                self.commit_cursor = 0;
                self.file_cursor = 0;
            }
            FocusedPane::Commits => {
                self.commit_cursor = 0;
                self.ensure_commit_files_cached();
            }
            FocusedPane::Files => {
                self.file_cursor = 0;
                self.ensure_diff_cached();
            }
        }
    }

    fn jump_bottom(&mut self) {
        match self.focused_pane {
            FocusedPane::Repos => {
                if !self.repos.is_empty() {
                    self.cursor = self.repos.len() - 1;
                    self.commit_cursor = 0;
                    self.file_cursor = 0;
                }
            }
            FocusedPane::Commits => {
                if let Some(repo) = self.current_repo()
                    && !repo.unpushed_commits.is_empty()
                {
                    self.commit_cursor = repo.unpushed_commits.len() - 1;
                    self.ensure_commit_files_cached();
                }
            }
            FocusedPane::Files => {
                if let Some(repo) = self.current_repo()
                    && !repo.changed_files.is_empty()
                {
                    self.file_cursor = repo.changed_files.len() - 1;
                    self.ensure_diff_cached();
                }
            }
        }
    }

    fn toggle_selection(&mut self) {
        if self.focused_pane == FocusedPane::Repos && !self.repos.is_empty() {
            self.selected[self.cursor] = !self.selected[self.cursor];
        }
    }

    fn sync_selected(&mut self) {
        if self.focused_pane != FocusedPane::Repos {
            return;
        }
        self.state = AppState::Syncing;
        self.sync_results.clear();

        let paths: Vec<_> = self
            .repos
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected[*i])
            .map(|(_, r)| r.path.clone())
            .collect();

        for path in &paths {
            match git::sync_repo(path) {
                Ok(result) => self.sync_results.push(result),
                Err(e) => {
                    self.sync_results.push(SyncResult {
                        path: path.clone(),
                        committed: false,
                        pushed: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        self.state = AppState::Results;
    }

    fn refresh_repos(&mut self) {
        let cfg = match config::load() {
            Ok(cfg) => cfg,
            Err(_) => return,
        };

        let expanded_repos = config::expand_repos(&cfg.repos);

        let handles: Vec<_> = expanded_repos
            .into_iter()
            .map(|p| thread::spawn(move || git::repo_status(&p)))
            .collect();

        self.repos = handles
            .into_iter()
            .map(|h| h.join().expect("thread panicked"))
            .collect();

        self.selected = self
            .repos
            .iter()
            .map(|r| r.has_changes() || !r.unpushed_commits.is_empty())
            .collect();
        if self.cursor >= self.repos.len() && !self.repos.is_empty() {
            self.cursor = self.repos.len() - 1;
        }
        self.commit_cursor = 0;
        self.file_cursor = 0;
        self.commit_files_cache.clear();
        self.diff_cache.clear();
    }

    fn set_focused_pane(&mut self, pane: FocusedPane) {
        self.focused_pane = pane;
        // Reset sub-cursors and trigger caching for the newly focused pane
        match pane {
            FocusedPane::Repos => {}
            FocusedPane::Commits => {
                self.commit_cursor = 0;
                self.ensure_commit_files_cached();
            }
            FocusedPane::Files => {
                self.file_cursor = 0;
                self.ensure_diff_cached();
            }
        }
    }

    /// Extract commit hash from an oneline format string (first word).
    fn current_commit_hash(&self) -> Option<String> {
        let repo = self.current_repo()?;
        let commit_line = repo.unpushed_commits.get(self.commit_cursor)?;
        commit_line.split_whitespace().next().map(String::from)
    }

    fn ensure_commit_files_cached(&mut self) {
        let Some(hash) = self.current_commit_hash() else {
            return;
        };
        if self.commit_files_cache.contains_key(&hash) {
            return;
        }
        let Some(repo) = self.current_repo() else {
            return;
        };
        let files = git::commit_files(&repo.path, &hash);
        self.commit_files_cache.insert(hash, files);
    }

    fn ensure_diff_cached(&mut self) {
        let Some(repo) = self.current_repo() else {
            return;
        };
        let Some(file) = repo.changed_files.get(self.file_cursor) else {
            return;
        };
        let key = (repo.path.clone(), file.path.clone());
        if self.diff_cache.contains_key(&key) {
            return;
        }
        let is_untracked = file.status == "?";
        let diff = git::file_diff(&repo.path, &file.path, is_untracked);
        self.diff_cache.insert(key, diff);
    }

    fn cached_commit_files(&self) -> &[FileChange] {
        if let Some(hash) = self.current_commit_hash()
            && let Some(files) = self.commit_files_cache.get(&hash)
        {
            return files;
        }
        &[]
    }

    fn cached_diff(&self) -> (&str, &str) {
        let Some(repo) = self.current_repo() else {
            return ("", "");
        };
        let Some(file) = repo.changed_files.get(self.file_cursor) else {
            return ("", "");
        };
        let key = (repo.path.clone(), file.path.clone());
        let diff = self.diff_cache.get(&key).map(|s| s.as_str()).unwrap_or("");
        (file.path.as_str(), diff)
    }
}

pub fn run() -> Result<(), SyncroError> {
    let cfg = config::load()?;
    if cfg.repos.is_empty() {
        println!("No repos watched. Use `syncro add <path>` to add repositories.");
        return Ok(());
    }

    let expanded_repos = config::expand_repos(&cfg.repos);

    if expanded_repos.is_empty() {
        println!("No Git repositories found in watched paths.");
        return Ok(());
    }

    let handles: Vec<_> = expanded_repos
        .into_iter()
        .map(|p| thread::spawn(move || git::repo_status(&p)))
        .collect();

    let repos: Vec<RepoStatus> = handles
        .into_iter()
        .map(|h| h.join().expect("thread panicked"))
        .collect();

    let mut app = App::new(repos);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), SyncroError> {
    loop {
        terminal.draw(|f| {
            let area = f.area();

            // Clear stale content from ratatui's alternating buffer to prevent
            // ghost artifacts when the layout changes (e.g. two-column → single-column).
            f.render_widget(Clear, area);

            match app.state {
                AppState::Browsing | AppState::Syncing => {
                    let outer =
                        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);

                    let current_repo = app.repos.get(app.cursor);
                    let show_right_panel = app.focused_pane != FocusedPane::Repos;

                    let main_area = outer[0];

                    if show_right_panel {
                        // Two-column layout
                        let columns = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                            .split(main_area);

                        let left_panes = Layout::vertical([
                            Constraint::Min(5),
                            Constraint::Length(8),
                            Constraint::Length(8),
                            Constraint::Length(3),
                        ])
                        .split(columns[0]);

                        render_left_panes(f, app, current_repo, &left_panes);

                        // Explicitly clear the right column to prevent artifacts when
                        // switching between files with different diff lengths.
                        f.render_widget(Clear, columns[1]);

                        // Right panel content
                        match app.focused_pane {
                            FocusedPane::Commits => {
                                let files = app.cached_commit_files();
                                f.render_widget(CommitFilesWidget { files }, columns[1]);
                            }
                            FocusedPane::Files => {
                                let (title, diff) = app.cached_diff();
                                f.render_widget(
                                    DiffViewWidget {
                                        title,
                                        diff,
                                        scroll: 0,
                                    },
                                    columns[1],
                                );
                            }
                            FocusedPane::Repos => unreachable!(),
                        }
                    } else {
                        // Single-column layout
                        let left_panes = Layout::vertical([
                            Constraint::Min(5),
                            Constraint::Length(8),
                            Constraint::Length(8),
                            Constraint::Length(3),
                        ])
                        .split(main_area);

                        render_left_panes(f, app, current_repo, &left_panes);
                    }

                    f.render_widget(
                        HelpBar {
                            focused_pane: app.focused_pane,
                        },
                        outer[1],
                    );

                    if app.state == AppState::Syncing {
                        let popup = Paragraph::new("Syncing...")
                            .style(Style::default().fg(Color::Yellow))
                            .block(Block::default().borders(Borders::ALL));
                        let popup_area = centered_rect(30, 3, main_area);
                        f.render_widget(popup, popup_area);
                    }
                }
                AppState::Results => {
                    let items: Vec<ListItem> = app
                        .sync_results
                        .iter()
                        .map(|r| {
                            let name = r
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| r.path.display().to_string());

                            let (icon, msg, color) = if let Some(err) = &r.error {
                                ("x", format!("{name}: {err}"), Color::Red)
                            } else {
                                let mut parts = Vec::new();
                                if r.committed {
                                    parts.push("committed");
                                }
                                if r.pushed {
                                    parts.push("pushed");
                                }
                                if parts.is_empty() {
                                    parts.push("nothing to do");
                                }
                                let msg = format!("{name}: {}", parts.join(", "));
                                ("✓", msg, Color::Green)
                            };

                            ListItem::new(Line::from(vec![
                                Span::styled(format!(" {icon} "), Style::default().fg(color)),
                                Span::styled(msg, Style::default().fg(color)),
                            ]))
                        })
                        .collect();

                    let chunks =
                        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);

                    let list = List::new(items).block(
                        Block::default()
                            .title(" Sync Results ")
                            .borders(Borders::ALL)
                            .padding(Padding::vertical(1)),
                    );
                    f.render_widget(list, chunks[0]);

                    let help = Line::from(vec![
                        Span::styled(" enter", Style::default().fg(Color::Cyan)),
                        Span::styled(": back  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("q", Style::default().fg(Color::Cyan)),
                        Span::styled(": quit", Style::default().fg(Color::DarkGray)),
                    ]);
                    f.render_widget(Paragraph::new(help), chunks[1]);
                }
            }
        })?;

        if let Event::Key(key) = event::read()? {
            match app.state {
                AppState::Browsing => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Char('1') => app.set_focused_pane(FocusedPane::Repos),
                    KeyCode::Char('2') => app.set_focused_pane(FocusedPane::Commits),
                    KeyCode::Char('3') => app.set_focused_pane(FocusedPane::Files),
                    KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                    KeyCode::Char('g') => app.jump_top(),
                    KeyCode::Char('G') => app.jump_bottom(),
                    KeyCode::Char(' ') => app.toggle_selection(),
                    KeyCode::Enter => {
                        if app.focused_pane == FocusedPane::Repos && app.selected.iter().any(|&s| s)
                        {
                            app.sync_selected();
                        }
                    }
                    _ => {}
                },
                AppState::Results => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        app.refresh_repos();
                        app.state = AppState::Browsing;
                    }
                    _ => {}
                },
                AppState::Syncing => {}
            }
        }
    }
}

fn render_left_panes(
    f: &mut ratatui::Frame,
    app: &App,
    current_repo: Option<&RepoStatus>,
    panes: &[ratatui::layout::Rect],
) {
    f.render_widget(
        RepoListWidget {
            repos: &app.repos,
            selected: &app.selected,
            cursor: app.cursor,
            focused: app.focused_pane == FocusedPane::Repos,
        },
        panes[0],
    );

    f.render_widget(
        UnpushedCommitsWidget {
            repo: current_repo,
            cursor: app.commit_cursor,
            focused: app.focused_pane == FocusedPane::Commits,
        },
        panes[1],
    );

    f.render_widget(
        ChangedFilesWidget {
            repo: current_repo,
            cursor: app.file_cursor,
            focused: app.focused_pane == FocusedPane::Files,
        },
        panes[2],
    );

    f.render_widget(RepoDetailWidget { repo: current_repo }, panes[3]);
}

fn centered_rect(width: u16, height: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    ratatui::layout::Rect::new(x, y, width.min(area.width), height.min(area.height))
}
