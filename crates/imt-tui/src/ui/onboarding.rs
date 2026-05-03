//! Account onboarding modal popup.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::OnboardingField;
use crate::theme;
use crate::ui::layout::centered;

/// Render the onboarding modal.
pub fn render(f: &mut Frame, full: Rect, app: &App) {
    let area = centered(full, 70, 90);
    f.render_widget(Clear, area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused())
        .title(Span::styled(" Add Account ", theme::accent()))
        .style(Style::default().bg(theme::POPUP_BG));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint line
            Constraint::Length(3), // display name
            Constraint::Length(3), // email
            Constraint::Length(3), // imap host
            Constraint::Length(3), // imap port
            Constraint::Length(3), // imap tls
            Constraint::Length(3), // smtp host
            Constraint::Length(3), // smtp port
            Constraint::Length(3), // smtp tls
            Constraint::Length(3), // username
            Constraint::Length(3), // password
            Constraint::Min(0),
            Constraint::Length(1), // footer
        ])
        .split(inner);

    let onboarding = match app.onboarding.as_ref() {
        Some(o) => o,
        None => return,
    };

    let hint = if let Some(provider) = onboarding.detected_provider.as_deref() {
        format!("Detected provider: {provider} (defaults applied)")
    } else {
        "Common providers: gmail / outlook / fastmail / yahoo / icloud auto-fill on email entry"
            .to_string()
    };
    let hint_p = Paragraph::new(Span::styled(hint, theme::muted()));
    f.render_widget(hint_p, chunks[0]);

    let cur = onboarding.field;
    render_input(f, chunks[1], "Display name", onboarding.display_name.value(), cur == OnboardingField::DisplayName);
    render_input(f, chunks[2], "Email", onboarding.email.value(), cur == OnboardingField::Email);
    render_input(f, chunks[3], "IMAP host", onboarding.imap_host.value(), cur == OnboardingField::ImapHost);
    render_input(f, chunks[4], "IMAP port", onboarding.imap_port.value(), cur == OnboardingField::ImapPort);
    render_field(f, chunks[5], "IMAP TLS", &format!("< {} >", tls_label(onboarding.imap_tls)), cur == OnboardingField::ImapTls, false);
    render_input(f, chunks[6], "SMTP host", onboarding.smtp_host.value(), cur == OnboardingField::SmtpHost);
    render_input(f, chunks[7], "SMTP port", onboarding.smtp_port.value(), cur == OnboardingField::SmtpPort);
    render_field(f, chunks[8], "SMTP TLS", &format!("< {} >", tls_label(onboarding.smtp_tls)), cur == OnboardingField::SmtpTls, false);
    render_input(f, chunks[9], "Username", onboarding.username.value(), cur == OnboardingField::Username);
    let masked: String = "*".repeat(onboarding.password.value().chars().count());
    render_input(f, chunks[10], "Password", &masked, cur == OnboardingField::Password);

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Tab", theme::accent()),
        Span::styled(" next  ", theme::muted()),
        Span::styled("Shift-Tab", theme::accent()),
        Span::styled(" prev  ", theme::muted()),
        Span::styled("Ctrl-S", theme::accent()),
        Span::styled(" save  ", theme::muted()),
        Span::styled("Esc", theme::accent()),
        Span::styled(" cancel", theme::muted()),
    ]))
    .style(theme::status());
    f.render_widget(footer, chunks[12]);
}

fn tls_label(t: imt_core::Tls) -> &'static str {
    match t {
        imt_core::Tls::Implicit => "Implicit",
        imt_core::Tls::StartTls => "StartTls",
        imt_core::Tls::None => "None",
    }
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
