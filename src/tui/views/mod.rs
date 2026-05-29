pub mod audit_view;
pub mod servers;

use crate::tui::app::{App, Tab};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Tabs};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let titles = vec!["Servers", "Audit"];
    let selected_tab = match app.tab {
        Tab::Servers => 0,
        Tab::Audit => 1,
    };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("MCP Hub"))
        .select(selected_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Servers => servers::draw(frame, app, chunks[1]),
        Tab::Audit => audit_view::draw(frame, app, chunks[1]),
    }

    let help_text = match app.tab {
        Tab::Servers => "j/k: navigate  Tab: switch  q: quit",
        Tab::Audit => "Tab: switch  q: quit",
    };
    let status = ratatui::widgets::Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, chunks[2]);
}
