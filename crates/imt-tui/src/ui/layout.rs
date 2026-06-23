//! Three-pane layout split.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The primary areas of the screen.
pub struct RootChunks {
    pub menu_bar: Rect,
    pub sidebar: Rect,
    pub list: Rect,
    pub reader: Rect,
    pub status: Rect,
}

/// Compute the root layout: menu bar on top, the three-pane body in the middle,
/// and the status/footer line at the bottom. `sidebar_frac`/`list_frac` are the
/// width fractions of the first two panes (reader takes the remainder).
pub fn root_layout(area: Rect, sidebar_frac: f32, list_frac: f32) -> RootChunks {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // menu bar
            Constraint::Min(1),    // main body
            Constraint::Length(1), // status / footer
        ])
        .split(area);

    let body = outer[1];
    let (sidebar_w, list_w, reader_w) = pane_widths(body.width, sidebar_frac, list_frac);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_w),
            Constraint::Length(list_w),
            Constraint::Length(reader_w),
        ])
        .split(body);
    RootChunks {
        menu_bar: outer[0],
        sidebar: main[0],
        list: main[1],
        reader: main[2],
        status: outer[2],
    }
}

/// Resolve the three pane widths for a body of `width` cells, enforcing
/// minimum widths so no pane collapses. Shared by layout + mouse hit-testing.
pub fn pane_widths(width: u16, sidebar_frac: f32, list_frac: f32) -> (u16, u16, u16) {
    if width < 24 {
        // Too narrow to enforce minimums - split roughly in thirds.
        let a = width / 3;
        return (a, a, width.saturating_sub(a * 2));
    }
    let total = width as f32;
    let sidebar_w = ((total * sidebar_frac).round() as u16).clamp(8, width.saturating_sub(16));
    let list_w = ((total * list_frac).round() as u16).clamp(8, width.saturating_sub(sidebar_w + 8));
    let reader_w = width.saturating_sub(sidebar_w + list_w);
    (sidebar_w, list_w, reader_w)
}

/// Center a popup with the given percentage of the area.
pub fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_h = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_h[1])[1]
}
