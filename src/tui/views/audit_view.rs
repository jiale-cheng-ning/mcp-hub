use crate::tui::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let findings = &app.cached_findings;

    if findings.is_empty() {
        let msg = Paragraph::new("✅ No issues found. All MCP server configs look clean.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Security Audit"),
            )
            .style(Style::default().fg(Color::Green));
        frame.render_widget(msg, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for f in findings {
        let (icon, _color) = match f.severity.as_str() {
            "Critical" => ("🔴", Color::Red),
            "Warning" => ("🟡", Color::Yellow),
            _ => ("ℹ️", Color::Blue),
        };
        lines.push(Line::from(vec![
            Span::raw(format!("{} ", icon)),
            Span::styled(
                format!("[{}]", f.rule_id),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(format!(" {}: {}", f.server_name, f.message)),
        ]));
        lines.push(Line::from(Span::styled(
            format!("   Fix: {}", f.fix),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Security Audit ({} findings)", findings.len())),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}
