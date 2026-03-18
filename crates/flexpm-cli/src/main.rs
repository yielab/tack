use clap::{Parser, Subcommand};
use flexpm_core::models::{CreateProject, ProjectType, CreateItem, ItemType, Priority, CreateWorkspace};
use flexpm_db::repo;
use sqlx::SqlitePool;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "flexpm", version, about = "FlexPM - Flexible Project Management")]
struct Cli {
    /// Database file path
    #[arg(short, long, default_value = "flexpm.db", env = "FLEXPM_DATABASE")]
    database: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new project
    Init {
        /// Project name
        name: String,
        /// Project type (software, web, mobile, construction, personal, homework, maintenance)
        #[arg(short, long, default_value = "software")]
        r#type: String,
    },
    /// Add a new item (task, epic, etc.)
    Add {
        /// Item type
        item_type: String,
        /// Item title
        title: String,
        /// Priority
        #[arg(short, long, default_value = "medium")]
        priority: String,
        /// Parent item ID
        #[arg(long)]
        parent: Option<String>,
    },
    /// List items with optional filters
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by type
        #[arg(short = 't', long)]
        item_type: Option<String>,
        /// Show as tree
        #[arg(long)]
        tree: bool,
    },
    /// Move an item to a new status
    Move {
        /// Item ID (short UUID prefix accepted)
        id: String,
        /// Target status
        status: String,
    },
    /// Show board in terminal (ASCII)
    Board,
    /// Sprint management
    Sprint {
        #[command(subcommand)]
        action: SprintAction,
    },
    /// Search items
    Search {
        /// Search query
        query: String,
    },
}

#[derive(Subcommand)]
enum SprintAction {
    /// Create a new sprint
    Create { name: String },
    /// Start the current sprint
    Start,
    /// Close the current sprint
    Close,
    /// List sprints
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for CLI
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("flexpm=info".parse().unwrap()),
        )
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();

    // Connect to database
    let db_url = format!("sqlite:{}?mode=rwc", cli.database.display());
    let pool = SqlitePool::connect(&db_url).await?;

    // Run migrations
    flexpm_db::run_migrations(&pool).await?;

    match cli.command {
        Commands::Init { name, r#type } => {
            init_project(&pool, name, r#type).await?;
        }
        Commands::Add { item_type, title, priority, parent } => {
            add_item(&pool, item_type, title, priority, parent).await?;
        }
        Commands::List { status, item_type, tree } => {
            list_items(&pool, status, item_type, tree).await?;
        }
        Commands::Move { id, status } => {
            move_item(&pool, id, status).await?;
        }
        Commands::Board => {
            show_board(&pool).await?;
        }
        Commands::Sprint { action } => {
            handle_sprint(&pool, action).await?;
        }
        Commands::Search { query } => {
            search_items(&pool, query).await?;
        }
    }

    Ok(())
}

async fn init_project(pool: &SqlitePool, name: String, type_str: String) -> anyhow::Result<()> {
    println!("Initializing project '{name}' with type '{type_str}'...");

    // Parse project type
    let project_type = match type_str.as_str() {
        "software" => ProjectType::Software,
        "web" => ProjectType::Web,
        "mobile" => ProjectType::Mobile,
        "construction" => ProjectType::Construction,
        "personal" => ProjectType::Personal,
        "homework" => ProjectType::Homework,
        "maintenance" => ProjectType::Maintenance,
        "custom" => ProjectType::Custom,
        _ => anyhow::bail!("Invalid project type: {type_str}"),
    };

    // Get or create default workspace
    let workspace_id = get_or_create_default_workspace(pool).await?;

    // Create project
    let create_data = CreateProject {
        name: name.clone(),
        description: None,
        project_type,
        vocabulary: None,
        workflow: None,
    };

    let project = repo::projects::create_project(pool, workspace_id, create_data).await?;

    println!("✓ Project created successfully");
    println!("  ID: {}", project.id);
    println!("  Name: {}", project.name);
    println!("  Type: {:?}", project.project_type);
    println!("  Workflow: {} statuses", project.workflow.statuses.len());

    Ok(())
}

async fn add_item(
    pool: &SqlitePool,
    item_type_str: String,
    title: String,
    priority_str: String,
    parent: Option<String>,
) -> anyhow::Result<()> {
    // Get the first project
    let projects = repo::projects::list_projects(pool).await?;
    let project = projects.first().ok_or_else(|| anyhow::anyhow!("No projects found. Run 'flexpm init' first."))?;

    // Parse item type
    let item_type = match item_type_str.as_str() {
        "epic" => ItemType::Epic,
        "feature" => ItemType::Feature,
        "task" => ItemType::Task,
        "subtask" => ItemType::Subtask,
        "bug" => ItemType::Bug,
        "spike" => ItemType::Spike,
        _ => anyhow::bail!("Invalid item type: {item_type_str}"),
    };

    // Parse priority
    let priority = match priority_str.as_str() {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        "medium" => Priority::Medium,
        "low" => Priority::Low,
        "none" => Priority::None,
        _ => anyhow::bail!("Invalid priority: {priority_str}"),
    };

    // Parse parent ID if provided
    let parent_id = if let Some(p) = parent {
        Some(Uuid::parse_str(&p)?)
    } else {
        None
    };

    // Get first status from workflow
    let first_status = project.workflow.statuses.first()
        .ok_or_else(|| anyhow::anyhow!("Project has no statuses"))?;

    // Create item
    let create_data = CreateItem {
        title: title.clone(),
        description: None,
        item_type,
        status: first_status.name.clone(),
        priority,
        estimate: None,
        tags: vec![],
        parent_id,
        sprint_id: None,
        sort_order: None,
        due_date: None,
    };

    let item = repo::items::create_item(pool, project.id, create_data).await?;

    println!("✓ Item created successfully");
    println!("  ID: {}", item.id);
    println!("  Title: {}", item.title);
    println!("  Type: {:?}", item.item_type);
    println!("  Status: {}", item.status);
    println!("  Priority: {:?}", item.priority);

    Ok(())
}

async fn list_items(
    pool: &SqlitePool,
    status_filter: Option<String>,
    type_filter: Option<String>,
    tree: bool,
) -> anyhow::Result<()> {
    // Get the first project
    let projects = repo::projects::list_projects(pool).await?;
    let project = projects.first().ok_or_else(|| anyhow::anyhow!("No projects found. Run 'flexpm init' first."))?;

    println!("Items in project '{}':", project.name);
    println!();

    if tree {
        // Fetch tree view
        let items = repo::items::get_item_tree(pool, project.id).await?;
        print_item_tree(&items, 0, status_filter.as_deref(), type_filter.as_deref());
    } else {
        // Fetch flat list
        let query = repo::items::ItemQuery {
            status: status_filter.clone(),
            item_type: type_filter.clone(),
            priority: None,
            parent_id: None,
            sprint_id: None,
            tags: None,
            page: None,
            per_page: None,
        };

        let items = repo::items::list_items(pool, project.id, query).await?;

        if items.is_empty() {
            println!("No items found.");
        } else {
            // Print table header
            println!("{:<36} {:<30} {:<10} {:<15} {:<10}", "ID", "TITLE", "TYPE", "STATUS", "PRIORITY");
            println!("{}", "-".repeat(105));

            for item in items {
                let id_short = &item.id.to_string()[..8];
                let title_trunc = if item.title.len() > 28 {
                    format!("{}...", &item.title[..25])
                } else {
                    item.title.clone()
                };

                println!(
                    "{:<36} {:<30} {:<10} {:<15} {:<10}",
                    id_short,
                    title_trunc,
                    format!("{:?}", item.item_type),
                    item.status,
                    format!("{:?}", item.priority)
                );
            }

            println!();
            println!("Total: {} items", items.len());
        }
    }

    Ok(())
}

fn print_item_tree(items: &[flexpm_core::models::Item], indent: usize, status_filter: Option<&str>, type_filter: Option<&str>) {
    for item in items {
        // Apply filters
        if let Some(status) = status_filter {
            if item.status != status {
                continue;
            }
        }
        if let Some(item_type) = type_filter {
            if format!("{:?}", item.item_type).to_lowercase() != item_type.to_lowercase() {
                continue;
            }
        }

        let indent_str = "  ".repeat(indent);
        let id_short = &item.id.to_string()[..8];
        println!("{indent_str}[{id_short}] {} ({:?}, {})", item.title, item.item_type, item.status);
    }
}

async fn move_item(pool: &SqlitePool, id_str: String, new_status: String) -> anyhow::Result<()> {
    // Try to parse full UUID or find by prefix
    let item_id = if let Ok(uuid) = Uuid::parse_str(&id_str) {
        uuid
    } else {
        // Search for item by ID prefix
        let projects = repo::projects::list_projects(pool).await?;
        let project = projects.first().ok_or_else(|| anyhow::anyhow!("No projects found"))?;

        let items = repo::items::list_items(pool, project.id, Default::default()).await?;
        items.iter()
            .find(|item| item.id.to_string().starts_with(&id_str))
            .ok_or_else(|| anyhow::anyhow!("No item found with ID prefix: {id_str}"))?
            .id
    };

    // Update item status
    let update = flexpm_core::models::UpdateItem {
        title: None,
        description: None,
        status: Some(new_status.clone()),
        priority: None,
        estimate: None,
        tags: None,
        parent_id: None,
        sprint_id: None,
        sort_order: None,
        due_date: None,
    };

    let updated_item = repo::items::update_item(pool, item_id, update).await?;

    println!("✓ Item moved successfully");
    println!("  ID: {}", updated_item.id);
    println!("  Title: {}", updated_item.title);
    println!("  New Status: {}", updated_item.status);

    Ok(())
}

async fn show_board(pool: &SqlitePool) -> anyhow::Result<()> {
    // Get the first project
    let projects = repo::projects::list_projects(pool).await?;
    let project = projects.first().ok_or_else(|| anyhow::anyhow!("No projects found. Run 'flexpm init' first."))?;

    println!("Board: {}", project.name);
    println!("{}", "=".repeat(80));
    println!();

    // Get all items
    let items = repo::items::list_items(pool, project.id, Default::default()).await?;

    // Group by status
    for status in &project.workflow.statuses {
        let status_items: Vec<_> = items.iter()
            .filter(|item| item.status == status.name)
            .collect();

        // Print column header
        let header = format!("{} ({})", status.name, status_items.len());
        println!("{}", header);
        println!("{}", "-".repeat(header.len()));

        // Print items in column
        for item in status_items {
            let id_short = &item.id.to_string()[..8];
            println!("[{id_short}] {}", item.title);
            println!("  Type: {:?} | Priority: {:?}", item.item_type, item.priority);
            if let Some(estimate) = item.estimate {
                println!("  Estimate: {estimate}");
            }
            println!();
        }

        println!();
    }

    Ok(())
}

async fn handle_sprint(pool: &SqlitePool, action: SprintAction) -> anyhow::Result<()> {
    match action {
        SprintAction::Create { name } => {
            println!("Creating sprint: {name}");
            println!("⚠  Not yet implemented");
        }
        SprintAction::Start => {
            println!("Starting sprint...");
            println!("⚠  Not yet implemented");
        }
        SprintAction::Close => {
            println!("Closing sprint...");
            println!("⚠  Not yet implemented");
        }
        SprintAction::List => {
            println!("Listing sprints...");
            println!("⚠  Not yet implemented");
        }
    }
    Ok(())
}

async fn search_items(pool: &SqlitePool, query: String) -> anyhow::Result<()> {
    // Get the first project
    let projects = repo::projects::list_projects(pool).await?;
    let project = projects.first().ok_or_else(|| anyhow::anyhow!("No projects found. Run 'flexpm init' first."))?;

    println!("Searching for: '{query}'");
    println!();

    let results = repo::items::search_items(pool, project.id, &query).await?;

    if results.is_empty() {
        println!("No results found.");
    } else {
        println!("Found {} results:", results.len());
        println!();

        for item in results {
            let id_short = &item.id.to_string()[..8];
            println!("[{id_short}] {}", item.title);
            println!("  Type: {:?} | Status: {} | Priority: {:?}", item.item_type, item.status, item.priority);
            if let Some(desc) = &item.description {
                let desc_short = if desc.len() > 80 { format!("{}...", &desc[..77]) } else { desc.clone() };
                println!("  {desc_short}");
            }
            println!();
        }
    }

    Ok(())
}

async fn get_or_create_default_workspace(pool: &SqlitePool) -> anyhow::Result<Uuid> {
    // Try to get existing workspaces
    let workspaces = repo::workspaces::list_workspaces(pool).await?;

    if let Some(workspace) = workspaces.first() {
        Ok(workspace.id)
    } else {
        // Create default workspace
        let create_data = CreateWorkspace {
            name: "Default Workspace".to_string(),
            description: Some("Auto-created workspace".to_string()),
        };

        let workspace = repo::workspaces::create_workspace(pool, create_data).await?;
        Ok(workspace.id)
    }
}
