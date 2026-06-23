//! Top menu bar, email-actions bar, and the dropdown overlay.

use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::Mode;
use crate::menu::{ACTIONS, MENUS};
use crate::theme;

/// Width in cells of a menu-bar label segment (" Label ").
fn seg_width(label: &str) -> u16 {
    label.chars().count() as u16 + 2
}

/// Render the top application menu bar (row 0).
pub fn render_menu_bar(f: &mut Frame, area: Rect, app: &App) {
    let in_menu = app.mode == Mode::Menu;
    let ms = app.menu_state;
    let active_row0 = in_menu && ms.map(|m| m.row == 0).unwrap_or(false);
    let sel = ms.map(|m| m.col).unwrap_or(usize::MAX);

    let mut spans: Vec<Span> = Vec::new();
    for (i, m) in MENUS.iter().enumerate() {
        let selected = active_row0 && i == sel;
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

/// Render the email-actions bar (row 1).
pub fn render_actions_bar(f: &mut Frame, area: Rect, app: &App) {
    let in_menu = app.mode == Mode::Menu;
    let ms = app.menu_state;
    let active_row1 = in_menu && ms.map(|m| m.row == 1).unwrap_or(false);
    let sel = ms.map(|m| m.col).unwrap_or(usize::MAX);

    let mut spans: Vec<Span> = Vec::new();
    for (i, a) in ACTIONS.iter().enumerate() {
        let selected = active_row1 && i == sel;
        if selected {
            spans.push(Span::styled(
                format!(" {} ", a.label),
                theme::selected().add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(format!(" {}", a.label), theme::normal()));
            spans.push(Span::styled(format!("({}) ", a.key_hint), theme::muted()));
        }
    }
    let p = Paragraph::new(Line::from(spans)).style(theme::status());
    f.render_widget(p, area);
}

/// Render the dropdown for the currently-open top menu, if any.
pub fn render_dropdown(f: &mut Frame, menu_bar: Rect, app: &App) {
    if app.mode != Mode::Menu {
        return;
    }
    let ms = match app.menu_state {
        Some(m) if m.row == 0 && m.open => m,
        _ => return,
    };
    let menu = match MENUS.get(ms.col) {
        Some(m) if !m.items.is_empty() => m,
        _ => return,
    };

    // X offset = sum of preceding label segment widths.
    let mut x = menu_bar.x;
    for m in MENUS.iter().take(ms.col) {
        let caret = if m.items.is_empty() { 0 } else { 2 };
        x += seg_width(m.label) + caret;
    }

    let item_w = menu
        .items
        .iter()
        .map(|it| it.label.chars().count() + it.key_hint.chars().count() + 5)
        .max()
        .unwrap_or(12) as u16;

    // Clamp the dropdown to the frame so it never indexes outside the buffer
    // (important on very small terminals).
    let full = f.area();
    let width = item_w.max(16).min(full.width);
    let height = (menu.items.len() as u16 + 2).min(full.height.saturating_sub(menu_bar.y + 1));
    if width == 0 || height == 0 {
        return;
    }
    let y = menu_bar.y + 1;
    let x = x.min(full.right().saturating_sub(width));
    let area = Rect { x, y, width, height };

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused())
        .title(Span::styled(format!(" {} ", menu.label), theme::accent()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = menu
        .items
        .iter()
        .map(|it| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<width$}", it.label, width = (item_w as usize).saturating_sub(it.key_hint.chars().count() + 4)), theme::normal()),
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
