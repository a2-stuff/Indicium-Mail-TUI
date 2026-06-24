//! Top menu bar, email-actions bar, and the dropdown overlay.

use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::Mode;
use crate::menu::MENUS;
use crate::theme;

/// Width in cells of a menu-bar label segment (" Label ").
fn seg_width(label: &str) -> u16 {
    label.chars().count() as u16 + 2
}

/// Render the top application menu bar.
pub fn render_menu_bar(f: &mut Frame, area: Rect, app: &App) {
    let in_menu = app.mode == Mode::Menu;
    let sel = app.menu_state.map(|m| m.col).unwrap_or(usize::MAX);

    let mut spans: Vec<Span> = Vec::new();
    for (i, m) in MENUS.iter().enumerate() {
        let selected = in_menu && i == sel;
        let caret = if m.items.is_empty() { "" } else { " \u{25be}" }; // ▾
        let text = format!(" {}{} ", m.label, caret);
        let style = if selected {
            theme::selected().add_modifier(Modifier::BOLD)
        } else {
            theme::normal()
        };
        spans.push(Span::styled(text, style));
    }
    spans.push(Span::styled("   [F10] menu", theme::muted()));
    let p = Paragraph::new(Line::from(spans)).style(theme::status());
    f.render_widget(p, area);
}

/// X offset of the menu-bar segment for menu `col` (start column).
pub fn segment_x(menu_bar: Rect, col: usize) -> u16 {
    let mut x = menu_bar.x;
    for m in MENUS.iter().take(col) {
        let caret = if m.items.is_empty() { 0 } else { 2 };
        x += seg_width(m.label) + caret;
    }
    x
}

/// Geometry of the dropdown box for menu `col`, clamped to `full`. Shared by the
/// renderer and mouse hit-testing so clicks land on the right item. None if the
/// menu has no items or there is no room.
pub fn dropdown_rect(menu_bar: Rect, full: Rect, col: usize) -> Option<Rect> {
    let menu = MENUS.get(col).filter(|m| !m.items.is_empty())?;
    let x = segment_x(menu_bar, col);
    let item_w = menu
        .items
        .iter()
        .map(|it| it.label.chars().count() + it.key_hint.chars().count() + 5)
        .max()
        .unwrap_or(12) as u16;
    // Clamp the dropdown to the frame so it never indexes outside the buffer
    // (important on very small terminals).
    let width = item_w.max(16).min(full.width);
    let height = (menu.items.len() as u16 + 2).min(full.height.saturating_sub(menu_bar.y + 1));
    if width == 0 || height == 0 {
        return None;
    }
    let y = menu_bar.y + 1;
    let x = x.min(full.right().saturating_sub(width));
    Some(Rect { x, y, width, height })
}

/// Render the dropdown for the currently-open top menu, if any.
pub fn render_dropdown(f: &mut Frame, menu_bar: Rect, app: &App) {
    if app.mode != Mode::Menu {
        return;
    }
    let ms = match app.menu_state {
        Some(m) if m.open => m,
        _ => return,
    };
    let menu = match MENUS.get(ms.col) {
        Some(m) if !m.items.is_empty() => m,
        _ => return,
    };

    let full = f.area();
    let area = match dropdown_rect(menu_bar, full, ms.col) {
        Some(a) => a,
        None => return,
    };

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused())
        .title(Span::styled(format!(" {} ", menu.label), theme::accent()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let item_w = inner.width as usize;
    let items: Vec<ListItem> = menu
        .items
        .iter()
        .map(|it| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<width$}", it.label, width = item_w.saturating_sub(it.key_hint.chars().count() + 3)), theme::normal()),
                Span::styled(format!("{} ", it.key_hint), theme::muted()),
            ]))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("");
    let mut state = ListState::default();
    state.select(Some(ms.item));
    f.render_stateful_widget(list, inner, &mut state);
}
