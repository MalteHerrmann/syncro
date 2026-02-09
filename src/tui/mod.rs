mod widgets;

use std::io;
use std::thread;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph};
use ratatui::Terminal;

use crate::config;
use crate::git::{self, RepoStatus, SyncResult};
use widgets::{HelpBar, RepoListWidget};

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
        }
    }

    fn move_down(&mut self) {
        if !self.repos.is_empty() && self.cursor < self.repos.len() - 1 {
            self.cursor += 1;
        }
    }

    fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn jump_top(&mut self) {
        self.cursor = 0;
    }

    fn jump_bottom(&mut self) {
        if !self.repos.is_empty() {
            self.cursor = self.repos.len() - 1;
        }
    }

    fn toggle_selection(&mut self) {
        if !self.repos.is_empty() {
            self.selected[self.cursor] = !self.selected[self.cursor];
        }
    }

    fn sync_selected(&mut self) {
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
        let paths: Vec<_> = self.repos.iter().map(|r| r.path.clone()).collect();
        let handles: Vec<_> = paths
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
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    if cfg.repos.is_empty() {
        println!("No repos watched. Use `syncro add <path>` to add repositories.");
        return Ok(());
    }

    // Query statuses in parallel
    let handles: Vec<_> = cfg
        .repos
        .into_iter()
        .map(|p| thread::spawn(move || git::repo_status(&p)))
        .collect();

    let repos: Vec<RepoStatus> = handles
        .into_iter()
        .map(|h| h.join().expect("thread panicked"))
        .collect();

    let mut app = App::new(repos);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_event_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| {
            let area = f.area();

            match app.state {
                AppState::Browsing | AppState::Syncing => {
                    let chunks = Layout::vertical([
                        Constraint::Min(3),
                        Constraint::Length(1),
                    ])
                    .split(area);

                    let widget = RepoListWidget {
                        repos: &app.repos,
                        selected: &app.selected,
                        cursor: app.cursor,
                    };
                    f.render_widget(widget, chunks[0]);
                    f.render_widget(HelpBar, chunks[1]);

                    if app.state == AppState::Syncing {
                        let popup = Paragraph::new("Syncing...")
                            .style(Style::default().fg(Color::Yellow))
                            .block(Block::default().borders(Borders::ALL));
                        let popup_area = centered_rect(30, 3, area);
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
                                Span::styled(
                                    format!(" {icon} "),
                                    Style::default().fg(color),
                                ),
                                Span::styled(msg, Style::default().fg(color)),
                            ]))
                        })
                        .collect();

                    let chunks = Layout::vertical([
                        Constraint::Min(3),
                        Constraint::Length(1),
                    ])
                    .split(area);

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
                        return Ok(())
                    }
                    KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                    KeyCode::Char('g') => app.jump_top(),
                    KeyCode::Char('G') => app.jump_bottom(),
                    KeyCode::Char(' ') => app.toggle_selection(),
                    KeyCode::Enter => {
                        if app.selected.iter().any(|&s| s) {
                            app.sync_selected();
                        }
                    }
                    _ => {}
                },
                AppState::Results => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(())
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

fn centered_rect(width: u16, height: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    ratatui::layout::Rect::new(x, y, width.min(area.width), height.min(area.height))
}
