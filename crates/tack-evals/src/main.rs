use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tack_evals::exam::{SolutionVerdict, check_solution};
use tack_evals::record::TrialStatus;
use tack_evals::trial::{TrialOptions, run_and_record};
use tack_evals::world::WorldOptions;
use tack_evals::{load_tasks, locate_tack_bin};

#[derive(Parser)]
#[command(name = "tack-evals", about = "Measure Tack's runs with a real agent")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run tasks through Tack in throwaway worlds and append each trial to trials.jsonl.
    Run {
        /// Task directories, or directories holding task directories.
        #[arg(required = true)]
        tasks: Vec<PathBuf>,
        /// Where trials.jsonl and the frozen runs are written.
        #[arg(long, default_value = "evals-out")]
        out: PathBuf,
        /// Trials per task.
        #[arg(long, default_value_t = 1)]
        epochs: u32,
        /// Run every task with this agent, those that apply to it.
        #[arg(long)]
        harness: Option<String>,
        /// The `tack` binary to serve with.
        #[arg(long)]
        tack_bin: Option<PathBuf>,
        /// A directory put in front of the server's PATH, where the agent's binary lives.
        #[arg(long = "agent-path")]
        agent_path: Vec<PathBuf>,
        /// An environment variable handed to the agent as a secret (an API key). Its value is
        /// read from this process and never printed or recorded.
        #[arg(long = "env-secret")]
        env_secret: Vec<String>,
        /// Keep each world's directory after its trial.
        #[arg(long)]
        keep_worlds: bool,
    },
    /// Apply each task's reference solution and run its hidden checks, with no model. Exits 1
    /// when any solution fails: the task is broken.
    CheckSolutions {
        #[arg(required = true)]
        tasks: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("tack-evals: {failure:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::CheckSolutions { tasks } => check_solutions(&tasks),
        Command::Run {
            tasks,
            out,
            epochs,
            harness,
            tack_bin,
            agent_path,
            env_secret,
            keep_worlds,
        } => {
            let world = WorldOptions {
                tack_bin: locate_tack_bin(tack_bin)?,
                agent_path,
                env_secrets: env_secret,
                keep: keep_worlds,
            };
            run_tasks(&tasks, &out, epochs, harness.as_deref(), world)
        }
    }
}

fn check_solutions(paths: &[PathBuf]) -> Result<ExitCode> {
    let mut broken = 0;
    for task in load_tasks(paths)? {
        match check_solution(&task)? {
            SolutionVerdict::Passed => println!("ok       {}", task.task.id),
            SolutionVerdict::Skipped(why) => println!("skipped  {}: {why}", task.task.id),
            SolutionVerdict::Broken(why) => {
                broken += 1;
                println!("BROKEN   {}: {why}", task.task.id);
            }
        }
    }
    Ok(if broken == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn run_tasks(
    paths: &[PathBuf],
    out: &std::path::Path,
    epochs: u32,
    harness: Option<&str>,
    world: WorldOptions,
) -> Result<ExitCode> {
    let mut errors = 0;
    for task in load_tasks(paths)? {
        let wanted = harness.unwrap_or(&task.task.profile.harness);
        if task
            .task
            .harnesses
            .as_ref()
            .is_some_and(|all| !all.iter().any(|h| h == wanted))
        {
            continue;
        }
        for epoch in 1..=epochs {
            let options = TrialOptions {
                out: out.to_owned(),
                epoch,
                harness: harness.map(str::to_owned),
                world: world.clone(),
            };
            let record = run_and_record(&task, &options)?;
            println!(
                "{:<20} epoch {epoch}  {:?}  run {}  {}",
                record.task,
                record.status,
                record.run_state.as_deref().unwrap_or("-"),
                record.reason.as_deref().unwrap_or("")
            );
            if record.status == TrialStatus::Error {
                errors += 1;
            }
        }
    }
    Ok(if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
