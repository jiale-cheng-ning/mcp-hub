use ratatui::style::{Color, Style};
use ratatui::{
    text::Line,
    widgets::{Block, Borders},
};
use std::{fs, io, path::PathBuf};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub fn toggle(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    fn preference_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("mcp-hub").join("theme"))
    }

    pub fn load() -> Self {
        Self::preference_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| match text.trim() {
                "dark" => Some(Self::Dark),
                "light" => Some(Self::Light),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn save(self) -> io::Result<()> {
        let path = Self::preference_path()
            .ok_or_else(|| io::Error::other("Cannot locate theme preferences"))?;
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(
            path,
            match self {
                Self::Dark => "dark\n",
                Self::Light => "light\n",
            },
        )
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: Color::Rgb(15, 23, 42),
                foreground: Color::Rgb(226, 232, 240),
                muted: Color::Rgb(148, 163, 184),
                border: Color::Rgb(100, 116, 139),
                accent: Color::Rgb(125, 211, 252),
                selected_background: Color::Rgb(30, 58, 95),
                selected_foreground: Color::Rgb(248, 250, 252),
                critical: Color::Rgb(253, 164, 175),
                warning: Color::Rgb(252, 211, 77),
                info: Color::Rgb(147, 197, 253),
                success: Color::Rgb(134, 239, 172),
                badge_foreground: Color::Rgb(15, 23, 42),
            },
            Self::Light => Palette {
                background: Color::Rgb(248, 250, 252),
                foreground: Color::Rgb(15, 23, 42),
                muted: Color::Rgb(71, 85, 105),
                border: Color::Rgb(100, 116, 139),
                accent: Color::Rgb(7, 89, 133),
                selected_background: Color::Rgb(219, 234, 254),
                selected_foreground: Color::Rgb(23, 37, 84),
                critical: Color::Rgb(159, 18, 57),
                warning: Color::Rgb(146, 64, 14),
                info: Color::Rgb(30, 64, 175),
                success: Color::Rgb(22, 101, 52),
                badge_foreground: Color::Rgb(255, 255, 255),
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub selected_background: Color,
    pub selected_foreground: Color,
    pub critical: Color,
    pub warning: Color,
    pub info: Color,
    pub success: Color,
    pub badge_foreground: Color,
}

impl Palette {
    pub fn block<'a>(self, title: impl Into<Line<'a>>) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .title(title.into())
            .style(self.base())
            .border_style(Style::default().fg(self.border))
            .title_style(self.base())
    }
    pub fn base(self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }
    pub fn secondary(self) -> Style {
        self.base().fg(self.muted)
    }
    pub fn selected(self) -> Style {
        Style::default()
            .fg(self.selected_foreground)
            .bg(self.selected_background)
    }
}
