//! Terminal lifecycle and main event loop.

use std::io::{stdout, Stdout};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::data::DataSource;
use crate::ui;

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        // Mouse capture intentionally NOT enabled - it would block native
        // terminal text selection / copy-paste, and the app doesn't use mouse.
        execute!(stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

/// Run the TUI application against `data`. Restores terminal on exit or panic.
pub async fn run<D: DataSource + 'static>(data: D) -> anyhow::Result<()> {
    run_with(data, crate::settings::Settings::default(), std::sync::Arc::new(|_| {})).await
}

/// Run the TUI with initial settings and a persistence callback.
pub async fn run_with<D: DataSource + 'static>(
    data: D,
    settings: crate::settings::Settings,
    on_settings_changed: std::sync::Arc<dyn Fn(&crate::settings::Settings) + Send + Sync>,
) -> anyhow::Result<()> {
    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let data_arc: Arc<dyn DataSource> = Arc::new(data);
    let mut app = App::new(data_arc);
    app.apply_settings(settings);
    app.on_settings_changed = Some(on_settings_changed);

    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(250));

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind != KeyEventKind::Release {
                            app.handle_key(key);
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                }
            }
            _ = tick.tick() => {
                app.tick();
            }
        }
    }
    Ok(())
}
