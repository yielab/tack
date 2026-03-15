use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "flexpm", version, about = "FlexPM - Flexible Project Management")]
struct Cli {
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

    match cli.command {
        Commands::Init { name, r#type } => {
            println!("Initializing project '{name}' with type '{type}'...");
            // TODO: implement project initialization
            println!("Project created successfully.");
        }
        Commands::Add { item_type, title, priority, parent } => {
            println!("Adding {item_type}: {title} (priority: {priority})");
            if let Some(ref p) = parent {
                println!("  under parent: {p}");
            }
            // TODO: implement item creation
        }
        Commands::List { status, item_type, tree } => {
            println!("Listing items...");
            if let Some(ref s) = status {
                println!("  filtered by status: {s}");
            }
            if let Some(ref t) = item_type {
                println!("  filtered by type: {t}");
            }
            if tree {
                println!("  (tree view)");
            }
            // TODO: implement item listing
        }
        Commands::Move { id, status } => {
            println!("Moving item {id} to '{status}'");
            // TODO: implement item move
        }
        Commands::Board => {
            println!("Board view coming soon...");
            // TODO: implement ASCII board
        }
        Commands::Sprint { action } => {
            match action {
                SprintAction::Create { name } => println!("Creating sprint: {name}"),
                SprintAction::Start => println!("Starting sprint..."),
                SprintAction::Close => println!("Closing sprint..."),
                SprintAction::List => println!("Listing sprints..."),
            }
            // TODO: implement sprint management
        }
        Commands::Search { query } => {
            println!("Searching for: {query}");
            // TODO: implement search
        }
    }

    Ok(())
}
