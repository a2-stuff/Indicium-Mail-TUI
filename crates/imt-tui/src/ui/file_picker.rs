//! File browser / picker modal popup.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

/// Render the file picker modal.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let s = match app.file_picker.as_ref() {
        Some(s) => s,
        None => return,
    };

    // Centered modal: 85% height, 70% width
    let v = Layout::vertical([
        Constraint::Min(0),
        Constraint::Percentage(85),
        Constraint::Min(0),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Percentage(70),
        Constraint::Min(0),
    ])
    .split(v[1]);
    let modal = h[1];

    f.render_widget(Clear, modal);

    let dir_str = s.current_dir.to_string_lossy();
    let title = if s.picked.is_empty() {
        format!(" File Browser - {} ", dir_str)
    } else {
        format!(" File Browser - {} ({} selected) ", dir_str, s.picked.len())
    };

    let block = Block::default()
        .title(Span::styled(title, theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());

    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(inner);

    let items: Vec<ListItem> = s
        .entries
        .iter()
        .map(|e| {
            let picked = s.picked.contains(&e.path);
            let indicator = if e.is_dir {
                "[d]"
            } else if picked {
                "[x]"
            } else {
                "[ ]"
            };
            let size_str = if e.is_dir {
                String::new()
            } else {
                format!("  {}", format_size(e.size))
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", indicator),
                    if e.is_dir {
                        theme::muted()
                    } else if picked {
                        theme::accent()
                    } else {
                        theme::normal()
                    },
                ),
                Span::styled(
                    e.name.clone(),
                    if e.is_dir {
                        theme::accent().add_modifier(Modifier::BOLD)
                    } else if picked {
                        theme::unread()
                    } else {
                        theme::normal()
                    },
                ),
                Span::styled(size_str, theme::muted()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("");
    let mut ls = ListState::default();
    ls.select(Some(s.selected_idx));
    f.render_stateful_widget(list, rows[0], &mut ls);

    let footer = Paragraph::new(Span::styled(
        " [↑↓] move  [Space] select  [Enter] confirm  [Backspace] up  [Esc] cancel",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.0}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.1}G", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}
