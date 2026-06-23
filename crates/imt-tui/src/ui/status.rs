//! Status bar at the bottom.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::keymap::Mode;
use crate::theme;

/// Render the status bar.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let mode = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Menu => "MENU",
        Mode::Compose => "COMPOSE",
        Mode::Search => "SEARCH",
        Mode::Help => "HELP",
        Mode::Onboarding => "ONBOARDING",
        Mode::Settings => "SETTINGS",
        Mode::Accounts => "ACCOUNTS",
        Mode::Move => "MOVE",
        Mode::Info => "INFO",
        Mode::FilePicker => "FILES",
        Mode::AttachmentViewer => "ATTACHMENTS",
        Mode::HtmlViewer => "HTML",
        Mode::Thread => "THREAD",
    };
    let hints = match app.mode {
        Mode::Normal => "[↑↓] move  [Enter] open  [Tab] pane  │  [c] compose  [r] reply  [a] attachments  │  [/] search  [F10] menu  [?] help  [q] quit",
        Mode::Menu => "[←→ / Tab] menus  [↑↓] open / select  [Enter] run  [Esc] exit",
        Mode::Compose => "[Tab] next  [Ctrl+G] AI reply  [Ctrl+S] send  [Ctrl+D] save  [Esc] cancel",
        Mode::Search => "[/] search  [Enter] jump  [Esc] cancel",
        Mode::Help => "[Esc] close",
        Mode::Onboarding => "[Tab] next  [Shift-Tab] prev  [Ctrl+S] save  [Esc] cancel  [←/→] cycle",
        Mode::Settings => "[Tab] next  [Space] toggle  [←/→] cycle  [Ctrl+S] save  [Esc] cancel",
        Mode::Accounts => "[↑↓] up/down  [e] edit  [Enter] select  [d] delete  [a] add  [Esc] cancel",
        Mode::Move => "[↑↓] move  [Enter] select  [Esc] cancel",
        Mode::Info => "Esc / q / i  close",
        Mode::FilePicker => "[↑↓] move  [Space] select  [Enter] confirm  [Backspace] up  [Esc] cancel",
        Mode::AttachmentViewer => "[↑↓] navigate  [Enter] view  [s] save  [Esc] close",
        Mode::HtmlViewer => "[↑↓] scroll  [o / Esc] close",
        Mode::Thread => "[↑↓] select message  [PgUp/PgDn] scroll  [a] attachments  [Esc] close",
    };
    let (sync_text, sync_style) = if app.is_busy() {
        (format!(" {} {} ", app.spinner_frame(), app.backend_status), theme::accent())
    } else if app.backend_status.is_empty() {
        (" ready ".to_string(), theme::success())
    } else {
        (format!(" {} ", app.backend_status), theme::success())
    };
    let line = Line::from(vec![
        Span::styled(format!(" {} ", mode), theme::accent()),
        Span::styled(format!(" {} ", hints), theme::muted()),
        Span::styled(sync_text, sync_style),
    ]);
    let p = Paragraph::new(line).style(theme::status());
    f.render_widget(p, area);
}
