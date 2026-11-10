use crate::audit::rules::Severity;
use crate::tui::app::App;
use crate::tui::theme::Palette;
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.theme.palette();
    let filtered = app.filtered_findings();

    if filtered.is_empty() && app.cached_findings.is_empty() {
        let msg = Paragraph::new("No issues found. All MCP server configs look clean.")
            .block(palette.block("Security Audit"))
            .style(palette.base().fg(palette.success));
        frame.render_widget(msg, area);
        return;
    }

    // Split area: filter bar on top, findings list below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // filter bar
            Constraint::Min(0),    // findings list
        ])
        .split(area);

    // ─── Filter bar ───
    let filter_text = vec![
        filter_chip(
            "1:Critical",
            app.audit_filter.show_critical,
            palette.critical,
            palette,
        ),
        Span::raw("  "),
        filter_chip(
            "2:Warning",
            app.audit_filter.show_warning,
            palette.warning,
            palette,
        ),
        Span::raw("  "),
        filter_chip("3:Info", app.audit_filter.show_info, palette.info, palette),
        Span::raw("  "),
        Span::styled(
            format!("  {}/{} shown", filtered.len(), app.cached_findings.len()),
            palette.secondary(),
        ),
    ];
    let filter_bar = Paragraph::new(Line::from(filter_text))
        .style(palette.base())
        .block(palette.block("Filter (toggle with 1/2/3)"));
    frame.render_widget(filter_bar, chunks[0]);

    // ─── Findings list ───
    if filtered.is_empty() {
        let msg = Paragraph::new("No findings match the current filter.")
            .block(palette.block("Findings"))
            .style(palette.secondary());
        frame.render_widget(msg, chunks[1]);
        return;
    }

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (icon, color) = match f.severity {
                Severity::Critical => ("!", palette.critical),
                Severity::Warning => ("~", palette.warning),
                Severity::Info => ("i", palette.info),
            };
            let style = if i == app.audit_selected {
                palette.selected()
            } else {
                palette.base()
            };
            let secondary = if i == app.audit_selected {
                palette.selected()
            } else {
                palette.secondary()
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", icon),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("[{}] ", f.rule_id), secondary),
                    Span::raw(format!("{}: {}", f.server_name, f.message)),
                ]),
                Line::from(Span::styled(format!("   Fix: {}", f.fix), secondary)),
            ];

            ListItem::new(lines).style(style)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.audit_selected));

    let list = List::new(items)
        .style(palette.base())
        .block(palette.block(format!(
            "Security Audit — {} findings (j/k: navigate, Enter: jump to server)",
            filtered.len()
        )))
        .highlight_style(palette.selected().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn filter_chip<'a>(label: &'a str, active: bool, color: Color, palette: Palette) -> Span<'a> {
    if active {
        Span::styled(
            format!(" [{}] ", label),
            Style::default()
                .fg(palette.badge_foreground)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" [{}] ", label), palette.secondary())
    }
}
