use crate::audit::rules::Finding;
use crate::audit::rules::Severity;
use crate::config::model::ServerEntry;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

pub enum Tab {
    Servers,
    Audit,
}

pub struct AuditFilter {
    pub show_critical: bool,
    pub show_warning: bool,
    pub show_info: bool,
}

impl AuditFilter {
    pub fn new() -> Self {
        Self {
            show_critical: true,
            show_warning: true,
            show_info: true,
        }
    }

    pub fn matches(&self, severity: &Severity) -> bool {
        match severity {
            Severity::Critical => self.show_critical,
            Severity::Warning => self.show_warning,
            Severity::Info => self.show_info,
        }
    }

    pub fn toggle(&mut self, severity: &Severity) {
        match severity {
            Severity::Critical => self.show_critical = !self.show_critical,
            Severity::Warning => self.show_warning = !self.show_warning,
            Severity::Info => self.show_info = !self.show_info,
        }
    }
}

pub struct App {
    pub servers: Vec<ServerEntry>,
    pub selected: usize,
    pub tab: Tab,
    pub should_quit: bool,
    pub cached_findings: Vec<Finding>,
    pub audit_filter: AuditFilter,
    #[allow(dead_code)]
    pub audit_scroll: usize,
    pub audit_selected: usize,
}

impl App {
    pub fn new(servers: Vec<ServerEntry>) -> Self {
        let cached_findings = crate::audit::engine::run_audit(&servers);
        Self {
            servers,
            selected: 0,
            tab: Tab::Servers,
            should_quit: false,
            cached_findings,
            audit_filter: AuditFilter::new(),
            audit_scroll: 0,
            audit_selected: 0,
        }
    }

    pub fn filtered_findings(&self) -> Vec<&Finding> {
        self.cached_findings
            .iter()
            .filter(|f| self.audit_filter.matches(&f.severity))
            .collect()
    }

    pub fn handle_input(&mut self) -> std::io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match &self.tab {
                        Tab::Servers => self.handle_servers_input(key.code),
                        Tab::Audit => self.handle_audit_input(key.code),
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_servers_input(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => self.tab = Tab::Audit,
            KeyCode::Char('j') | KeyCode::Down if !self.servers.is_empty() => {
                self.selected = (self.selected + 1) % self.servers.len();
            }
            KeyCode::Char('k') | KeyCode::Up if !self.servers.is_empty() => {
                self.selected = if self.selected == 0 {
                    self.servers.len() - 1
                } else {
                    self.selected - 1
                };
            }
            _ => {}
        }
    }

    fn handle_audit_input(&mut self, code: KeyCode) {
        let filtered = self.filtered_findings();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => self.tab = Tab::Servers,
            KeyCode::Char('1') => self.audit_filter.toggle(&Severity::Critical),
            KeyCode::Char('2') => self.audit_filter.toggle(&Severity::Warning),
            KeyCode::Char('3') => self.audit_filter.toggle(&Severity::Info),
            KeyCode::Char('j') | KeyCode::Down if !filtered.is_empty() => {
                self.audit_selected = (self.audit_selected + 1) % filtered.len();
            }
            KeyCode::Char('k') | KeyCode::Up if !filtered.is_empty() => {
                self.audit_selected = if self.audit_selected == 0 {
                    filtered.len() - 1
                } else {
                    self.audit_selected - 1
                };
            }
            KeyCode::Char('g') => self.audit_selected = 0,
            KeyCode::Char('G') if !filtered.is_empty() => {
                self.audit_selected = filtered.len() - 1;
            }
            _ => {}
        }
    }
}
