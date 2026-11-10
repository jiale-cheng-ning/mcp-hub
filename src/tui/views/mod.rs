pub mod audit_view;
pub mod servers;

use crate::tui::app::{App, Tab};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Tabs};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let palette = app.theme.palette();
    // Paint every cell so a theme change cannot leave the previous background
    // behind in blank areas, shortened text or an empty view.
    frame.render_widget(Block::default().style(palette.base()), area);

    let titles = vec!["Servers", "Audit"];
    let selected_tab = match app.tab {
        Tab::Servers => 0,
        Tab::Audit => 1,
    };
    let tabs = Tabs::new(titles)
        .block(palette.block(format!("MCP Hub | {}", app.theme.label())))
        .select(selected_tab)
        .style(palette.base())
        .highlight_style(
            Style::default()
                .fg(palette.accent)
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
        Tab::Servers => "j/k: move  /: search  Tab: audit  t: dark/light  q: quit",
        Tab::Audit => {
            "j/k: move  1/2/3: filter  Tab: servers  t: dark/light  Enter: detail  q: quit"
        }
    };
    let status = ratatui::widgets::Paragraph::new(help_text)
        .block(palette.block("Keys"))
        .style(palette.secondary());
    frame.render_widget(status, chunks[2]);
}
