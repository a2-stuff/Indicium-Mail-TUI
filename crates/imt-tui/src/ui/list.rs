//! Message list pane.

use chrono::{DateTime, Datelike, Local, Utc};
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::keymap::Focus;
use crate::theme;

/// Format a date relative to now: today => HH:MM, this week => weekday, else Mon DD.
pub fn format_date(date: DateTime<Utc>) -> String {
    let local: DateTime<Local> = date.into();
    let now = Local::now();
    if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else if (now - local).num_days() < 7 {
        local.format("%a").to_string()
    } else if local.year() == now.year() {
        local.format("%b %d").to_string()
    } else {
        local.format("%Y-%m").to_string()
    }
}

fn truncate_to(s: &str, width: usize) -> String {
    if s.width() <= width {
        return format!("{:width$}", s, width = width);
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw + 1 > width {
            break;
        }
        out.push(ch);
        w += cw;
    }
    while w < width {
        out.push(' ');
        w += 1;
    }
    out
}

/// Render the message list pane.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::MessageList;
    let title = match app.current_folder() {
        Some(folder) => {
            let unread = app.messages.iter().filter(|m| m.is_unread()).count();
            format!(" {} ({} unread) ", folder.name, unread)
        }
        None => " (no folder) ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused { theme::border_focused() } else { theme::border() })
        .title(Span::styled(title, theme::accent()));

    if app.messages.is_empty() {
        let text = if app.is_busy() {
            format!("{}  Loading messages...", app.spinner_frame())
        } else {
            "(no messages)".to_string()
        };
        let style = if app.is_busy() { theme::accent() } else { theme::muted() };
        let p = Paragraph::new(Line::from(Span::styled(text, style)))
            .alignment(Alignment::Center)
            .block(block);
        f.render_widget(p, area);
        return;
    }

    let show_snippet = app.settings.show_snippet;
    let rows: Vec<Row> = app
        .messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let unread = m.is_unread();
            let flagged = m.is_flagged();
            let answered = m.is_answered();
            let indicator = if flagged {
                "*"
            } else if unread {
                "\u{2022}"
            } else if answered {
                ">"
            } else {
                " "
            };
            let from = m
                .headers
                .from
                .first()
                .map(|a| a.name.clone().unwrap_or_else(|| a.email.clone()))
                .unwrap_or_default();
            let from = truncate_to(&from, 18);
            let subject = m.headers.subject.clone();
            let date = format_date(m.headers.date);

            let row_style = if unread { theme::unread() } else { theme::normal() };
            let ind_style = if flagged {
                theme::error()
            } else if unread {
                theme::accent()
            } else {
                theme::muted()
            };

            let subject_cell = if show_snippet && !m.snippet.is_empty() {
                Cell::from(Text::from(vec![
                    Line::from(Span::styled(subject, row_style)),
                    Line::from(Span::styled(m.snippet.clone(), theme::muted())),
                ]))
            } else {
                Cell::from(Line::from(vec![Span::styled(subject, row_style)]))
            };

            let height = if show_snippet && !m.snippet.is_empty() { 2 } else { 1 };

            let mut row = Row::new(vec![
                Cell::from(Span::styled(indicator.to_string(), ind_style)),
                Cell::from(Span::styled(from, row_style)),
                subject_cell,
                Cell::from(Span::styled(date, theme::muted())),
            ])
            .height(height);
            if i == app.message_idx {
                row = row.style(theme::selected());
            }
            row
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(18),
        Constraint::Min(10),
        Constraint::Length(7),
    ];

    let table = Table::new(rows, widths).block(block).column_spacing(1);
    f.render_widget(table, area);
}
