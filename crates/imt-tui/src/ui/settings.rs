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

fn check(b: bool) -> &'static str {
    if b { "[x]" } else { "[ ]" }
}

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
    let modal = centered(area, 66, 24);
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
        Constraint::Length(2), // auto-refresh
        Constraint::Length(2), // mark read
        Constraint::Length(2), // html external
        Constraint::Length(2), // browser
        Constraint::Length(2), // show snippet
        Constraint::Length(2), // theme
        Constraint::Length(2), // ai provider
        Constraint::Length(2), // ai model
        Constraint::Min(0),    // info
        Constraint::Length(1), // footer
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
            check(s.draft.mark_read_on_open).to_string(),
        ),
        rows[1],
    );
    f.render_widget(
        line(
            s.field == SettingsField::HtmlExternal,
            "HTML mail in $BROWSER",
            check(s.draft.html_external).to_string(),
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
            check(s.draft.show_snippet).to_string(),
        ),
        rows[4],
    );

    // Theme row: show name with left/right arrows as hints
    let theme_sel = s.field == SettingsField::Theme;
    let theme_style = if theme_sel { theme::accent() } else { theme::normal() };
    let theme_value = if theme_sel {
        format!("< {} >", s.draft.theme.label())
    } else {
        s.draft.theme.label().to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", mark(theme_sel)), theme::accent()),
            Span::styled(format!("{:<26}", "Theme"), theme_style),
            Span::styled(theme_value, theme::accent().add_modifier(Modifier::BOLD)),
        ])),
        rows[5],
    );

    // AI provider row: cycler (←/→), like Theme.
    let prov_sel = s.field == SettingsField::AiProvider;
    let prov_style = if prov_sel { theme::accent() } else { theme::normal() };
    let prov_value = if prov_sel {
        format!("< {} >", s.draft.ai_provider.label())
    } else {
        s.draft.ai_provider.label().to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", mark(prov_sel)), theme::accent()),
            Span::styled(format!("{:<26}", "AI reply provider"), prov_style),
            Span::styled(prov_value, theme::accent().add_modifier(Modifier::BOLD)),
        ])),
        rows[6],
    );

    // AI model row: text input, like Browser.
    let model_display = if s.ai_model.value().is_empty() {
        "[]  (empty = CLI default)".to_string()
    } else {
        format!("[{}]", s.ai_model.value())
    };
    f.render_widget(
        line(
            s.field == SettingsField::AiModel,
            "AI model",
            model_display,
        ),
        rows[7],
    );

    let info = Paragraph::new(vec![
        Line::from(Span::styled("", theme::muted())),
        Line::from(Span::styled(" Compose: Ctrl-I drafts a reply with the selected AI provider.", theme::muted())),
        Line::from(Span::styled(" Messages are NEVER deleted from the server.", theme::muted())),
    ]);
    f.render_widget(info, rows[8]);

    let footer = Paragraph::new(Span::styled(
        " [Tab] next  [Space] toggle  [←/→] cycle  [Ctrl+S] save  [Esc] cancel ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[9]);
}
