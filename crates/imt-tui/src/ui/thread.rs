//! Thread (conversation) view modal: the messages in a conversation, with the
//! selected one's body shown below.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

fn centered_pct(area: Rect, pw: u16, ph: u16) -> Rect {
    let mh = (area.height as u32 * (100 - ph as u32) / 200) as u16;
    let mw = (area.width as u32 * (100 - pw as u32) / 200) as u16;
    Rect {
        x: area.x + mw,
        y: area.y + mh,
        width: area.width.saturating_sub(mw * 2),
        height: area.height.saturating_sub(mh * 2),
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let ts = match app.thread_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    let modal = centered_pct(area, 88, 86);
    f.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            format!(" Conversation ({}) ", ts.messages.len()),
            theme::accent().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    // Top: the message list (capped height). Bottom: selected body. Footer.
    let list_h = (ts.messages.len() as u16 + 1).min(inner.height / 2).max(1);
    let rows = Layout::vertical([
        Constraint::Length(list_h),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let items: Vec<ListItem> = ts
        .messages
        .iter()
        .map(|m| {
            let unread = if m.is_unread() { "●" } else { " " };
            let flag = if m.is_flagged() { "★" } else { " " };
            let clip = if app.message_has_attachments(m) { "📎" } else { "  " };
            let from = m
                .headers
                .from
                .first()
                .map(|a| a.format())
                .unwrap_or_default();
            let date = m.headers.date.format("%Y-%m-%d %H:%M").to_string();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {}{} ", unread, flag), theme::accent()),
                Span::styled(format!("{:<28}", truncate(&from, 28)), theme::normal()),
                Span::styled(format!("  {}", date), theme::muted()),
                Span::styled(format!("  {}", clip), theme::accent()),
            ]))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("");
    let mut state = ListState::default();
    state.select(Some(ts.selected));
    f.render_stateful_widget(list, rows[0], &mut state);

    // Selected message body.
    let sel_has_attachments = ts
        .messages
        .get(ts.selected)
        .map(|m| app.message_has_attachments(m))
        .unwrap_or(false);
    let body_text = ts
        .messages
        .get(ts.selected)
        .map(|m| {
            let subj = if m.headers.subject.is_empty() {
                "(no subject)".to_string()
            } else {
                m.headers.subject.clone()
            };
            let full = app.data.message_body(m.id).or_else(|| m.body.clone());
            let body = full
                .as_ref()
                .map(crate::ai::body_to_text)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "Loading message...".to_string());
            let att_line = match full.as_ref() {
                Some(b) if !b.attachments.is_empty() => {
                    let names: Vec<String> = b
                        .attachments
                        .iter()
                        .map(|a| a.filename.clone())
                        .collect();
                    format!("📎 {} attachment(s): {}\n\n", names.len(), names.join(", "))
                }
                _ => String::new(),
            };
            format!("{}\n\n{}{}", subj, att_line, body)
        })
        .unwrap_or_default();
    let body = Paragraph::new(body_text)
        .wrap(Wrap { trim: false })
        .scroll((ts.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(theme::border()),
        )
        .style(theme::normal());
    f.render_widget(body, rows[1]);

    let footer_text = if sel_has_attachments {
        " [↑↓] select message   [PgUp/PgDn] scroll   [a] view attachments   [Esc] close "
    } else {
        " [↑↓] select message   [PgUp/PgDn] scroll   [Esc] close "
    };
    let footer = Paragraph::new(Span::styled(footer_text, theme::muted())).alignment(Alignment::Center);
    f.render_widget(footer, rows[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", t)
    }
}
