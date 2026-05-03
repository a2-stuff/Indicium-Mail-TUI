//! UI rendering: panes, modals, overlays.

pub mod accounts;
pub mod compose;
pub mod help;
pub mod layout;
pub mod list;
pub mod onboarding;
pub mod reader;
pub mod search;
pub mod settings;
pub mod sidebar;
pub mod status;

use ratatui::Frame;

use crate::app::App;
use crate::keymap::Mode;

/// Top-level draw function.
pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = layout::root_layout(f.area());
    sidebar::render(f, chunks.sidebar, app);
    list::render(f, chunks.list, app);
    reader::render(f, chunks.reader, app);
    status::render(f, chunks.status, app);
    if app.mode == Mode::Search {
        search::render(f, chunks.status, app);
    }
    if app.mode == Mode::Compose {
        compose::render(f, f.area(), app);
    }
    if app.mode == Mode::Onboarding {
        onboarding::render(f, f.area(), app);
    }
    if app.mode == Mode::Help {
        help::render(f, f.area());
    }
    if app.mode == Mode::Settings {
        settings::render_modal(f, f.area(), app);
    }
    if app.mode == Mode::Accounts {
        accounts::render_modal(f, f.area(), app);
    }
}
