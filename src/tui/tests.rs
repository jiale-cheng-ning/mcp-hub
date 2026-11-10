use super::{
    app::{App, Tab},
    theme::Theme,
    views,
};
use crate::{
    audit::rules::{Finding, Severity},
    config::{
        model::{ClientType, ServerEntry, ServerStatus},
        transport::Transport,
    },
};
use crossterm::event::KeyCode;
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};
use serde_json::json;
use std::collections::HashMap;

fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => panic!("Theme left a terminal-dependent color: {color:?}"),
    }
}

fn luminance(color: Color) -> f64 {
    let [r, g, b] = rgb(color).map(|v| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn contrast(fg: Color, bg: Color) -> f64 {
    let a = luminance(fg);
    let b = luminance(bg);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn theme_text_and_badges_have_readable_contrast() {
    for theme in [Theme::Dark, Theme::Light] {
        let p = theme.palette();
        for fg in [
            p.foreground,
            p.muted,
            p.accent,
            p.critical,
            p.warning,
            p.info,
            p.success,
        ] {
            assert!(
                contrast(fg, p.background) >= 4.5,
                "{theme:?}: {fg:?} on background"
            );
        }
        for fg in [
            p.selected_foreground,
            p.critical,
            p.warning,
            p.info,
            p.success,
        ] {
            assert!(
                contrast(fg, p.selected_background) >= 4.5,
                "{theme:?}: {fg:?} on selection"
            );
        }
        for bg in [p.critical, p.warning, p.info] {
            assert!(
                contrast(p.badge_foreground, bg) >= 4.5,
                "{theme:?}: badge {bg:?}"
            );
        }
        assert!(contrast(p.border, p.background) >= 3.0);
    }
}

fn fixture(theme: Theme) -> App {
    let servers = [
        ServerStatus::Active,
        ServerStatus::Disabled,
        ServerStatus::ParseError("fixture".into()),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, status)| ServerEntry {
        name: ["filesystem", "github", "postgres"][i].into(),
        source_client: ClientType::Cursor,
        source_path: "/demo/mcp.json".into(),
        transport: Transport::Stdio,
        command: "node".into(),
        args: vec!["server.js".into()],
        env: HashMap::new(),
        status,
    })
    .collect();
    let mut app = App::new(servers, theme);
    app.cached_findings = [Severity::Critical, Severity::Warning, Severity::Info]
        .into_iter()
        .enumerate()
        .map(|(i, severity)| Finding {
            rule_id: ["ENV_PLAINTEXT_SECRET", "NO_VERSION_PIN", "DUPLICATE_SERVER"][i].into(),
            severity,
            server_name: ["github", "filesystem", "postgres"][i].into(),
            message: [
                "Plaintext credential in configuration",
                "Package version is not pinned",
                "Server appears in multiple clients",
            ][i]
                .into(),
            fix: [
                "Use an environment variable reference",
                "Pin a specific package version",
                "Review duplicate configurations",
            ][i]
                .into(),
        })
        .collect();
    app
}

fn check_buffer(buffer: &Buffer) {
    for cell in buffer.content() {
        // Every cell, including blank margins, must have a concrete background.
        let _ = rgb(cell.bg);
        if cell.symbol().trim().is_empty() {
            continue;
        }
        let threshold = if "┌┐└┘─│├┤┬┴┼".contains(cell.symbol()) {
            3.0
        } else {
            4.5
        };
        assert!(
            contrast(cell.fg, cell.bg) >= threshold,
            "Unreadable rendered cell {:?}: {:?} on {:?} ({:.2})",
            cell.symbol(),
            cell.fg,
            cell.bg,
            contrast(cell.fg, cell.bg)
        );
    }
}

fn preview(buffer: &Buffer, name: &str) {
    if let Some(directory) = std::env::var_os("MCP_HUB_THEME_PREVIEW_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let cells: Vec<_> = buffer
            .content()
            .iter()
            .map(|c| {
                json!({
                    "text": c.symbol(), "fg": rgb(c.fg), "bg": rgb(c.bg),
                })
            })
            .collect();
        std::fs::write(
            directory.join(format!("{name}.json")),
            json!({"width": buffer.area.width, "height": buffer.area.height, "cells": cells})
                .to_string(),
        )
        .unwrap();
    }
}

#[test]
fn switching_themes_repaints_all_views_and_selected_text() {
    let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
    let mut app = fixture(Theme::Dark);
    for theme in [Theme::Dark, Theme::Light, Theme::Dark] {
        app.theme = theme;
        app.tab = Tab::Servers;
        for selected in 0..3 {
            app.selected = selected;
            terminal.draw(|frame| views::draw(frame, &mut app)).unwrap();
            check_buffer(terminal.backend().buffer());
        }
        preview(
            terminal.backend().buffer(),
            &format!("{}-servers", theme.label().to_lowercase()),
        );
        app.tab = Tab::Audit;
        for selected in 0..3 {
            app.audit_selected = selected;
            terminal.draw(|frame| views::draw(frame, &mut app)).unwrap();
            check_buffer(terminal.backend().buffer());
        }
        preview(
            terminal.backend().buffer(),
            &format!("{}-audit", theme.label().to_lowercase()),
        );
        // Inactive filter chips and empty views need their own contrast checks.
        app.audit_filter.show_critical = false;
        app.audit_filter.show_warning = false;
        app.audit_filter.show_info = false;
        terminal.draw(|frame| views::draw(frame, &mut app)).unwrap();
        check_buffer(terminal.backend().buffer());
        app.audit_filter.show_critical = true;
        app.audit_filter.show_warning = true;
        app.audit_filter.show_info = true;
    }
}

#[test]
fn empty_views_and_small_windows_have_explicit_theme_colors() {
    for theme in [Theme::Dark, Theme::Light] {
        for (width, height) in [(110, 30), (60, 18), (20, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut app = App::new(vec![], theme);
            for tab in [Tab::Servers, Tab::Audit] {
                app.tab = tab;
                terminal.draw(|frame| views::draw(frame, &mut app)).unwrap();
                check_buffer(terminal.backend().buffer());
            }
        }
    }
}

#[test]
fn theme_shortcut_preserves_navigation_filters_and_search_input() {
    let mut app = fixture(Theme::Dark);
    app.selected = 1;
    app.search_query = "git".into();
    app.handle_key(KeyCode::Char('t'));
    assert_eq!(app.theme, Theme::Light);
    assert_eq!(app.selected, 1);
    assert_eq!(app.search_query, "git");
    assert!(app.theme_changed);
    app.handle_key(KeyCode::Tab);
    app.handle_key(KeyCode::Char('1'));
    app.handle_key(KeyCode::Char('t'));
    assert_eq!(app.theme, Theme::Dark);
    assert!(!app.audit_filter.show_critical);
    app.handle_key(KeyCode::Tab);
    app.handle_key(KeyCode::Char('/'));
    app.handle_key(KeyCode::Char('t'));
    assert_eq!(app.search_query, "t");
    assert_eq!(app.theme, Theme::Dark);
}
