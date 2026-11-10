use crate::config::model::ServerStatus;
use crate::tui::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Paragraph, Row, Table};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.theme.palette();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let filtered_indices = app.filtered_server_indices();

    let header = Row::new(vec![
        Cell::from("Name"),
        Cell::from("Client"),
        Cell::from("Command"),
        Cell::from("Status"),
    ])
    .style(
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = filtered_indices
        .iter()
        .map(|&i| {
            let s = &app.servers[i];
            let (status_str, status_color) = match &s.status {
                ServerStatus::Active => ("[+] Active", palette.success),
                ServerStatus::Disabled => ("[-] Disabled", palette.muted),
                ServerStatus::ParseError(_) => ("[!] Error", palette.critical),
            };
            let style = if i == app.selected {
                palette.selected().add_modifier(Modifier::BOLD)
            } else {
                palette.base()
            };
            Row::new(vec![
                Cell::from(format!(
                    "{}{}",
                    if i == app.selected { "> " } else { "  " },
                    s.name
                )),
                Cell::from(format!("{}", s.source_client)),
                Cell::from(format!("{}: {}", s.transport.label(), s.display_endpoint())),
                Cell::from(status_str).style(Style::default().fg(
                    if i == app.selected && matches!(s.status, ServerStatus::Disabled) {
                        palette.selected_foreground
                    } else {
                        status_color
                    },
                )),
            ])
            .style(style)
        })
        .collect();

    // Title shows search state
    let title = if app.search_mode {
        format!("Servers — searching: {}▌", app.search_query)
    } else if !app.search_query.is_empty() {
        format!(
            "Servers — filter '{}' ({} of {}, Esc to clear)",
            app.search_query,
            filtered_indices.len(),
            app.servers.len()
        )
    } else {
        format!("Servers ({})  / to search", app.servers.len())
    };

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Percentage(20),
        ],
    )
    .style(palette.base())
    .header(header)
    .block(palette.block(title));

    frame.render_widget(table, chunks[0]);

    let detail = if let Some(server) = app.servers.get(app.selected) {
        let env_display = if server.credential_fields().next().is_none() {
            "(none)".to_string()
        } else {
            server
                .credential_fields()
                .map(|(k, _)| format!("  {}=****", k))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "Name: {}\nCommand: {} {}\nClient: {}\nSource: {}\nEnv:\n{}",
            server.name,
            server.display_endpoint(),
            format_args!(
                "[{} arguments; {}]",
                server.args.len(),
                server.transport.label()
            ),
            server.source_client,
            server.source_path.display(),
            env_display
        )
    } else {
        "No server selected".to_string()
    };

    let detail_widget = Paragraph::new(detail)
        .style(palette.base())
        .block(palette.block("Detail"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(detail_widget, chunks[1]);
}
