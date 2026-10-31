use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

use tack_cli::client::TackClient;
use tack_cli::config::Config;

use cmd::backup::cmd_list_remote_backups;
use cmd::board::{
    ProjectCommands, cmd_add, cmd_board, cmd_branch, cmd_config, cmd_export, cmd_init, cmd_list,
    cmd_move, cmd_open, cmd_project_autonomy, cmd_projects, cmd_search, cmd_start,
};
use cmd::entities::{CommentAction, FieldAction, SprintAction, cmd_role, cmd_template};
use cmd::entities::{RoleAction, TemplateAction};
use cmd::runs::{AgentProfileAction, ExecutionAction, FleetAction, RunnerAction};
use cmd::runs::{SecretAction, ServiceAction};

mod cmd;
mod doctor;
mod local_enrollment;
mod local_runner;
mod secret;
mod serve;
mod service;

#[derive(Parser)]
#[command(
    name = "tack",
    version,
    about = "Tack — Flexible Project Management CLI"
)]
struct Cli {
    /// Tack API base URL
    #[arg(long, env = "TACK_API_URL")]
    api_url: Option<String>,

    /// Bearer token for authentication
    #[arg(long, env = "TACK_API_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Tack server and web UI (this is the default when you run
    /// `tack` with no subcommand)
    Serve {
        /// Also start an embedded runner in this process, speaking runner-v1
        /// over loopback HTTP exactly like a remote runner. Off by default;
        /// also settable via TACK_LOCAL_RUNNER_ENABLE=1. Refused on startup
        /// if the server is not bound to loopback.
        #[arg(long)]
        with_runner: bool,
        /// A TOML configuration file for the embedded runner — the same file
        /// `tack runner start --config` reads, which is the only place its
        /// `[verify]`, `[git]`, `[secrets]` and `[environment]` tables can be set.
        /// Also settable via TACK_LOCAL_RUNNER_CONFIG.
        #[arg(long, value_name = "PATH")]
        runner_config: Option<PathBuf>,
    },

    /// Create a new project
    Init {
        /// Project name
        name: String,
        /// Project type (software, web, mobile, construction, personal, homework, maintenance, legal, research, event, custom)
        #[arg(short = 't', long, default_value = "software")]
        r#type: String,
        /// Optional description
        #[arg(short, long)]
        description: Option<String>,
        /// Start from a template: its id, or its name ("web-product-with-content"
        /// reads as "Web product with content"). The template brings its roles,
        /// profiles and automation; --type is ignored
        #[arg(long)]
        template: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Write a project to a file or stdout: its items as JSON, YAML or CSV, or
    /// with --bundle the whole project file (fields, roles, profiles, sprints,
    /// automation and items) that `tack import` reads back
    Export {
        /// Project ID or name
        project: String,
        /// json, yaml or csv (default yaml with --bundle, else json)
        #[arg(long)]
        format: Option<String>,
        /// Export the whole project file instead of the items alone
        #[arg(long)]
        bundle: bool,
        /// Write to this file instead of stdout
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },

    /// Create a project and its items, or add items to a project, from an
    /// outline (.json, .yaml, .yml) or a CSV file. Nothing is written unless
    /// the whole file can be.
    Import {
        /// The file: an outline as JSON or YAML, or a CSV with a header row
        file: PathBuf,
        /// Add the items to this project instead of creating one
        #[arg(short = 'p', long)]
        project: Option<String>,
        /// The new project's name; overrides an outline's. A CSV defaults to
        /// the file's name
        #[arg(long, conflicts_with = "project")]
        name: Option<String>,
        /// The new project's type (default custom); overrides an outline's
        #[arg(short = 't', long = "type", conflicts_with = "project")]
        project_type: Option<String>,
        /// Check the file and show what would be created, writing nothing
        #[arg(long)]
        dry_run: bool,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// List projects
    Projects {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// A project's settings
    Project {
        #[command(subcommand)]
        action: ProjectCommands,
    },

    /// Add a new item to a project
    Add {
        /// Item title
        title: String,
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Item type (task, epic, feature, bug, subtask, requirement)
        #[arg(short = 't', long, default_value = "task")]
        item_type: String,
        /// Priority (critical, high, medium, low)
        #[arg(long, default_value = "medium")]
        priority: String,
        /// Parent item ID
        #[arg(long)]
        parent: Option<String>,
        /// Assignee name
        #[arg(short, long)]
        assignee: Option<String>,
        /// Sprint ID
        #[arg(short, long)]
        sprint: Option<String>,
        /// Repeat the task when it is closed
        #[arg(long, value_parser = ["weekly", "monthly", "quarterly"])]
        recurrence: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// List items in a project
    List {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by item type
        #[arg(long)]
        item_type: Option<String>,
        /// Filter by assignee
        #[arg(short, long)]
        assignee: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Move an item to a new status
    Move {
        /// Item ID
        id: String,
        /// Target status
        status: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Show board view for a project
    Board {
        /// Project ID
        #[arg(short = 'p', long)]
        project: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Derive a git branch name from an item (and optionally create it)
    Branch {
        /// Item ID
        id: String,
        /// Create and switch to the branch (runs `git checkout -b`)
        #[arg(short, long)]
        checkout: bool,
        /// Override the type-derived prefix (e.g. `hotfix`, `wip`)
        #[arg(long)]
        prefix: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Move an item to the first in-progress status and check out its branch
    Start {
        /// Item ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Print an item's web URL, opening it in $BROWSER when set
    Open {
        /// Item ID
        id: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Run a Model Context Protocol (MCP) server over stdio for AI agents
    Mcp,

    /// Search items
    Search {
        /// Search query
        query: String,
        /// Project ID (optional; searches globally if omitted)
        #[arg(short = 'p', long)]
        project: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Sprint management
    Sprint {
        #[command(subcommand)]
        action: SprintAction,
    },

    /// Write ~/.tackrc with connection settings
    Config {
        /// API base URL to save
        #[arg(long)]
        url: Option<String>,
        /// Bearer token to save (omit to clear)
        #[arg(long)]
        token: Option<String>,
        /// Show current config without changing it
        #[arg(long)]
        show: bool,
    },

    /// Print shell completion script to stdout
    Completions {
        /// Target shell (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },

    /// Download a local backup or trigger a remote cloud backup
    Backup {
        /// Where to save the local backup (default: tack-backup.db). Ignored with --remote.
        path: Option<std::path::PathBuf>,
        /// Upload a new backup to the configured S3-compatible cloud store instead of downloading locally
        #[arg(long)]
        remote: bool,
        /// Output raw JSON (only applies to --remote)
        #[arg(long)]
        json: bool,
    },

    /// List remote cloud backups
    Backups {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Stage a backup file for restore on next server restart
    Restore {
        /// Path to the backup file produced by `tack backup` (omit with --remote)
        path: Option<std::path::PathBuf>,
        /// Restore from remote cloud storage instead of a local file
        #[arg(long)]
        remote: bool,
        /// Specific remote backup key to restore (defaults to latest when using --remote)
        #[arg(long)]
        key: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage project templates
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },

    /// Manage project roles (specialties / disciplines)
    Role {
        #[command(subcommand)]
        action: RoleAction,
    },

    /// Manage item comments
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },

    /// Manage custom field definitions and item values
    Field {
        #[command(subcommand)]
        action: FieldAction,
    },

    /// Create/list/cancel/reconcile agent-fleet execution requests
    Execution {
        #[command(subcommand)]
        action: ExecutionAction,
    },

    /// Manage runner fleets
    Fleet {
        #[command(subcommand)]
        action: FleetAction,
    },

    /// Enroll and revoke execution runners
    Runner {
        #[command(subcommand)]
        action: RunnerAction,
    },

    /// Manage tack as a background service that outlives the terminal
    /// (a systemd user unit on Linux, a launchd agent on macOS)
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Manage agent profiles (instructions, tool policy, limits)
    AgentProfile {
        #[command(subcommand)]
        action: AgentProfileAction,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Bare `tack` (no subcommand) launches the server + web UI — the primary,
    // UI-first experience. `tack serve` does the same explicitly.
    let Some(command) = cli.command else {
        return run_server(false, None);
    };

    // Serve, Runner Start, Runner Doctor, Service, Config and Completions
    // don't need a live API client: Serve and Runner Start each build their
    // own async runtime and speak the runner-v1/HTTP protocol directly,
    // never through `TackClient`; Runner Doctor only probes this machine's
    // own harness installations; Service only writes/reads a unit file and
    // shells out to the OS's own service manager.
    let command = match command {
        Commands::Serve {
            with_runner,
            runner_config,
        } => {
            return run_server(with_runner, runner_config);
        }
        Commands::Runner {
            action:
                RunnerAction::Start {
                    config,
                    api_url,
                    runner_id,
                    state_dir,
                    enrollment_token,
                },
        } => {
            return local_runner::run_standalone(
                config,
                api_url,
                runner_id,
                state_dir,
                enrollment_token,
            );
        }
        Commands::Runner {
            action: RunnerAction::Doctor { json, config },
        } => {
            return doctor::run(json, config);
        }
        Commands::Runner {
            action: RunnerAction::Secret { action },
        } => {
            return secret::run(action);
        }
        Commands::Service { action } => {
            return match action {
                ServiceAction::Install => service::install(),
                ServiceAction::Uninstall => service::uninstall(),
                ServiceAction::Status => service::status(),
            };
        }
        Commands::Config { url, token, show } => {
            return cmd_config(
                url.as_deref(),
                token.as_deref(),
                show,
                &cli.api_url,
                &cli.token,
            );
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "tack", &mut std::io::stdout());
            return Ok(());
        }
        other => other,
    };

    let config = Config::load(cli.api_url, cli.token);
    let client = TackClient::new(&config)?;

    match command {
        Commands::Init {
            name,
            r#type,
            description,
            template,
            json,
        } => cmd_init(&client, name, r#type, description, template, json),

        Commands::Export {
            project,
            format,
            bundle,
            output,
        } => cmd_export(&client, &project, format, bundle, output),

        Commands::Import {
            file,
            project,
            name,
            project_type,
            dry_run,
            json,
        } => cmd::board::run_import(&client, &file, project, name, project_type, dry_run, json),

        Commands::Projects { json } => cmd_projects(&client, json),

        Commands::Project {
            action: ProjectCommands::Autonomy { project, json },
        } => cmd_project_autonomy(&client, &project, json),

        Commands::Add {
            title,
            project,
            item_type,
            priority,
            parent,
            assignee,
            sprint,
            recurrence,
            json,
        } => cmd_add(
            &client, title, project, item_type, priority, parent, assignee, sprint, recurrence,
            json,
        ),

        Commands::List {
            project,
            status,
            item_type,
            assignee,
            json,
        } => cmd_list(&client, project, status, item_type, assignee, json),

        Commands::Move { id, status, json } => cmd_move(&client, id, status, json),

        Commands::Board { project, json } => cmd_board(&client, project, json),

        Commands::Branch {
            id,
            checkout,
            prefix,
            json,
        } => cmd_branch(&client, id, checkout, prefix, json),

        Commands::Start { id, json } => cmd_start(&client, id, json),

        Commands::Open { id, json } => cmd_open(&client, &config.base_url, id, json),

        Commands::Mcp => tack_cli::mcp::run(&client),

        Commands::Search {
            query,
            project,
            json,
        } => cmd_search(&client, query, project, json),

        Commands::Backup { path, remote, json } => {
            cmd::backup::run_backup(&client, path, remote, json)
        }

        Commands::Backups { json } => cmd_list_remote_backups(&client, json),

        Commands::Restore {
            path,
            remote,
            key,
            json,
        } => cmd::backup::run_restore(&client, path, remote, key, json),

        Commands::Template { action } => cmd_template(&client, action),

        Commands::Role { action } => cmd_role(&client, action),

        Commands::Comment { action } => cmd::entities::run_comment(&client, action),

        Commands::Sprint { action } => cmd::entities::run_sprint(&client, action),

        Commands::Field { action } => cmd::entities::run_field(&client, action),

        Commands::Execution { action } => cmd::runs::run_execution(&client, action),

        Commands::Fleet { action } => cmd::runs::run_fleet(&client, action),

        Commands::Runner { action } => cmd::runs::run_runner(&client, action),

        // Already handled above; unreachable but required for exhaustiveness.
        Commands::Service { .. } => unreachable!(),

        Commands::AgentProfile { action } => cmd::runs::run_agent_profile(&client, action),

        // Already handled above; unreachable but required for exhaustiveness.
        Commands::Serve { .. } | Commands::Config { .. } | Commands::Completions { .. } => {
            unreachable!()
        }
    }
}

/// Start the in-process HTTP server (the app + embedded web UI) with an
/// embedded runner always wired in — see `local_runner::serve`'s own doc
/// comment for why even a plain `tack serve` needs one (ADR 0005 decisions
/// 2 and 6: the UI can turn it on later, with no restart). `with_runner`
/// only decides whether it also starts immediately at boot: it is folded
/// into the loaded configuration as a value, so the flag and
/// `TACK_LOCAL_RUNNER_ENABLE` take one path. The environment is read here,
/// once, and handed down.
///
/// The rest of the CLI is synchronous (it uses a blocking HTTP client), so we
/// build a Tokio runtime on demand here rather than making `main` async.
fn run_server(with_runner: bool, runner_config: Option<PathBuf>) -> anyhow::Result<()> {
    let mut config = tack_api::config::AppConfig::load();
    config.local_runner_enable |= with_runner;
    let environment = tack_cli::config::ServeEnvironment::from_process();
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(local_runner::serve(config, runner_config, &environment))
}
