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
    /// Search mode state
    pub search_mode: bool,
    pub search_query: String,
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
            search_mode: false,
            search_query: String::new(),
        }
    }

    /// Indices of servers matching the current search query.
    pub fn filtered_server_indices(&self) -> Vec<usize> {
        if self.search_query.is_empty() {
            return (0..self.servers.len()).collect();
        }
        let q = self.search_query.to_lowercase();
        (0..self.servers.len())
            .filter(|&i| {
                let s = &self.servers[i];
                s.name.to_lowercase().contains(&q)
                    || s.display_endpoint().to_lowercase().contains(&q)
                    || s.args.iter().any(|a| a.to_lowercase().contains(&q))
                    || format!("{}", s.source_client).to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn filtered_findings(&self) -> Vec<&Finding> {
        self.cached_findings
            .iter()
            .filter(|f| self.audit_filter.matches(&f.severity))
            .collect()
    }

    /// Jump to the Servers tab and select the server matching the current audit finding.
    pub fn jump_to_server_for_finding(&mut self) {
        let filtered = self.filtered_findings();
        if let Some(finding) = filtered.get(self.audit_selected) {
            let name = &finding.server_name;
            if let Some(idx) = self.servers.iter().position(|s| &s.name == name) {
                self.selected = idx;
                self.search_query.clear();
                self.tab = Tab::Servers;
            }
        }
    }

    pub fn handle_input(&mut self) -> std::io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Search mode captures all input
                    if self.search_mode {
                        self.handle_search_input(key.code);
                        return Ok(());
                    }
                    match &self.tab {
                        Tab::Servers => self.handle_servers_input(key.code),
                        Tab::Audit => self.handle_audit_input(key.code),
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_search_input(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.selected = 0;
            }
            KeyCode::Enter => {
                self.search_mode = false;
                // Keep the query active for filtering
                let indices = self.filtered_server_indices();
                self.selected = indices.first().copied().unwrap_or(0);
            }
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
            }
            _ => {}
        }
    }

    fn handle_servers_input(&mut self, code: KeyCode) {
        let filtered = self.filtered_server_indices();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if !self.search_query.is_empty() {
                    self.search_query.clear();
                    self.selected = 0;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
            }
            KeyCode::Tab => self.tab = Tab::Audit,
            KeyCode::Char('j') | KeyCode::Down if !filtered.is_empty() => {
                let pos = filtered
                    .iter()
                    .position(|&i| i == self.selected)
                    .unwrap_or(0);
                let next = (pos + 1) % filtered.len();
                self.selected = filtered[next];
            }
            KeyCode::Char('k') | KeyCode::Up if !filtered.is_empty() => {
                let pos = filtered
                    .iter()
                    .position(|&i| i == self.selected)
                    .unwrap_or(0);
                let prev = if pos == 0 {
                    filtered.len() - 1
                } else {
                    pos - 1
                };
                self.selected = filtered[prev];
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
            // Enter: jump to the server that produced this finding
            KeyCode::Enter => self.jump_to_server_for_finding(),
            _ => {}
        }
    }
}
