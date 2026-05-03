//! Theme: colors and styles for the TUI.
//!
//! All visible text routes through here so a future light theme is one-line change.

use ratatui::style::{Color, Modifier, Style};

/// Accent / focus highlight color.
pub const ACCENT: Color = Color::Rgb(124, 156, 255);
/// Muted / secondary text.
pub const MUTED: Color = Color::Rgb(120, 120, 130);
/// Unread message indicator.
pub const UNREAD: Color = Color::Rgb(240, 240, 250);
/// Selected row background.
pub const SELECTED_BG: Color = Color::Rgb(48, 56, 86);
/// Default border color.
pub const BORDER: Color = Color::Rgb(80, 80, 100);
/// Error / destructive color.
pub const ERROR: Color = Color::Rgb(244, 102, 102);
/// Success color.
pub const SUCCESS: Color = Color::Rgb(120, 210, 140);
/// Background for popups / modals.
pub const POPUP_BG: Color = Color::Rgb(24, 26, 38);

/// Style for normal body text.
pub fn normal() -> Style {
    Style::default().fg(Color::Rgb(210, 210, 220))
}

/// Style for muted secondary text.
pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

/// Style for accent text.
pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

/// Style for unread (bold) text.
pub fn unread() -> Style {
    Style::default().fg(UNREAD).add_modifier(Modifier::BOLD)
}

/// Style for selected rows.
pub fn selected() -> Style {
    Style::default().bg(SELECTED_BG).add_modifier(Modifier::BOLD)
}

/// Style for borders of unfocused panes.
pub fn border() -> Style {
    Style::default().fg(BORDER)
}

/// Style for borders of the focused pane.
pub fn border_focused() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

/// Style for status bar text.
pub fn status() -> Style {
    Style::default().fg(Color::Rgb(180, 180, 200)).bg(Color::Rgb(32, 34, 48))
}

/// Style for error text.
pub fn error() -> Style {
    Style::default().fg(ERROR).add_modifier(Modifier::BOLD)
}

/// Style for success text.
pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

/// Style for header field labels in the reader.
pub fn header_label() -> Style {
    Style::default().fg(MUTED).add_modifier(Modifier::BOLD)
}
