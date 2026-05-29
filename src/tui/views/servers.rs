use crate::config::model::ServerStatus;
use crate::tui::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let header = Row::new(vec![
        Cell::from("Name"),
        Cell::from("Client"),
        Cell::from("Command"),
        Cell::from("Status"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let status_str = match &s.status {
                ServerStatus::Active => "🟢 Active",
                ServerStatus::Disabled => "⏸  Disabled",
                ServerStatus::ParseError(_) => "🔴 Error",
            };
            let style = if i == app.selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(s.name.clone()),
                Cell::from(format!("{}", s.source_client)),
                Cell::from(s.command.clone()),
                Cell::from(status_str),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Servers"));

    frame.render_widget(table, chunks[0]);

    let detail = if let Some(server) = app.servers.get(app.selected) {
        let env_display = if server.env.is_empty() {
            "(none)".to_string()
        } else {
            server
                .env
                .keys()
                .map(|k| format!("  {}=****", k))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "Name: {}\nCommand: {} {}\nClient: {}\nSource: {}\nEnv:\n{}",
            server.name,
            server.command,
            server.args.join(" "),
            server.source_client,
            server.source_path.display(),
            env_display
        )
    } else {
        "No server selected".to_string()
    };

    let detail_widget = Paragraph::new(detail)
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(detail_widget, chunks[1]);
}
