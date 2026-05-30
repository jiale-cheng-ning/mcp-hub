use crate::audit::rules::Severity;
use crate::tui::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let filtered = app.filtered_findings();

    if filtered.is_empty() && app.cached_findings.is_empty() {
        let msg = Paragraph::new("No issues found. All MCP server configs look clean.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Security Audit"),
            )
            .style(Style::default().fg(Color::Green));
        frame.render_widget(msg, area);
        return;
    }

    // Split area: filter bar on top, findings list below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // filter bar
            Constraint::Min(0),   // findings list
        ])
        .split(area);

    // ─── Filter bar ───
    let filter_text = vec![
        filter_chip("1:Critical", app.audit_filter.show_critical, Color::Red),
        Span::raw("  "),
        filter_chip("2:Warning", app.audit_filter.show_warning, Color::Yellow),
        Span::raw("  "),
        filter_chip("3:Info", app.audit_filter.show_info, Color::Blue),
        Span::raw("  "),
        Span::styled(
            format!("  {}/{} shown", filtered.len(), app.cached_findings.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    let filter_bar = Paragraph::new(Line::from(filter_text))
        .block(Block::default().borders(Borders::ALL).title("Filter (toggle with 1/2/3)"));
    frame.render_widget(filter_bar, chunks[0]);

    // ─── Findings list ───
    if filtered.is_empty() {
        let msg = Paragraph::new("No findings match the current filter.")
            .block(Block::default().borders(Borders::ALL).title("Findings"))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(msg, chunks[1]);
        return;
    }

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (icon, color) = match f.severity {
                Severity::Critical => ("!", Color::Red),
                Severity::Warning => ("~", Color::Yellow),
                Severity::Info => ("i", Color::Blue),
            };
            let style = if i == app.audit_selected {
                Style::default().fg(Color::Black).bg(color)
            } else {
                Style::default()
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", icon),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{}] ", f.rule_id),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(format!("{}: {}", f.server_name, f.message)),
                ]),
                Line::from(Span::styled(
                    format!("   Fix: {}", f.fix),
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            ListItem::new(lines).style(style)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.audit_selected));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Security Audit — {} findings (j/k: navigate, g/G: top/bottom)",
                    filtered.len()
                ))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn filter_chip<'a>(label: &'a str, active: bool, color: Color) -> Span<'a> {
    if active {
        Span::styled(
            format!(" [{}] ", label),
            Style::default().fg(Color::Black).bg(color).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!(" [{}] ", label),
            Style::default().fg(Color::DarkGray),
        )
    }
}
