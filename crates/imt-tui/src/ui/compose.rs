//! Compose modal popup.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::ComposeField;
use crate::theme;
use crate::ui::layout::centered;

/// Render the compose modal.
pub fn render(f: &mut Frame, full: Rect, app: &mut App) {
    let area = centered(full, 80, 80);
    f.render_widget(Clear, area);

    // Read app-level flags before the mutable borrow of `app.compose` below.
    let ai_generating = app.ai_generating;
    let ai_spinner = app.spinner_frame();

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused())
        .title(Span::styled(" Compose ", theme::accent()))
        .style(Style::default().bg(theme::POPUP_BG));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    let compose = match app.compose.as_mut() {
        Some(c) => c,
        None => return,
    };

    let from_label = app
        .accounts
        .get(compose.from_idx)
        .map(|a| a.account.address.format())
        .unwrap_or_default();

    render_field(f, chunks[0], "From", &format!("< {from_label} >"), compose.field == ComposeField::From, false);
    render_input(f, chunks[1], "To", compose.to.value(), compose.field == ComposeField::To);
    render_input(f, chunks[2], "Cc", compose.cc.value(), compose.field == ComposeField::Cc);
    render_input(f, chunks[3], "Bcc", compose.bcc.value(), compose.field == ComposeField::Bcc);
    render_input(f, chunks[4], "Subject", compose.subject.value(), compose.field == ComposeField::Subject);

    let body_focused = compose.field == ComposeField::Body;
    let body_title = if ai_generating {
        format!(" Body  {ai_spinner} generating reply... ")
    } else {
        " Body ".to_string()
    };
    compose.body.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if body_focused { theme::border_focused() } else { theme::border() })
            .title(Span::styled(body_title, if body_focused { theme::accent() } else { theme::muted() })),
    );
    f.render_widget(&compose.body, chunks[5]);

    let attach_label = if compose.draft.attachments.is_empty() {
        "Ctrl-A to add files".to_string()
    } else {
        let names = compose.draft.attachments.iter()
            .map(|a| a.filename.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{}]  {}  (Ctrl-A add more  Backspace remove last)", compose.draft.attachments.len(), names)
    };
    render_field(f, chunks[6], "Attachments", &attach_label, compose.field == ComposeField::Attachments, true);

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Ctrl-S", theme::accent()),
        Span::styled(" send  ", theme::muted()),
        Span::styled("Ctrl-D", theme::accent()),
        Span::styled(" save draft  ", theme::muted()),
        Span::styled("Ctrl-G", theme::accent()),
        Span::styled(" AI reply  ", theme::muted()),
        Span::styled("Tab", theme::accent()),
        Span::styled(" next field  ", theme::muted()),
        Span::styled("Esc", theme::accent()),
        Span::styled(" cancel", theme::muted()),
    ]))
    .style(theme::status());
    f.render_widget(footer, chunks[7]);
}

fn render_field(f: &mut Frame, area: Rect, label: &str, value: &str, focused: bool, secondary: bool) {
    let style = if focused { theme::border_focused() } else { theme::border() };
    let title_style = if focused { theme::accent() } else { theme::muted() };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style)
        .title(Span::styled(format!(" {label} "), title_style));
    let value_style = if secondary { theme::muted() } else { theme::normal() };
    let p = Paragraph::new(Span::styled(value.to_string(), value_style)).block(block);
    f.render_widget(p, area);
}

fn render_input(f: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    render_field(f, area, label, value, focused, false);
}
