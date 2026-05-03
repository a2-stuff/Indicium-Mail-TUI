//! Account manager modal.

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
    let modal = centered(area, 78, 18);
    let state = match app.accounts_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    f.render_widget(Clear, modal);
    let title = if state.confirm_delete.is_some() {
        " Account Manager - Delete? press d again to confirm "
    } else {
        " Account Manager "
    };
    let block = Block::default()
        .title(Span::styled(title, theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    if app.accounts.is_empty() {
        let p = Paragraph::new("No accounts. Press 'a' to add one.")
            .alignment(Alignment::Center)
            .style(theme::muted());
        f.render_widget(p, rows[0]);
    } else {
        let items: Vec<ListItem> = app
            .accounts
            .iter()
            .map(|av| {
                let acc = &av.account;
                let confirm = state.confirm_delete == Some(acc.id);
                let prefix = if confirm { " ! " } else { "   " };
                let line = format!(
                    "{}{:<22}  {:<32}  imap={}:{}  smtp={}:{}",
                    prefix,
                    acc.display_name,
                    acc.address.email,
                    acc.imap.host,
                    acc.imap.port,
                    acc.smtp.host,
                    acc.smtp.port
                );
                let style = if confirm { theme::error() } else { theme::normal() };
                ListItem::new(Span::styled(line, style))
            })
            .collect();
        let list = List::new(items)
            .highlight_style(theme::selected())
            .highlight_symbol("");
        let mut ls = ListState::default();
        ls.select(Some(state.selected));
        f.render_stateful_widget(list, rows[0], &mut ls);
    }

    let footer = Paragraph::new(Span::styled(
        " j/k or up/down move  Enter/e edit  d delete  a add  Esc/q close ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[2]);
}
