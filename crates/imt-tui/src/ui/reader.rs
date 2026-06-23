//! Message reader pane: headers + body + attachments.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::Focus;
use crate::theme;

/// Render the reader pane.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Reader;
    let title = app
        .current_message()
        .map(|m| format!(" {} ", m.headers.subject))
        .unwrap_or_else(|| " (no message) ".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused { theme::border_focused() } else { theme::border() })
        .title(Span::styled(title, theme::accent()));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let msg = match app.current_message() {
        Some(m) => m,
        None => {
            let p = Paragraph::new("Select a message to read.").style(theme::muted());
            f.render_widget(p, inner);
            return;
        }
    };

    // Determine attachment count for layout
    let attach_count = app.current_body.as_ref().map(|b| b.attachments.len()).unwrap_or(0);
    let attach_height = if attach_count > 0 { (attach_count as u16 + 2).min(6) } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(1),
            Constraint::Length(attach_height),
        ])
        .split(inner);

    let from_str = msg
        .headers
        .from
        .iter()
        .map(|a| a.format())
        .collect::<Vec<_>>()
        .join(", ");
    let to_str = msg.headers.to.iter().map(|a| a.format()).collect::<Vec<_>>().join(", ");
    let cc_str = msg.headers.cc.iter().map(|a| a.format()).collect::<Vec<_>>().join(", ");
    let date_str = msg.headers.date.format("%Y-%m-%d %H:%M").to_string();

    let mut header_lines: Vec<Line> = vec![
        header_line("From:", &from_str),
        header_line("To:", &to_str),
    ];
    if !cc_str.is_empty() {
        header_lines.push(header_line("Cc:", &cc_str));
    }
    header_lines.push(header_line("Subject:", &msg.headers.subject));
    header_lines.push(header_line("Date:", &date_str));
    if app.current_thread_count > 1 {
        header_lines.push(Line::from(vec![
            Span::styled(format!("{:<9}", "Thread:"), theme::muted()),
            Span::styled(
                format!("{} messages - press [t] to view", app.current_thread_count),
                theme::accent(),
            ),
        ]));
    }

    let headers = Paragraph::new(header_lines).wrap(Wrap { trim: false });
    f.render_widget(headers, chunks[0]);

    let body_text = render_body_text(app, chunks[1].width as usize);
    let body = Paragraph::new(body_text)
        .wrap(Wrap { trim: false })
        .scroll((app.reader_scroll, 0))
        .style(theme::normal());
    f.render_widget(body, chunks[1]);

    if attach_count > 0 {
        if let Some(body) = app.current_body.as_ref() {
            let attach_block = Block::default()
                .borders(Borders::TOP)
                .border_style(theme::border())
                .title(Span::styled(
                    format!(" {} Attachment{} ", attach_count, if attach_count == 1 { "" } else { "s" }),
                    theme::muted().add_modifier(Modifier::BOLD),
                ));
            let attach_inner = attach_block.inner(chunks[2]);
            f.render_widget(attach_block, chunks[2]);

            let lines: Vec<Line> = body.attachments.iter().map(|a| {
                let size_str = format_size(a.size);
                Line::from(vec![
                    Span::styled("  ", theme::normal()),
                    Span::styled("", theme::accent()),
                    Span::styled(
                        if a.filename.is_empty() { format!("  {} ({})", a.mime_type, size_str) }
                        else { format!("  {} ", a.filename) },
                        theme::normal(),
                    ),
                    Span::styled(
                        if a.filename.is_empty() { String::new() }
                        else { format!(" {}  {}", size_str, a.mime_type) },
                        theme::muted(),
                    ),
                ])
            }).collect();
            let p = Paragraph::new(lines);
            f.render_widget(p, attach_inner);
        }
    }
}

fn header_line<'a>(label: &'a str, value: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), theme::header_label()),
        Span::styled(value.to_string(), theme::normal()),
    ])
}

fn render_body_text(app: &App, width: usize) -> String {
    let body = match app.current_body.as_ref() {
        Some(b) => b,
        None => return String::from("(loading...)"),
    };
    if let Some(plain) = &body.text_plain {
        return plain.clone();
    }
    if let Some(html) = &body.text_html {
        let w = width.max(20);
        if let Ok(rendered) = html2text::from_read(html.as_bytes(), w) {
            return rendered;
        }
        return html.clone();
    }
    String::from("(empty)")
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.0}K", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}M", bytes as f64 / 1024.0 / 1024.0)
    }
}
