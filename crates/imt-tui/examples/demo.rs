//! Smoke-test demo: launch the TUI against in-memory sample data.

use imt_tui::InMemoryDataSource;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    imt_tui::run(InMemoryDataSource::sample()).await
}
