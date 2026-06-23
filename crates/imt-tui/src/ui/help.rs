//! Help overlay.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::theme;
use crate::ui::layout::centered;

/// Render the help overlay.
pub fn render(f: &mut Frame, full: Rect) {
    let area = centered(full, 70, 75);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused())
        .title(Span::styled(" Help ", theme::accent()))
        .style(Style::default().bg(theme::POPUP_BG));

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled("Navigation", theme::accent())));
    lines.extend(group(&[
        ("j / k", "down / up"),
        ("g / G", "top / bottom"),
        ("PgDn / PgUp", "page down / up"),
        ("Tab / Shift-Tab", "next / prev pane"),
        ("} / {", "next / prev account"),
        ("Enter", "open selected"),
        ("Esc", "back / cancel"),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Mail", theme::accent())));
    lines.extend(group(&[
        ("c", "compose"),
        ("r / R", "reply / reply-all"),
        ("f", "forward"),
        ("t", "view conversation thread"),
        ("a", "attachments (reader focus)"),
        ("u", "toggle read / unread"),
        ("s", "★ mark / unmark important"),
        ("v", "move to folder"),
        ("d", "delete (moves to Trash)"),
        ("E", "empty Trash (only in Trash folder)"),
        ("/", "search"),
        ("o", "open HTML body in browser"),
        ("A", "add account (sidebar focus)"),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Compose", theme::accent())));
    lines.extend(group(&[
        ("Tab / Shift-Tab", "next / prev field"),
        ("Ctrl-S", "send"),
        ("Ctrl-D", "save draft"),
        ("Ctrl-A", "add attachment"),
        ("Ctrl-G", "AI reply (draft / refine via Claude, Gemini, or Codex)"),
        ("Esc", "cancel"),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("App", theme::accent())));
    lines.extend(group(&[
        ("m", "account manager"),
        (",", "settings"),
        ("i", "about / info"),
        ("Ctrl-R / Ctrl-L", "refresh"),
        ("?", "toggle help"),
        ("q", "quit"),
    ]));

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn group(items: &[(&str, &str)]) -> Vec<Line<'static>> {
    items
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("  {:<18}", k), theme::accent()),
                Span::styled(v.to_string(), theme::normal()),
            ])
        })
        .collect()
}
