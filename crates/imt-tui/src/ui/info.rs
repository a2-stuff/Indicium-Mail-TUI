//! App info modal.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::theme;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB: &str = "https://github.com/a2-stuff/Indicium-Mail-TUI";
const TWITTER: &str = "https://x.com/not_jarod";

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

pub fn render(f: &mut Frame, area: Rect) {
    let modal = centered(area, 62, 16);
    f.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(" About Indicium Mail TUI ", theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());

    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    let lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Indicium Mail TUI", theme::accent().add_modifier(Modifier::BOLD)),
            Span::styled(format!("  v{}", VERSION), theme::muted()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  A fast, keyboard-driven TUI email client with",
            theme::normal(),
        )),
        Line::from(Span::styled(
            "  IMAP / SMTP support, HTML rendering, full-text",
            theme::normal(),
        )),
        Line::from(Span::styled(
            "  search, and multi-account management.",
            theme::normal(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  GitHub   ", theme::muted()),
            Span::styled(GITHUB, theme::accent()),
        ]),
        Line::from(vec![
            Span::styled("  Twitter  ", theme::muted()),
            Span::styled(TWITTER, theme::accent()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  MIT License - 2026 Indicium contributors",
            theme::muted(),
        )),
    ];

    let p = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    f.render_widget(p, rows[0]);

    let footer = Paragraph::new(Span::styled(
        " Esc / q / i  close ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}
