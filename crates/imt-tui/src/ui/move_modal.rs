//! Move-to-folder modal.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let v = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(h),
        Constraint::Min(0),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(w),
        Constraint::Min(0),
    ])
    .split(v[1]);
    h[1]
}

pub fn render_modal(f: &mut Frame, area: Rect, app: &App) {
    let modal = centered(area, 50, 14);
    let state = match app.move_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    f.render_widget(Clear, modal);
    let block = Block::default()
        .title(Span::styled(" Move to folder ", theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(inner);

    let items: Vec<ListItem> = state
        .folders
        .iter()
        .map(|f| ListItem::new(format!(" {}", f.name)))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut ls = ListState::default();
    ls.select(Some(state.selected));
    f.render_stateful_widget(list, rows[0], &mut ls);

    let footer = Paragraph::new(Span::styled(
        " j/k or up/down  Enter move  Esc cancel ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}
