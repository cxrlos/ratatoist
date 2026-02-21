mod app;
mod keys;
mod ui;

use anyhow::Result;
use clap::Parser;

use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::config::Config;
use ratatoist_core::logging;

use app::App;

#[derive(Parser)]
#[command(name = "ratatoist", version, about = "A terminal UI for Todoist")]
struct Cli {
    #[arg(long, help = "Enable debug logging")]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let _log_guard = logging::init(cli.debug)?;

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    };

    let client = match TodoistClient::new(config.token()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to initialize API client: {e:#}");
            std::process::exit(1);
        }
    };

    let mut terminal = ratatui::init();
    let mut app = App::new(client);

    app.load_with_splash(&mut terminal).await;

    let result = app.run(&mut terminal).await;
    ratatui::restore();

    result
}
