use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Padding, Widget},
};

use crate::git::RepoStatus;

pub struct RepoListWidget<'a> {
    pub repos: &'a [RepoStatus],
    pub selected: &'a [bool],
    pub cursor: usize,
}

impl Widget for RepoListWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
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

                let name_style = if is_cursor {
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

                let line = Line::from(vec![
                    Span::styled(format!(" {checkbox} "), indicator_style),
                    Span::styled(format!("{name:<20}"), name_style),
                    Span::styled(format!("{branch:<12}"), branch_style),
                    Span::styled(summary, summary_style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let block = Block::default()
            .title(" Syncro ")
            .borders(Borders::ALL)
            .padding(Padding::vertical(1));

        let list = List::new(items).block(block);
        list.render(area, buf);
    }
}

pub struct HelpBar;

impl Widget for HelpBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let help = Line::from(vec![
            Span::styled(" j/k", Style::default().fg(Color::Cyan)),
            Span::styled(": navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("space", Style::default().fg(Color::Cyan)),
            Span::styled(": toggle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("enter", Style::default().fg(Color::Cyan)),
            Span::styled(": sync  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::styled(": quit", Style::default().fg(Color::DarkGray)),
        ]);
        buf.set_line(area.x, area.y, &help, area.width);
    }
}
