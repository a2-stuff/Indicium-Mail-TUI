//! Toast notification overlay - appears bottom-right, auto-clears.

use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

/// Render the transient toast notification when `app.status` is non-empty.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    if app.status.is_empty() {
        return;
    }

    let msg = &app.status;

    // Cap content width at 46 cols; height grows with wrapped lines.
    let content_w: u16 = msg.chars().count().min(46) as u16;
    let toast_w = content_w + 4; // 2 border + 2 side padding
    let content_lines = ((msg.chars().count() as u16).saturating_sub(1) / content_w) + 1;
    let toast_h = content_lines + 2; // border top/bottom

    // Position: bottom-right corner, one row above the status bar.
    let x = area.width.saturating_sub(toast_w + 2);
    let y = area.height.saturating_sub(toast_h + 2); // 1 for status bar + 1 gap

    if area.width < toast_w + 4 || area.height < toast_h + 3 {
        return; // terminal too small
    }

    let toast_area = Rect {
        x: area.x + x,
        y: area.y + y,
        width: toast_w,
        height: toast_h,
    };

    f.render_widget(Clear, toast_area);

    let is_error = msg.to_lowercase().contains("fail")
        || msg.to_lowercase().contains("error")
        || msg.to_lowercase().contains("invalid");

    let border_style = if is_error { theme::error() } else { theme::accent() };
    let text_style = if is_error {
        theme::error()
    } else {
        theme::normal().add_modifier(Modifier::BOLD)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let p = Paragraph::new(Line::from(Span::styled(msg.as_str(), text_style)))
        .block(block)
        .wrap(Wrap { trim: true });

    f.render_widget(p, toast_area);
}
