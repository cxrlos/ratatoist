use anyhow::Result;
use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::api::models::{CreateComment, CreateLabel, CreateProject, CreateTask};
use ratatoist_core::config::Config;

const TEST_PROJECT_NAME: &str = "[ratatoist-test] Scaffold Verification";

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = TodoistClient::new(config.token())?;

    println!("=== ratatoist API test harness ===\n");

    println!("── 1. Config ──");
    println!("  token loaded: yes (redacted)");
    println!("  config debug: {:?}", config);
    println!("  PASS\n");

    println!("── 2. GET /projects ──");
    let projects = client.get_projects().await?;
    println!("  found {} projects", projects.len());
    for p in &projects {
        let marker = if p.is_inbox() {
            " (inbox)"
        } else if p.is_favorite {
            " ★"
        } else {
            ""
        };
        println!("  - {}{}", p.name, marker);
    }
    assert!(!projects.is_empty(), "expected at least an Inbox project");
    println!("  PASS\n");

    println!("── 3. POST /projects (create test project) ──");
    let test_project = client
        .create_project(&CreateProject {
            name: TEST_PROJECT_NAME.to_string(),
            color: Some("blue".to_string()),
            parent_id: None,
            is_favorite: Some(false),
        })
        .await?;
    println!("  created: {} (id: {})", test_project.name, test_project.id);
    println!("  PASS\n");

    let project_id = test_project.id.clone();

    println!("── 4. POST /labels (create test labels) ──");
    let label_bug = client
        .create_label(&CreateLabel {
            name: "ratatoist-bug".to_string(),
            color: Some("red".to_string()),
        })
        .await?;
    let label_feature = client
        .create_label(&CreateLabel {
            name: "ratatoist-feature".to_string(),
            color: Some("blue".to_string()),
        })
        .await?;
    let label_chore = client
        .create_label(&CreateLabel {
            name: "ratatoist-chore".to_string(),
            color: Some("grey".to_string()),
        })
        .await?;
    println!("  created: {} (id: {})", label_bug.name, label_bug.id);
    println!(
        "  created: {} (id: {})",
        label_feature.name, label_feature.id
    );
    println!("  created: {} (id: {})", label_chore.name, label_chore.id);
    println!("  PASS\n");

    println!("── 5. POST /tasks (create test tasks) ──");

    let tasks_to_create = vec![
        CreateTask {
            content: "Fix crash on startup".to_string(),
            description: Some("App panics when token is missing from config".to_string()),
            project_id: Some(project_id.clone()),
            priority: Some(4),
            due_string: Some("today".to_string()),
            labels: Some(vec!["ratatoist-bug".to_string()]),
        },
        CreateTask {
            content: "Implement task detail view".to_string(),
            description: Some(
                "Show description, labels, comments when pressing Enter on a task".to_string(),
            ),
            project_id: Some(project_id.clone()),
            priority: Some(3),
            due_string: Some("tomorrow".to_string()),
            labels: Some(vec!["ratatoist-feature".to_string()]),
        },
        CreateTask {
            content: "Add loading spinner".to_string(),
            description: None,
            project_id: Some(project_id.clone()),
            priority: Some(2),
            due_string: Some("next week".to_string()),
            labels: Some(vec!["ratatoist-feature".to_string()]),
        },
        CreateTask {
            content: "Update README with screenshots".to_string(),
            description: None,
            project_id: Some(project_id.clone()),
            priority: Some(1),
            due_string: None,
            labels: Some(vec!["ratatoist-chore".to_string()]),
        },
        CreateTask {
            content: "Minimal task with no metadata".to_string(),
            description: Some("Edge case: task with minimal fields".to_string()),
            project_id: Some(project_id.clone()),
            priority: Some(1),
            due_string: None,
            labels: None,
        },
        CreateTask {
            content: "Should have been done yesterday".to_string(),
            description: None,
            project_id: Some(project_id.clone()),
            priority: Some(4),
            due_string: Some("yesterday".to_string()),
            labels: Some(vec!["ratatoist-bug".to_string()]),
        },
        CreateTask {
            content: "Multi-label task".to_string(),
            description: Some("Tests that multiple labels render correctly".to_string()),
            project_id: Some(project_id.clone()),
            priority: Some(3),
            due_string: Some("in 3 days".to_string()),
            labels: Some(vec![
                "ratatoist-bug".to_string(),
                "ratatoist-feature".to_string(),
            ]),
        },
    ];

    let mut created_task_ids = Vec::new();
    for task_def in &tasks_to_create {
        let task = client.create_task(task_def).await?;
        println!(
            "  created: [P{}] {} (id: {}, due: {})",
            task.priority,
            task.content,
            task.id,
            task.due.as_ref().map(|d| d.date.as_str()).unwrap_or("none")
        );
        created_task_ids.push(task.id.clone());
    }
    println!("  {} tasks created", created_task_ids.len());
    println!("  PASS\n");

    println!("── 6. POST /comments (create test comment) ──");
    let comment = client
        .create_comment(&CreateComment {
            content: "This is a test comment from ratatoist test harness.".to_string(),
            task_id: Some(created_task_ids[0].clone()),
            project_id: None,
        })
        .await?;
    println!(
        "  created comment (id: {}) on task {}",
        comment.id, created_task_ids[0]
    );
    println!("  PASS\n");

    println!("── 7. GET /tasks?project_id (verify read-back) ──");
    let fetched_tasks = client.get_tasks(Some(&project_id)).await?;
    println!("  fetched {} tasks from test project", fetched_tasks.len());
    assert_eq!(
        fetched_tasks.len(),
        tasks_to_create.len(),
        "expected {} tasks, got {}",
        tasks_to_create.len(),
        fetched_tasks.len()
    );
    for t in &fetched_tasks {
        let due = t.due.as_ref().map(|d| d.date.as_str()).unwrap_or("none");
        let labels = if t.labels.is_empty() {
            "none".to_string()
        } else {
            t.labels.join(", ")
        };
        println!(
            "  [P{}] {} | due: {} | labels: {}",
            t.priority, t.content, due, labels
        );
    }
    println!("  PASS\n");

    println!("── 8. GET /labels (verify labels exist) ──");
    let labels = client.get_labels().await?;
    let test_labels: Vec<_> = labels
        .iter()
        .filter(|l| l.name.starts_with("ratatoist-"))
        .collect();
    println!("  found {} ratatoist-* labels", test_labels.len());
    for l in &test_labels {
        println!("  - {} (color: {}, id: {})", l.name, l.color, l.id);
    }
    assert_eq!(test_labels.len(), 3, "expected 3 test labels");
    println!("  PASS\n");

    println!("── 9. GET /comments (verify comment on task) ──");
    let comments = client.get_comments(&created_task_ids[0]).await?;
    println!(
        "  found {} comment(s) on task {}",
        comments.len(),
        created_task_ids[0]
    );
    assert!(!comments.is_empty(), "expected at least 1 comment");
    println!("  content: {}", comments[0].content);
    println!("  PASS\n");

    println!("══════════════════════════════════════════");
    println!("  ALL 9 SECTIONS PASSED");
    println!("══════════════════════════════════════════");
    println!();
    println!("Test data is live in your Todoist account.");
    println!("Run `cargo run` now to see it in the TUI.");
    println!();
    println!("To clean up test data later, run:");
    println!("  cargo run --example cleanup_test");

    Ok(())
}
