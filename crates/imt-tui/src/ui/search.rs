//! Inline search bar at the bottom of the screen.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme;

/// Render the search bar over the status row.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let line = Line::from(vec![
        Span::styled(" / ", theme::accent()),
        Span::styled(app.search_input.value().to_string(), theme::normal()),
        Span::styled(format!("  ({} matches)", app.search_results.len()), theme::muted()),
        Span::styled("  Enter: jump  Esc: cancel", theme::muted()),
    ]);
    let p = Paragraph::new(line).style(theme::status());
    f.render_widget(p, area);
}
