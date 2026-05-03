//! Theme: colors and styles for the TUI.

use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

// ── Theme names ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeName {
    #[default]
    Midnight,
    Dracula,
    Nord,
    Gruvbox,
    TokyoNight,
    CatppuccinMocha,
    RosePine,
    SolarizedDark,
    Everforest,
    Monokai,
    Matrix,
    Gmail,
}

impl ThemeName {
    pub const ALL: &'static [ThemeName] = &[
        ThemeName::Midnight,
        ThemeName::Dracula,
        ThemeName::Nord,
        ThemeName::Gruvbox,
        ThemeName::TokyoNight,
        ThemeName::CatppuccinMocha,
        ThemeName::RosePine,
        ThemeName::SolarizedDark,
        ThemeName::Everforest,
        ThemeName::Monokai,
        ThemeName::Matrix,
        ThemeName::Gmail,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Midnight       => "Midnight",
            ThemeName::Dracula        => "Dracula",
            ThemeName::Nord           => "Nord",
            ThemeName::Gruvbox        => "Gruvbox",
            ThemeName::TokyoNight     => "Tokyo Night",
            ThemeName::CatppuccinMocha=> "Catppuccin Mocha",
            ThemeName::RosePine       => "Rose Pine",
            ThemeName::SolarizedDark  => "Solarized Dark",
            ThemeName::Everforest     => "Everforest",
            ThemeName::Monokai        => "Monokai",
            ThemeName::Matrix         => "Matrix",
            ThemeName::Gmail          => "Gmail",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

// ── Palette ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Palette {
    accent:      Color,
    muted:       Color,
    normal_fg:   Color,
    unread_fg:   Color,
    selected_bg: Color,
    border:      Color,
    error:       Color,
    success:     Color,
    status_fg:   Color,
    status_bg:   Color,
}

impl Palette {
    fn for_theme(name: ThemeName) -> Self {
        match name {
            ThemeName::Midnight => Self {
                accent:      Color::Rgb(124, 156, 255),
                muted:       Color::Rgb(120, 120, 130),
                normal_fg:   Color::Rgb(210, 210, 220),
                unread_fg:   Color::Rgb(240, 240, 250),
                selected_bg: Color::Rgb(48,  56,  86),
                border:      Color::Rgb(80,  80,  100),
                error:       Color::Rgb(244, 102, 102),
                success:     Color::Rgb(120, 210, 140),
                status_fg:   Color::Rgb(180, 180, 200),
                status_bg:   Color::Rgb(32,  34,  48),
            },
            ThemeName::Dracula => Self {
                accent:      Color::Rgb(189, 147, 249), // purple
                muted:       Color::Rgb(98,  114, 164),
                normal_fg:   Color::Rgb(248, 248, 242),
                unread_fg:   Color::Rgb(255, 255, 255),
                selected_bg: Color::Rgb(68,  71,  90),
                border:      Color::Rgb(68,  71,  90),
                error:       Color::Rgb(255, 85,  85),
                success:     Color::Rgb(80,  250, 123),
                status_fg:   Color::Rgb(248, 248, 242),
                status_bg:   Color::Rgb(40,  42,  54),
            },
            ThemeName::Nord => Self {
                accent:      Color::Rgb(136, 192, 208), // frost blue
                muted:       Color::Rgb(76,  86,  106),
                normal_fg:   Color::Rgb(216, 222, 233),
                unread_fg:   Color::Rgb(236, 239, 244),
                selected_bg: Color::Rgb(59,  66,  82),
                border:      Color::Rgb(67,  76,  94),
                error:       Color::Rgb(191, 97,  106),
                success:     Color::Rgb(163, 190, 140),
                status_fg:   Color::Rgb(216, 222, 233),
                status_bg:   Color::Rgb(36,  41,  51),
            },
            ThemeName::Gruvbox => Self {
                accent:      Color::Rgb(215, 153, 33),  // yellow
                muted:       Color::Rgb(146, 131, 116),
                normal_fg:   Color::Rgb(235, 219, 178),
                unread_fg:   Color::Rgb(251, 241, 199),
                selected_bg: Color::Rgb(80,  73,  69),
                border:      Color::Rgb(102, 92,  84),
                error:       Color::Rgb(204, 36,  29),
                success:     Color::Rgb(152, 151, 26),
                status_fg:   Color::Rgb(235, 219, 178),
                status_bg:   Color::Rgb(40,  40,  40),
            },
            ThemeName::TokyoNight => Self {
                accent:      Color::Rgb(122, 162, 247), // blue
                muted:       Color::Rgb(86,  95,  137),
                normal_fg:   Color::Rgb(192, 202, 245),
                unread_fg:   Color::Rgb(220, 224, 255),
                selected_bg: Color::Rgb(41,  46,  66),
                border:      Color::Rgb(41,  46,  73),
                error:       Color::Rgb(247, 118, 142),
                success:     Color::Rgb(158, 206, 106),
                status_fg:   Color::Rgb(192, 202, 245),
                status_bg:   Color::Rgb(22,  22,  30),
            },
            ThemeName::CatppuccinMocha => Self {
                accent:      Color::Rgb(203, 166, 247), // mauve
                muted:       Color::Rgb(108, 112, 134),
                normal_fg:   Color::Rgb(205, 214, 244),
                unread_fg:   Color::Rgb(220, 224, 255),
                selected_bg: Color::Rgb(49,  50,  68),
                border:      Color::Rgb(88,  91,  112),
                error:       Color::Rgb(243, 139, 168),
                success:     Color::Rgb(166, 227, 161),
                status_fg:   Color::Rgb(205, 214, 244),
                status_bg:   Color::Rgb(24,  24,  37),
            },
            ThemeName::RosePine => Self {
                accent:      Color::Rgb(235, 188, 186), // rose
                muted:       Color::Rgb(110, 106, 134),
                normal_fg:   Color::Rgb(224, 222, 244),
                unread_fg:   Color::Rgb(240, 240, 250),
                selected_bg: Color::Rgb(38,  35,  58),
                border:      Color::Rgb(64,  61,  82),
                error:       Color::Rgb(235, 111, 146),
                success:     Color::Rgb(49,  116, 143),
                status_fg:   Color::Rgb(224, 222, 244),
                status_bg:   Color::Rgb(21,  21,  34),
            },
            ThemeName::SolarizedDark => Self {
                accent:      Color::Rgb(38,  139, 210), // blue
                muted:       Color::Rgb(88,  110, 117),
                normal_fg:   Color::Rgb(131, 148, 150),
                unread_fg:   Color::Rgb(253, 246, 227),
                selected_bg: Color::Rgb(7,   54,  66),
                border:      Color::Rgb(0,   43,  54),
                error:       Color::Rgb(220, 50,  47),
                success:     Color::Rgb(133, 153, 0),
                status_fg:   Color::Rgb(101, 123, 131),
                status_bg:   Color::Rgb(0,   43,  54),
            },
            ThemeName::Everforest => Self {
                accent:      Color::Rgb(131, 192, 146), // green
                muted:       Color::Rgb(115, 127, 101),
                normal_fg:   Color::Rgb(211, 198, 170),
                unread_fg:   Color::Rgb(230, 220, 195),
                selected_bg: Color::Rgb(60,  73,  58),
                border:      Color::Rgb(80,  90,  70),
                error:       Color::Rgb(230, 126, 128),
                success:     Color::Rgb(167, 192, 128),
                status_fg:   Color::Rgb(211, 198, 170),
                status_bg:   Color::Rgb(35,  43,  33),
            },
            ThemeName::Monokai => Self {
                accent:      Color::Rgb(102, 217, 239), // cyan
                muted:       Color::Rgb(117, 113, 94),
                normal_fg:   Color::Rgb(248, 248, 242),
                unread_fg:   Color::Rgb(255, 255, 255),
                selected_bg: Color::Rgb(73,  72,  62),
                border:      Color::Rgb(73,  72,  62),
                error:       Color::Rgb(249, 38,  114),
                success:     Color::Rgb(166, 226, 46),
                status_fg:   Color::Rgb(248, 248, 242),
                status_bg:   Color::Rgb(39,  40,  34),
            },
            ThemeName::Matrix => Self {
                accent:      Color::Rgb(0,   255, 70),  // bright matrix green
                muted:       Color::Rgb(0,   140, 40),  // dim green
                normal_fg:   Color::Rgb(0,   220, 60),  // medium green
                unread_fg:   Color::Rgb(180, 255, 180), // light green for unread
                selected_bg: Color::Rgb(0,   60,  20),  // dark green selection
                border:      Color::Rgb(0,   100, 30),
                error:       Color::Rgb(255, 50,  50),
                success:     Color::Rgb(0,   255, 70),
                status_fg:   Color::Rgb(0,   200, 55),
                status_bg:   Color::Rgb(0,   10,  0),   // near-black green tint
            },
            ThemeName::Gmail => Self {
                accent:      Color::Rgb(66,  133, 244), // Google blue
                muted:       Color::Rgb(128, 134, 139), // Google grey
                normal_fg:   Color::Rgb(32,  33,  36),  // Google dark text
                unread_fg:   Color::Rgb(0,   0,   0),   // bold black for unread
                selected_bg: Color::Rgb(194, 212, 253), // light Google blue selection
                border:      Color::Rgb(218, 220, 224), // Google border grey
                error:       Color::Rgb(234, 67,  53),  // Google red
                success:     Color::Rgb(52,  168, 83),  // Google green
                status_fg:   Color::Rgb(32,  33,  36),
                status_bg:   Color::Rgb(242, 245, 253), // Google light blue-grey
            },
        }
    }
}

// ── Active palette singleton ──────────────────────────────────────────────────

static ACTIVE: Lazy<RwLock<Palette>> =
    Lazy::new(|| RwLock::new(Palette::for_theme(ThemeName::Midnight)));

pub fn apply(name: ThemeName) {
    if let Ok(mut p) = ACTIVE.write() {
        *p = Palette::for_theme(name);
    }
}

// ── Style accessors ───────────────────────────────────────────────────────────

macro_rules! read {
    ($field:ident) => {{
        ACTIVE.read().map(|p| p.$field).unwrap_or(Color::Reset)
    }};
}

pub fn normal() -> Style {
    Style::default().fg(read!(normal_fg))
}

pub fn muted() -> Style {
    Style::default().fg(read!(muted))
}

pub fn accent() -> Style {
    Style::default().fg(read!(accent))
}

pub fn unread() -> Style {
    Style::default().fg(read!(unread_fg)).add_modifier(Modifier::BOLD)
}

pub fn selected() -> Style {
    Style::default().bg(read!(selected_bg)).add_modifier(Modifier::BOLD)
}

pub fn border() -> Style {
    Style::default().fg(read!(border))
}

pub fn border_focused() -> Style {
    Style::default().fg(read!(accent)).add_modifier(Modifier::BOLD)
}

pub fn status() -> Style {
    Style::default()
        .fg(read!(status_fg))
        .bg(read!(status_bg))
}

pub fn error() -> Style {
    Style::default().fg(read!(error)).add_modifier(Modifier::BOLD)
}

pub fn success() -> Style {
    Style::default().fg(read!(success))
}

pub fn header_label() -> Style {
    Style::default().fg(read!(muted)).add_modifier(Modifier::BOLD)
}

pub fn important() -> Style {
    Style::default().fg(Color::Rgb(255, 200, 60)).add_modifier(Modifier::BOLD)
}

pub fn popup_bg() -> Color {
    read!(status_bg)
}

// Back-compat constant used by a few modal backgrounds.
pub use self::_compat::POPUP_BG;
mod _compat {
    use ratatui::style::Color;
    pub const POPUP_BG: Color = Color::Rgb(24, 26, 38);
}
