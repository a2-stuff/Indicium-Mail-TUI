//! Status bar at the bottom.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::keymap::Mode;
use crate::theme;

/// Render the status bar.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let mode = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Compose => "COMPOSE",
        Mode::Search => "SEARCH",
        Mode::Help => "HELP",
        Mode::Onboarding => "ONBOARDING",
        Mode::Settings => "SETTINGS",
        Mode::Accounts => "ACCOUNTS",
        Mode::Move => "MOVE",
        Mode::Info => "INFO",
    };
    let hints = match app.mode {
        Mode::Normal => "[c] compose  [Enter] open  [m] accounts  [,] settings  [/] search  [?] help  [q] quit  [i] info",
        Mode::Compose => "Tab next  Ctrl-S send  Ctrl-D save  Esc cancel",
        Mode::Search => "type to search  Enter jump  Esc cancel",
        Mode::Help => "Esc / ? close",
        Mode::Onboarding => "Tab next  Shift-Tab prev  Ctrl-S save  Esc cancel  Left/Right cycle TLS",
        Mode::Settings => "Tab next  Space toggle  Ctrl-S save  Esc cancel",
        Mode::Accounts => "j/k move  Enter edit  d delete  a add  Esc close",
        Mode::Move => "j/k move  Enter select  Esc cancel",
        Mode::Info => "Esc / q / i  close",
    };
    let (sync_text, sync_style) = if app.is_busy() {
        (format!(" {} {} ", app.spinner_frame(), app.backend_status), theme::accent())
    } else if app.backend_status.is_empty() {
        (" ready ".to_string(), theme::success())
    } else {
        (format!(" {} ", app.backend_status), theme::success())
    };
    let line = Line::from(vec![
        Span::styled(format!(" {} ", mode), theme::accent()),
        Span::styled(format!(" {} ", hints), theme::muted()),
        Span::styled(sync_text, sync_style),
    ]);
    let p = Paragraph::new(line).style(theme::status());
    f.render_widget(p, area);
}
