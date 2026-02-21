use anyhow::Result;
use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::config::Config;

const TEST_PROJECT_NAME: &str = "[ratatoist-test] Scaffold Verification";

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = TodoistClient::new(config.token())?;

    println!("=== ratatoist test cleanup ===\n");

    let projects = client.get_projects().await?;
    let test_projects: Vec<_> = projects
        .iter()
        .filter(|p| p.name == TEST_PROJECT_NAME)
        .collect();

    if test_projects.is_empty() {
        println!("No test project found, nothing to clean up.");
    } else {
        for p in &test_projects {
            println!("Deleting project: {} (id: {})", p.name, p.id);
            client.delete_project(&p.id).await?;
        }
        println!(
            "Deleted {} test project(s) and all their tasks.",
            test_projects.len()
        );
    }

    let labels = client.get_labels().await?;
    let test_labels: Vec<_> = labels
        .iter()
        .filter(|l| l.name.starts_with("ratatoist-"))
        .collect();

    if test_labels.is_empty() {
        println!("No test labels found.");
    } else {
        for l in &test_labels {
            println!("Deleting label: {} (id: {})", l.name, l.id);
            client.delete_label(&l.id).await?;
        }
        println!("Deleted {} test label(s).", test_labels.len());
    }

    println!("\nCleanup complete.");
    Ok(())
}
