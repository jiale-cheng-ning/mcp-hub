pub mod app;
pub mod theme;
pub mod views;

#[cfg(test)]
mod tests;

use app::App;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run_tui(
    servers: Vec<crate::config::model::ServerEntry>,
    theme: Option<theme::Theme>,
) -> io::Result<()> {
    enable_raw_mode()?;
    let restore = RestoreTerminal;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(servers, theme.unwrap_or_else(theme::Theme::load));

    loop {
        terminal.draw(|frame| views::draw(frame, &mut app))?;
        app.handle_input()?;
        if app.should_quit {
            break;
        }
    }

    drop(terminal);
    drop(restore);
    // Only interactive changes are remembered; --theme is a session override.
    if app.theme_changed {
        app.theme.save().map_err(|_| {
            io::Error::other("Unable to save theme preference; theme changed for this session only")
        })?;
    }
    Ok(())
}
