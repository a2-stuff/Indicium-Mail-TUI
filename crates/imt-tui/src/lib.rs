//! imt-tui: Ratatui application, screens, components, keymap.

pub mod ai;
pub mod app;
pub mod attachments;
pub mod data;
pub mod keymap;
pub mod menu;
pub mod presets;
pub mod quote;
pub mod run;
pub mod settings;
pub mod theme;
pub mod thread;
pub mod ui;

pub use app::App;
pub use data::{DataSource, InMemoryDataSource};
pub use run::{run, run_with};
pub use settings::{AiProvider, Settings};
