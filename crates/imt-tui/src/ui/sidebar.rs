//! Sidebar: accounts and their folders.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::Focus;
use crate::theme;

/// Render the sidebar pane.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Sidebar;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused { theme::border_focused() } else { theme::border() })
        .title(Span::styled(" Accounts ", theme::accent()));

    let mut items: Vec<ListItem> = Vec::new();
    for (ai, av) in app.accounts.iter().enumerate() {
        let is_acc_sel = ai == app.sidebar_account_idx;
        let arrow = if av.expanded { "v" } else { ">" };
        let acc_line = Line::from(vec![
            Span::styled(format!(" {} ", arrow), theme::muted()),
            Span::styled(av.account.display_name.clone(), theme::accent()),
            Span::styled(format!("  {}", av.account.address.email), theme::muted()),
        ]);
        items.push(ListItem::new(acc_line));

        if av.expanded {
            for (fi, folder) in av.folders.iter().enumerate() {
                let selected = is_acc_sel && fi == app.sidebar_folder_idx;
                let live_unread = app
                    .data
                    .messages(folder.id)
                    .iter()
                    .filter(|m| m.is_unread())
                    .count() as u32;
                let unread = if live_unread > 0 || folder.message_count == 0 {
                    live_unread
                } else {
                    folder.unread_count
                };
                let mut spans: Vec<Span> = Vec::new();
                spans.push(Span::raw("    "));
                let label_style = if unread > 0 {
                    theme::unread()
                } else {
                    theme::normal()
                };
                spans.push(Span::styled(folder.name.clone(), label_style));
                if unread > 0 {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(format!("({})", unread), theme::accent()));
                }
                let mut line = Line::from(spans);
                if selected {
                    line = line.patch_style(theme::selected());
                }
                items.push(ListItem::new(line));
            }
        }
    }

    let list = List::new(items)
        .block(block)
        .style(Style::default());
    f.render_widget(list, area);
}
