mod api;
mod app;
mod config;
mod keys;
mod ui;

use anyhow::Result;

use api::client::TodoistClient;
use app::App;
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = TodoistClient::new(config.token())?;

    let mut app = App::new(client);
    app.load_initial_data().await?;

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal).await;
    ratatui::restore();

    result
}
