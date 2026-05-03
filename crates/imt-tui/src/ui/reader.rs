//! Message reader pane: headers + body.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(1)])
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

    let headers = Paragraph::new(header_lines).wrap(Wrap { trim: false });
    f.render_widget(headers, chunks[0]);

    let body_text = render_body_text(app, chunks[1].width as usize);
    let body = Paragraph::new(body_text)
        .wrap(Wrap { trim: false })
        .scroll((app.reader_scroll, 0))
        .style(theme::normal());
    f.render_widget(body, chunks[1]);
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
        None => return String::from("(no body)"),
    };
    if let Some(plain) = &body.text_plain {
        return plain.clone();
    }
    if let Some(html) = &body.text_html {
        if app.html_external && body.text_plain.is_none() {
            return String::from("[HTML body - press 'o' to open in browser]");
        }
        let w = width.max(20);
        if let Ok(rendered) = html2text::from_read(html.as_bytes(), w) {
            return rendered;
        }
        return html.clone();
    }
    String::from("(empty)")
}
