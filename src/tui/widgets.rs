use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Padding, Widget},
};

/// Fill every cell in `area` with a blank space, preventing stale content
/// from previous frames from showing through.
fn clear_area(area: Rect, buf: &mut Buffer) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.reset();
            }
        }
    }
}

use crate::git::{FileChange, RepoStatus};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedPane {
    Repos,
    Commits,
    Files,
}

fn pane_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .title(title)
        .title_style(style)
        .borders(Borders::ALL)
        .border_style(border_style)
        .padding(Padding::new(1, 1, 0, 0))
}

pub struct RepoListWidget<'a> {
    pub repos: &'a [RepoStatus],
    pub selected: &'a [bool],
    pub cursor: usize,
    pub focused: bool,
}

impl Widget for RepoListWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let max_name_width = self
            .repos
            .iter()
            .map(|r| r.display_name().len())
            .max()
            .unwrap_or(0)
            + 2;
        let max_branch_width = self
            .repos
            .iter()
            .map(|r| r.branch.len())
            .max()
            .unwrap_or(0)
            + 2;

        let items: Vec<ListItem> = self
            .repos
            .iter()
            .enumerate()
            .map(|(i, repo)| {
                let is_cursor = i == self.cursor;
                let checkbox = if repo.error.is_some() {
                    "[!]"
                } else if self.selected[i] {
                    "[x]"
                } else {
                    "[ ]"
                };

                let name = repo.display_name();
                let branch = &repo.branch;
                let summary = repo.summary();

                let indicator_style = if repo.error.is_some() {
                    Style::default().fg(Color::Red)
                } else if self.selected[i] {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let name_style = if is_cursor && self.focused {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let branch_style = if !repo.branch_on_remote && repo.has_remote {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let summary_style = if repo.is_clean() {
                    Style::default().fg(Color::Green)
                } else if repo.error.is_some() {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Yellow)
                };

                let cursor_indicator = if is_cursor && self.focused {
                    ">"
                } else {
                    " "
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{cursor_indicator}{checkbox} "),
                        indicator_style,
                    ),
                    Span::styled(format!("{name:<max_name_width$}"), name_style),
                    Span::styled(format!("{branch:<max_branch_width$}"), branch_style),
                    Span::styled(summary, summary_style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = " [1] Repos ";
        let block = pane_block(title, self.focused)
            .padding(Padding::vertical(1));

        let list = List::new(items).block(block);
        list.render(area, buf);
    }
}

pub struct RepoDetailWidget<'a> {
    pub repo: Option<&'a RepoStatus>,
}

impl Widget for RepoDetailWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Repo Details ")
            .title_style(Style::default().fg(Color::DarkGray))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .padding(Padding::new(1, 1, 0, 0));

        let Some(repo) = self.repo else {
            let paragraph =
                ratatui::widgets::Paragraph::new("No repository selected").block(block);
            paragraph.render(area, buf);
            return;
        };

        let remote = repo
            .remote_url
            .as_deref()
            .unwrap_or("none");

        let (status_text, status_color) = if repo.error.is_some() {
            ("Error", Color::Red)
        } else if repo.is_clean() {
            ("Clean", Color::Green)
        } else {
            ("Dirty", Color::Yellow)
        };

        let line = Line::from(vec![
            Span::styled(
                repo.path.display().to_string(),
                Style::default().fg(Color::White),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(&repo.branch, Style::default().fg(Color::Cyan)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(remote, Style::default().fg(Color::White)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]);

        let paragraph = ratatui::widgets::Paragraph::new(line).block(block);
        paragraph.render(area, buf);
    }
}

pub struct UnpushedCommitsWidget<'a> {
    pub repo: Option<&'a RepoStatus>,
    pub cursor: usize,
    pub focused: bool,
}

impl Widget for UnpushedCommitsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = " [2] Unpushed Commits ";
        let block = pane_block(title, self.focused);
        let inner = block.inner(area);
        block.render(area, buf);
        clear_area(inner, buf);

        let Some(repo) = self.repo else {
            return;
        };

        if repo.unpushed_commits.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(Span::styled(
                "No unpushed commits",
                Style::default().fg(Color::DarkGray),
            ));
            paragraph.render(inner, buf);
            return;
        }

        let lines: Vec<Line> = repo
            .unpushed_commits
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let is_cursor = i == self.cursor && self.focused;
                let indicator = if is_cursor { "> " } else { "  " };
                let style = if is_cursor {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!("{indicator}{c}"), style))
            })
            .collect();

        let paragraph = ratatui::widgets::Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

pub struct ChangedFilesWidget<'a> {
    pub repo: Option<&'a RepoStatus>,
    pub cursor: usize,
    pub focused: bool,
}

impl Widget for ChangedFilesWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = " [3] Changed Files ";
        let block = pane_block(title, self.focused);
        let inner = block.inner(area);
        block.render(area, buf);
        clear_area(inner, buf);

        let Some(repo) = self.repo else {
            return;
        };

        if repo.changed_files.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(Span::styled(
                "No changes",
                Style::default().fg(Color::DarkGray),
            ));
            paragraph.render(inner, buf);
            return;
        }

        let lines: Vec<Line> = repo
            .changed_files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let is_cursor = i == self.cursor && self.focused;
                let indicator = if is_cursor { ">" } else { " " };
                let status_color = match f.status.as_str() {
                    "M" => Color::Yellow,
                    "D" => Color::Red,
                    "?" => Color::Green,
                    _ => Color::White,
                };
                let name_style = if is_cursor {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::styled(
                        format!("{indicator} {} ", f.status),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(&f.path, name_style),
                ])
            })
            .collect();

        let paragraph = ratatui::widgets::Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

pub struct CommitFilesWidget<'a> {
    pub files: &'a [FileChange],
}

impl Widget for CommitFilesWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Commit Files ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .padding(Padding::new(1, 1, 0, 0));
        let inner = block.inner(area);
        block.render(area, buf);
        clear_area(inner, buf);

        if self.files.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(Span::styled(
                "No files",
                Style::default().fg(Color::DarkGray),
            ));
            paragraph.render(inner, buf);
            return;
        }

        let lines: Vec<Line> = self
            .files
            .iter()
            .map(|f| {
                let status_color = match f.status.as_str() {
                    "M" => Color::Yellow,
                    "D" => Color::Red,
                    "A" => Color::Green,
                    _ => Color::White,
                };
                Line::from(vec![
                    Span::styled(
                        format!("{} ", f.status),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(&f.path, Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let paragraph = ratatui::widgets::Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

pub struct DiffViewWidget<'a> {
    pub title: &'a str,
    pub diff: &'a str,
    pub scroll: u16,
}

impl Widget for DiffViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" Diff: {} ", self.title))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .padding(Padding::new(1, 1, 0, 0));
        let inner = block.inner(area);
        block.render(area, buf);
        clear_area(inner, buf);

        if self.diff.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(Span::styled(
                "No diff available",
                Style::default().fg(Color::DarkGray),
            ));
            paragraph.render(inner, buf);
            return;
        }

        let lines: Vec<Line> = self
            .diff
            .lines()
            .map(|line| {
                let style = if line.starts_with('+') {
                    Style::default().fg(Color::Green)
                } else if line.starts_with('-') {
                    Style::default().fg(Color::Red)
                } else if line.starts_with("@@") {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(line, style))
            })
            .collect();

        let paragraph = ratatui::widgets::Paragraph::new(lines)
            .scroll((self.scroll, 0));
        paragraph.render(inner, buf);
    }
}

pub struct HelpBar {
    pub focused_pane: FocusedPane,
}

impl Widget for HelpBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let help = match self.focused_pane {
            FocusedPane::Repos => Line::from(vec![
                Span::styled(" 1/2/3", Style::default().fg(Color::Cyan)),
                Span::styled(": focus  ", Style::default().fg(Color::DarkGray)),
                Span::styled("j/k", Style::default().fg(Color::Cyan)),
                Span::styled(": navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled("space", Style::default().fg(Color::Cyan)),
                Span::styled(": toggle  ", Style::default().fg(Color::DarkGray)),
                Span::styled("enter", Style::default().fg(Color::Cyan)),
                Span::styled(": sync  ", Style::default().fg(Color::DarkGray)),
                Span::styled("q", Style::default().fg(Color::Cyan)),
                Span::styled(": quit", Style::default().fg(Color::DarkGray)),
            ]),
            FocusedPane::Commits => Line::from(vec![
                Span::styled(" 1/2/3", Style::default().fg(Color::Cyan)),
                Span::styled(": focus  ", Style::default().fg(Color::DarkGray)),
                Span::styled("j/k", Style::default().fg(Color::Cyan)),
                Span::styled(": select commit  ", Style::default().fg(Color::DarkGray)),
                Span::styled("q", Style::default().fg(Color::Cyan)),
                Span::styled(": quit", Style::default().fg(Color::DarkGray)),
            ]),
            FocusedPane::Files => Line::from(vec![
                Span::styled(" 1/2/3", Style::default().fg(Color::Cyan)),
                Span::styled(": focus  ", Style::default().fg(Color::DarkGray)),
                Span::styled("j/k", Style::default().fg(Color::Cyan)),
                Span::styled(": select file  ", Style::default().fg(Color::DarkGray)),
                Span::styled("q", Style::default().fg(Color::Cyan)),
                Span::styled(": quit", Style::default().fg(Color::DarkGray)),
            ]),
        };
        buf.set_line(area.x, area.y, &help, area.width);
    }
}
