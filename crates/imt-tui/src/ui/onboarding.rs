//! Account onboarding modal popup.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::keymap::OnboardingField;
use crate::theme;
use crate::ui::layout::centered;

/// Render the onboarding modal.
pub fn render(f: &mut Frame, full: Rect, app: &App) {
    let area = centered(full, 72, 94);
    f.render_widget(Clear, area);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused())
        .title(Span::styled(" Add Account ", theme::accent()))
        .style(Style::default().bg(theme::POPUP_BG));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let onboarding = match app.onboarding.as_ref() {
        Some(o) => o,
        None => return,
    };
    let oauth2 = onboarding.use_oauth2;

    // Build constraint list based on auth type.
    let _auth_extra_rows = if oauth2 { 4 } else { 1 };
    let url_height: u16 = if oauth2 && onboarding.oauth_auth_url.is_some() { 4 } else { 0 };

    let mut constraints = vec![
        Constraint::Length(1), // hint
        Constraint::Length(3), // display name
        Constraint::Length(3), // email
        Constraint::Length(3), // imap host
        Constraint::Length(3), // imap port
        Constraint::Length(3), // imap tls
        Constraint::Length(3), // smtp host
        Constraint::Length(3), // smtp port
        Constraint::Length(3), // smtp tls
        Constraint::Length(3), // username
        Constraint::Length(3), // auth type
    ];
    if oauth2 {
        constraints.push(Constraint::Length(3)); // client_id
        constraints.push(Constraint::Length(3)); // client_secret
        if url_height > 0 {
            constraints.push(Constraint::Length(url_height)); // auth url display
        }
        constraints.push(Constraint::Length(3)); // auth code
    } else {
        constraints.push(Constraint::Length(3)); // password
    }
    constraints.push(Constraint::Min(0));
    constraints.push(Constraint::Length(1)); // footer

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let hint = if let Some(provider) = onboarding.detected_provider.as_deref() {
        format!("Detected: {provider} (defaults applied)")
    } else {
        "Common providers: gmail / outlook / fastmail / yahoo / icloud auto-fill on email entry".to_string()
    };
    f.render_widget(Paragraph::new(Span::styled(hint, theme::muted())), chunks[0]);

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

    let auth_label = if oauth2 { "< OAuth2 >" } else { "< Password >" };
    render_field(f, chunks[10], "Auth type  [←/→]", auth_label, cur == OnboardingField::AuthType, false);

    let mut idx = 11usize;
    if oauth2 {
        render_input(f, chunks[idx], "Client ID", onboarding.client_id.value(), cur == OnboardingField::ClientId);
        idx += 1;
        let secret_masked: String = "*".repeat(onboarding.client_secret.value().chars().count());
        render_input(f, chunks[idx], "Client Secret (optional)", &secret_masked, cur == OnboardingField::ClientSecret);
        idx += 1;
        if url_height > 0 {
            if let Some(url) = &onboarding.oauth_auth_url {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(theme::border())
                    .title(Span::styled(" Auth URL - open in browser, then paste code below ", theme::muted()));
                let p = Paragraph::new(Span::styled(url.as_str(), theme::accent().add_modifier(Modifier::BOLD)))
                    .block(block)
                    .wrap(Wrap { trim: true });
                f.render_widget(p, chunks[idx]);
                idx += 1;
            }
        }
        render_input(f, chunks[idx], "Auth Code (from redirect URL ?code=...)", onboarding.auth_code.value(), cur == OnboardingField::AuthCode);
        idx += 1;
    } else {
        let masked: String = "*".repeat(onboarding.password.value().chars().count());
        render_input(f, chunks[idx], "Password", &masked, cur == OnboardingField::Password);
        idx += 1;
    }

    let footer_idx = idx + 1; // skip Min(0)
    if footer_idx < chunks.len() {
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("[Tab]", theme::accent()),
            Span::styled(" next  ", theme::muted()),
            Span::styled("[Shift-Tab]", theme::accent()),
            Span::styled(" prev  ", theme::muted()),
            Span::styled("[←/→]", theme::accent()),
            Span::styled(" cycle  ", theme::muted()),
            Span::styled("[Ctrl+S]", theme::accent()),
            Span::styled(" save  ", theme::muted()),
            Span::styled("[Esc]", theme::accent()),
            Span::styled(" cancel", theme::muted()),
        ]))
        .style(theme::status());
        f.render_widget(footer, chunks[footer_idx]);
    }
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
