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
/// and the status/footer line at the bottom.
pub fn root_layout(area: Rect) -> RootChunks {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // menu bar
            Constraint::Min(1),    // main body
            Constraint::Length(1), // status / footer
        ])
        .split(area);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(35),
            Constraint::Percentage(45),
        ])
        .split(outer[1]);
    RootChunks {
        menu_bar: outer[0],
        sidebar: main[0],
        list: main[1],
        reader: main[2],
        status: outer[2],
    }
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
