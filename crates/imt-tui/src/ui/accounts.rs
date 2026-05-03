//! Account manager modal.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState,
};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

fn centered_pct(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let margin_v = (area.height as u32 * (100 - pct_h as u32) / 200) as u16;
    let margin_h = (area.width as u32 * (100 - pct_w as u32) / 200) as u16;
    Rect {
        x: area.x + margin_h,
        y: area.y + margin_v,
        width: area.width.saturating_sub(margin_h * 2),
        height: area.height.saturating_sub(margin_v * 2),
    }
}

pub fn render_modal(f: &mut Frame, area: Rect, app: &App) {
    let modal = centered_pct(area, 92, 60);
    let state = match app.accounts_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    f.render_widget(Clear, modal);

    let title = if state.confirm_delete.is_some() {
        " Account Manager - press [d] again to confirm delete "
    } else {
        " Account Manager "
    };
    let block = Block::default()
        .title(Span::styled(title, theme::accent().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    if app.accounts.is_empty() {
        let p = Paragraph::new("No accounts. Press 'a' to add one.")
            .alignment(Alignment::Center)
            .style(theme::muted());
        f.render_widget(p, rows[0]);
    } else {
        let header = Row::new(vec![
            Cell::from("  Name").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Cell::from("Email").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Cell::from("IMAP Host").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Cell::from("Port").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Cell::from("SMTP Host").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Cell::from("Port").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Cell::from("TLS").style(theme::header_label().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        ])
        .height(1);

        let data_rows: Vec<Row> = app
            .accounts
            .iter()
            .map(|av| {
                let acc = &av.account;
                let confirm = state.confirm_delete == Some(acc.id);
                let prefix = if confirm { "! " } else { "  " };
                let style = if confirm { theme::error() } else { theme::normal() };
                let tls = format!("{:?}", acc.imap.tls);
                Row::new(vec![
                    Cell::from(format!("{}{}", prefix, acc.display_name)).style(style),
                    Cell::from(acc.address.email.clone()).style(style),
                    Cell::from(acc.imap.host.clone()).style(style),
                    Cell::from(acc.imap.port.to_string()).style(style),
                    Cell::from(acc.smtp.host.clone()).style(style),
                    Cell::from(acc.smtp.port.to_string()).style(style),
                    Cell::from(tls).style(theme::muted()),
                ])
                .height(1)
            })
            .collect();

        let widths = [
            Constraint::Length(22),
            Constraint::Min(24),
            Constraint::Min(20),
            Constraint::Length(6),
            Constraint::Min(20),
            Constraint::Length(6),
            Constraint::Length(9),
        ];

        let table = Table::new(data_rows, widths)
            .header(header)
            .highlight_style(theme::selected())
            .highlight_symbol("");

        let mut ts = TableState::default();
        ts.select(Some(state.selected));
        f.render_stateful_widget(table, rows[0], &mut ts);
    }

    let footer = Paragraph::new(Line::from(Span::styled(
        " [↑↓] up/down  [e] edit  [Enter] select  [d] delete  [a] add  [Esc] cancel",
        theme::muted(),
    )))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}
