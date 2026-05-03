//! Settings modal.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::settings::SettingsField;
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

fn check(b: bool) -> &'static str { if b { "[x]" } else { "[ ]" } }

pub fn render(f: &mut Frame, area: Rect) {
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(" Settings ", theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    f.render_widget(block, area);
}

pub fn render_modal(f: &mut Frame, area: Rect, app: &App) {
    let modal = centered(area, 60, 16);
    let s = match app.settings_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    f.render_widget(Clear, modal);
    let block = Block::default()
        .title(Span::styled(" Settings ", theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    let mark = |sel: bool| if sel { ">" } else { " " };

    let line = |sel: bool, label: &str, value: String| {
        let style = if sel { theme::accent() } else { theme::normal() };
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", mark(sel)), theme::accent()),
            Span::styled(format!("{:<26}", label), style),
            Span::styled(value, theme::muted()),
        ]))
    };

    f.render_widget(
        line(
            s.field == SettingsField::AutoRefreshSecs,
            "Auto-refresh interval (s)",
            format!("[{}]  (0 = off, IDLE still on)", s.auto_refresh_secs.value()),
        ),
        rows[0],
    );
    f.render_widget(
        line(
            s.field == SettingsField::MarkReadOnOpen,
            "Mark as read on open",
            format!("{}", check(s.draft.mark_read_on_open)),
        ),
        rows[1],
    );
    f.render_widget(
        line(
            s.field == SettingsField::HtmlExternal,
            "HTML mail in $BROWSER",
            format!("{}", check(s.draft.html_external)),
        ),
        rows[2],
    );
    f.render_widget(
        line(
            s.field == SettingsField::Browser,
            "Browser command",
            format!("[{}]", s.browser.value()),
        ),
        rows[3],
    );
    f.render_widget(
        line(
            s.field == SettingsField::ShowSnippet,
            "Show preview snippet",
            format!("{}", check(s.draft.show_snippet)),
        ),
        rows[4],
    );

    let info = Paragraph::new(vec![
        Line::from(Span::styled(" Messages are NEVER deleted from the server.", theme::muted())),
        Line::from(Span::styled(" Fetches use BODY.PEEK[] (no Seen flag set).", theme::muted())),
    ]);
    f.render_widget(info, rows[5]);

    let footer = Paragraph::new(Span::styled(
        " Tab next  Shift-Tab prev  Space/Enter toggle  Ctrl-S save  Esc cancel ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[6]);
}
