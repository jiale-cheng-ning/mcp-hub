use crate::audit::rules::Finding;
use crate::config::model::ServerEntry;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

pub enum Tab {
    Servers,
    Audit,
}

pub struct App {
    pub servers: Vec<ServerEntry>,
    pub selected: usize,
    pub tab: Tab,
    pub should_quit: bool,
    pub cached_findings: Vec<Finding>,
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
        }
    }

    pub fn handle_input(&mut self) -> std::io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                        KeyCode::Tab => {
                            self.tab = match self.tab {
                                Tab::Servers => Tab::Audit,
                                Tab::Audit => Tab::Servers,
                            };
                        }
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
            }
        }
        Ok(())
    }
}
