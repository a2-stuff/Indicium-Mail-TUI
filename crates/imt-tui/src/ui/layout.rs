//! Three-pane layout split.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The four primary areas of the screen.
pub struct RootChunks {
    pub sidebar: Rect,
    pub list: Rect,
    pub reader: Rect,
    pub status: Rect,
}

/// Compute the root layout for an area.
pub fn root_layout(area: Rect) -> RootChunks {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(35),
            Constraint::Percentage(45),
        ])
        .split(outer[0]);
    RootChunks {
        sidebar: main[0],
        list: main[1],
        reader: main[2],
        status: outer[1],
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
