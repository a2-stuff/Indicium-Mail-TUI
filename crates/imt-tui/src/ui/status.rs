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
    };
    let hints = match app.mode {
        Mode::Normal => "j/k move  Enter open  c compose  A add account  o open html  / search  ? help  q quit",
        Mode::Compose => "Tab next  Ctrl-S send  Ctrl-D save  Esc cancel",
        Mode::Search => "type to search  Enter jump  Esc cancel",
        Mode::Help => "Esc / ? close",
        Mode::Onboarding => "Tab next  Shift-Tab prev  Ctrl-S save  Esc cancel  Left/Right cycle TLS",
    };
    let sync = "[idle]";
    let line = Line::from(vec![
        Span::styled(format!(" {} ", mode), theme::accent()),
        Span::styled(format!(" {} ", hints), theme::muted()),
        Span::styled(format!(" {} ", app.status), theme::normal()),
        Span::styled(format!(" {sync}"), theme::success()),
    ]);
    let p = Paragraph::new(line).style(theme::status());
    f.render_widget(p, area);
}
