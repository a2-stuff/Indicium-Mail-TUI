//! Attachment viewer modal: list attachments and view text-based ones inline.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, AttachmentViewMode};
use crate::attachments;
use crate::theme;

fn centered_pct(area: Rect, pw: u16, ph: u16) -> Rect {
    let mh = (area.height as u32 * (100 - ph as u32) / 200) as u16;
    let mw = (area.width as u32 * (100 - pw as u32) / 200) as u16;
    Rect {
        x: area.x + mw,
        y: area.y + mh,
        width: area.width.saturating_sub(mw * 2),
        height: area.height.saturating_sub(mh * 2),
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let av = match app.attachment_viewer.as_ref() {
        Some(s) => s,
        None => return,
    };

    let modal = centered_pct(area, 88, 80);
    f.render_widget(Clear, modal);

    match &av.mode {
        AttachmentViewMode::Listing { selected } => render_list(f, modal, app, *selected),
        AttachmentViewMode::Viewing { content, scroll, idx } => {
            let att = av.attachments.get(*idx);
            render_content(f, modal, att.map(|a| a.filename.as_str()).unwrap_or("attachment"), content, *scroll)
        }
        AttachmentViewMode::ViewingImage { idx, image } => {
            let att = av.attachments.get(*idx);
            render_image(f, modal, att.map(|a| a.filename.as_str()).unwrap_or("image"), image)
        }
    }
}

fn render_image(f: &mut Frame, area: Rect, filename: &str, image: &image::DynamicImage) {
    let block = Block::default()
        .title(Span::styled(
            format!("  {}  ", filename),
            theme::accent().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let lines = attachments::image_to_lines(image, rows[0].width, rows[0].height);
    let p = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(p, rows[0]);

    let footer = Paragraph::new(Span::styled(
        " [s] save to Downloads  [Esc] back ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}

fn render_list(f: &mut Frame, area: Rect, app: &App, selected: usize) {
    let av = app.attachment_viewer.as_ref().unwrap();
    let count = av.attachments.len();

    let block = Block::default()
        .title(Span::styled(
            format!(" Attachments ({}) ", count),
            theme::accent().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    let items: Vec<ListItem> = av.attachments.iter().map(|att| {
        let kind = attachments::classify(&att.mime_type, &att.filename);
        let viewable = kind != attachments::AttachmentKind::Other;
        let has_data = att.temp_path.is_some();
        let icon = match kind {
            attachments::AttachmentKind::Image => "[img]",
            attachments::AttachmentKind::Pdf => "[pdf]",
            attachments::AttachmentKind::Docx => "[doc]",
            attachments::AttachmentKind::Text => "[txt]",
            attachments::AttachmentKind::Other => "[bin]",
        };
        let size_str = format_size(att.size);
        let name = if att.filename.is_empty() { att.mime_type.clone() } else { att.filename.clone() };
        let action = if !has_data { " (no data)" } else if viewable { " [Enter] view" } else { " [s] save" };
        let line = Line::from(vec![
            Span::styled(format!(" {} ", icon), if viewable { theme::accent() } else { theme::muted() }),
            Span::styled(name, if viewable { theme::normal() } else { theme::muted() }),
            Span::styled(format!("  {}  {}", size_str, att.mime_type), theme::muted()),
            Span::styled(action, theme::muted().add_modifier(Modifier::ITALIC)),
        ]);
        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("");
    let mut ls = ListState::default();
    ls.select(Some(selected));
    f.render_stateful_widget(list, rows[0], &mut ls);

    let footer = Paragraph::new(Span::styled(
        " [↑↓] navigate  [Enter] view  [s] save to Downloads  [Esc] close ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}

fn render_content(f: &mut Frame, area: Rect, filename: &str, content: &str, scroll: u16) {
    let block = Block::default()
        .title(Span::styled(
            format!("  {}  ", filename),
            theme::accent().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    let p = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .style(theme::normal());
    f.render_widget(p, rows[0]);

    let footer = Paragraph::new(Span::styled(
        " [↑↓] scroll  [s] save to Downloads  [Esc] back ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}

pub fn render_html_viewer(f: &mut Frame, area: Rect, content: &str, scroll: u16) {
    let modal = centered_pct(area, 92, 90);
    f.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            "  HTML View  ",
            theme::accent().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_focused());

    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    let p = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .style(theme::normal());
    f.render_widget(p, rows[0]);

    let footer = Paragraph::new(Span::styled(
        " [↑↓] scroll  [o / Esc] close ",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    f.render_widget(footer, rows[1]);
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{}B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.0}K", bytes as f64 / 1024.0) }
    else { format!("{:.1}M", bytes as f64 / 1024.0 / 1024.0) }
}
