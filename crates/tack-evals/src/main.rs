use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tack_evals::api::Api;
use tack_evals::exam::{SolutionVerdict, check_solution};
use tack_evals::grade::{GradedTrial, grade_file, grade_trial};
use tack_evals::graders::verifier::{Verdicts, score_markdown, score_set};
use tack_evals::promote::{PromoteOptions, promote, refusals};
use tack_evals::record::{self, TrialRecord, TrialStatus};
use tack_evals::report::{
    compare, compare_markdown, failures_markdown, load_summary, summarize, warnings, write_summary,
};
use tack_evals::trial::{TrialOptions, run_and_record};
use tack_evals::verifier_set::generate::{Options, check_set, generate};
use tack_evals::verifier_set::{Expected, load_set, unconfirmed};
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
    /// Run tasks through Tack in throwaway worlds, append each trial to trials.jsonl and write
    /// summary.md and summary.json.
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
        #[command(flatten)]
        agent: AgentArgs,
    },
    /// Set a run against a base: pair the tasks by id, version and agent and report the
    /// difference with its interval. Informational; always exits 0.
    Compare {
        /// The base: a summary.json, a directory holding one, a trials.jsonl, or the name of a
        /// directory under evals/baselines/.
        #[arg(long)]
        base: String,
        /// The run to judge, in any of the same forms.
        #[arg(long)]
        head: PathBuf,
        /// Task directories, needed to judge a trials.jsonl.
        #[arg(long = "tasks", num_args = 1..)]
        tasks: Vec<PathBuf>,
        /// Also write compare.md here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// The scheduled regression run: run the tasks, set them against the base and warn about
    /// every task that passed all its trials there and fails two or more here. Exits 1 only
    /// when it warns. Failures of the eval and of the infrastructure never warn.
    Nightly {
        #[arg(required = true)]
        tasks: Vec<PathBuf>,
        #[arg(long)]
        base: String,
        #[arg(long, default_value = "evals-out")]
        out: PathBuf,
        #[arg(long, default_value_t = 3)]
        epochs: u32,
        /// Stop starting trials once this many tokens have been counted.
        #[arg(long)]
        token_cap: Option<u64>,
        /// Judge the trials in this file and run nothing.
        #[arg(long)]
        trials: Option<PathBuf>,
        #[command(flatten)]
        agent: AgentArgs,
    },
    /// Apply each task's reference solution and run its hidden checks, with no model. Exits 1
    /// when any solution fails: the task is broken. A candidate from `promote` is refused until a
    /// person has written its exam and its solution.
    CheckSolutions {
        #[arg(required = true)]
        tasks: Vec<PathBuf>,
    },
    /// Make the verifier set from the reference solutions of the regression tasks: good cases
    /// (reference solutions and harmless variants) and mutants, each proved by running the
    /// task's own checks. Every label starts unconfirmed. With --check, re-prove a written set
    /// instead and exit 1 when a case no longer holds.
    GenVerifierSet {
        /// Task directories, or directories holding task directories.
        #[arg(required = true)]
        tasks: Vec<PathBuf>,
        #[arg(long, default_value = "evals/tasks/verifier")]
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Cases that should merge.
        #[arg(long, default_value_t = 15)]
        good: usize,
        /// Cases that should be rejected.
        #[arg(long, default_value_t = 15)]
        bad: usize,
        #[arg(long)]
        check: bool,
    },
    /// Make a candidate for the capability suite from an attempt a person rejected (or, with
    /// --reason, work that was merged and reverted): the repository at the base commit as a git
    /// bundle, the item and its brief, the reason and a pointer to the evidence, anonymised, in
    /// <out>/<attempt id>/. Reads the server; changes nothing on it. The exam and the reference
    /// solution are written by a person.
    Promote {
        attempt_id: String,
        /// The server that holds the attempt.
        #[arg(long, default_value = "http://127.0.0.1:3210")]
        server: String,
        /// The environment variable holding the server's API token, if it asks for one.
        #[arg(long, default_value = "TACK_API_TOKEN")]
        token_env: String,
        /// The run (request id) the attempt belongs to; faster than searching the server's runs.
        #[arg(long)]
        run: Option<String>,
        /// A clone to read the base commit from, when the project's repository is not a local
        /// folder.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Why the work was refused, for a revert (the board records no reason for one).
        #[arg(long)]
        reason: Option<String>,
        /// A name to take out besides the project's and the repository's (repeatable).
        #[arg(long = "name")]
        names: Vec<String>,
        /// An environment variable whose value is a secret to take out wherever it appears
        /// (repeatable). Never printed.
        #[arg(long = "env-secret")]
        env_secret: Vec<String>,
        #[arg(long, default_value = "evals/candidates")]
        out: PathBuf,
    },
    /// List the verifier-set labels no person has confirmed yet.
    VerifierLabels {
        #[arg(default_value = "evals/tasks/verifier")]
        set: PathBuf,
    },
    /// Score verdicts (a JSON file of case id to `{prill, reviewer}` words) against the set's
    /// labels: false approval and false rejection per verifier. Refuses a set with unconfirmed
    /// labels unless --allow-unconfirmed.
    ScoreVerifier {
        #[arg(default_value = "evals/tasks/verifier")]
        set: PathBuf,
        #[arg(long)]
        verdicts: PathBuf,
        #[arg(long)]
        allow_unconfirmed: bool,
        /// Also write verifier-score.json and verifier-score.md here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

/// How trials are run: which agent, with which binary, under which secrets.
#[derive(clap::Args)]
struct AgentArgs {
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
}

impl AgentArgs {
    fn world(&self) -> Result<WorldOptions> {
        Ok(WorldOptions {
            tack_bin: locate_tack_bin(self.tack_bin.clone())?,
            agent_path: self.agent_path.clone(),
            env_secrets: self.env_secret.clone(),
            keep: self.keep_worlds,
        })
    }
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
        Command::GenVerifierSet {
            tasks,
            out,
            seed,
            good,
            bad,
            check,
        } => gen_verifier_set(&tasks, &out, &Options { seed, good, bad }, check),
        Command::Promote {
            attempt_id,
            server,
            token_env,
            run,
            repo,
            reason,
            names,
            env_secret,
            out,
        } => {
            let token = std::env::var(&token_env).ok();
            let api = Api::with_token(&server, token.as_deref())?;
            let options = PromoteOptions {
                out,
                run,
                repo,
                reason,
                names,
                secrets: env_secret
                    .iter()
                    .filter_map(|name| std::env::var(name).ok())
                    .collect(),
            };
            let promoted = promote(&api, &attempt_id, &options)?;
            println!("candidate  {}", promoted.dir.display());
            for file in &promoted.files {
                println!("  {file}");
            }
            for warning in &promoted.warnings {
                println!("WARNING  {warning}");
            }
            println!(
                "read it before anything else: anonymising is not proof. Then write hidden/ and                  solution/, unbundle repo.bundle into repo/ and add task.yaml (evals/README.md)"
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::VerifierLabels { set } => verifier_labels(&set),
        Command::ScoreVerifier {
            set,
            verdicts,
            allow_unconfirmed,
            out,
        } => score_verifier(&set, &verdicts, allow_unconfirmed, out.as_deref()),
        Command::Run {
            tasks,
            out,
            epochs,
            agent,
        } => {
            let batch = run_tasks(&tasks, &out, epochs, &agent, None)?;
            // The summary covers every trial in `out`, so one directory can collect the agents
            // one run after another.
            let all = grade_file(&record::trials_path(&out), &load_tasks(&tasks)?)?;
            write_summary(&out, &summarize(&all), None)?;
            std::fs::write(out.join("failures.md"), failures_markdown(&all))?;
            Ok(exit_code(batch.errors > 0))
        }
        Command::Compare {
            base,
            head,
            tasks,
            out,
        } => {
            let tasks = load_tasks(&tasks)?;
            let comparison = compare(
                &load_summary(&resolve_base(&base), &tasks)?,
                &load_summary(&head, &tasks)?,
            );
            let text = compare_markdown(&comparison);
            print!("{text}");
            if let Some(out) = out {
                std::fs::create_dir_all(&out)?;
                std::fs::write(out.join("compare.md"), text)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Nightly {
            tasks,
            base,
            out,
            epochs,
            token_cap,
            trials,
            agent,
        } => nightly(
            &tasks,
            &base,
            &out,
            epochs,
            token_cap,
            trials.as_deref(),
            &agent,
        ),
    }
}

fn exit_code(failed: bool) -> ExitCode {
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `evals/baselines/<name>` when `base` is not a path that exists.
fn resolve_base(base: &str) -> PathBuf {
    let path = PathBuf::from(base);
    if path.exists() {
        path
    } else {
        PathBuf::from("evals/baselines").join(base)
    }
}

fn nightly(
    paths: &[PathBuf],
    base: &str,
    out: &std::path::Path,
    epochs: u32,
    token_cap: Option<u64>,
    trials: Option<&std::path::Path>,
    agent: &AgentArgs,
) -> Result<ExitCode> {
    let tasks = load_tasks(paths)?;
    let (graded, errors) = match trials {
        Some(file) => (grade_file(file, &tasks)?, 0),
        None => {
            let batch = run_tasks(paths, out, epochs, agent, token_cap)?;
            (batch.graded, batch.errors)
        }
    };
    let head = summarize(&graded);
    let comparison = compare(&load_summary(&resolve_base(base), &tasks)?, &head);
    let raised = warnings(&comparison);
    write_summary(out, &head, Some(&comparison))?;
    std::fs::write(out.join("compare.md"), compare_markdown(&comparison))?;
    std::fs::write(
        out.join("warnings.json"),
        serde_json::to_vec_pretty(&raised)?,
    )?;
    for warning in &raised {
        println!("WARNING  {}", warning.message);
    }
    if errors > 0 {
        eprintln!("tack-evals: {errors} trials could not run; see the trials.jsonl");
    }
    println!(
        "{} warnings; {}",
        raised.len(),
        tack_evals::report::verdict(&comparison)
    );
    Ok(exit_code(!raised.is_empty()))
}

fn check_solutions(paths: &[PathBuf]) -> Result<ExitCode> {
    let mut broken = 0;
    for refusal in refusals(paths)? {
        broken += 1;
        println!("REFUSED  {}", refusal.message());
    }
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

fn gen_verifier_set(
    paths: &[PathBuf],
    out: &std::path::Path,
    options: &Options,
    check: bool,
) -> Result<ExitCode> {
    let tasks = load_tasks(paths)?;
    if check {
        let problems = check_set(out, &tasks)?;
        for problem in &problems {
            println!("STALE    {problem}");
        }
        if problems.is_empty() {
            println!("ok       every case still holds");
        }
        return Ok(exit_code(!problems.is_empty()));
    }
    let written = generate(&tasks, out, options)?;
    for case in &written {
        println!(
            "{}  {:<6}  {:<14}  {}",
            case.id,
            match case.label.expected {
                Expected::Merge => "merge",
                Expected::Reject => "reject",
            },
            case.label.family.as_deref().unwrap_or("-"),
            case.task
        );
    }
    println!(
        "{} cases written to {}; every label is unconfirmed: see `tack-evals verifier-labels`",
        written.len(),
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn verifier_labels(set: &std::path::Path) -> Result<ExitCode> {
    let cases = load_set(set)?;
    let waiting = unconfirmed(&cases);
    for case in &waiting {
        println!(
            "{}  expected {:<6}  {}\n    {}\n    {}",
            case.id,
            match case.label.expected {
                Expected::Merge => "merge",
                Expected::Reject => "reject",
            },
            case.reference.task,
            case.diff_path().display(),
            case.label.reason
        );
    }
    println!(
        "{} of {} labels unconfirmed; set `confirmed: true` in a case's label.yaml once its diff \
         is read and `expected` is right",
        waiting.len(),
        cases.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn score_verifier(
    set: &std::path::Path,
    verdicts: &std::path::Path,
    allow_unconfirmed: bool,
    out: Option<&std::path::Path>,
) -> Result<ExitCode> {
    let cases = load_set(set)?;
    let verdicts: Verdicts = serde_json::from_slice(&std::fs::read(verdicts)?)?;
    let score = score_set(&cases, &verdicts, allow_unconfirmed)?;
    let text = score_markdown(&score);
    print!("{text}");
    if let Some(out) = out {
        std::fs::create_dir_all(out)?;
        std::fs::write(out.join("verifier-score.md"), &text)?;
        std::fs::write(
            out.join("verifier-score.json"),
            serde_json::to_vec_pretty(&score)?,
        )?;
    }
    Ok(ExitCode::SUCCESS)
}

struct Batch {
    graded: Vec<GradedTrial>,
    /// Trials the harness itself could not run.
    errors: usize,
}

fn run_tasks(
    paths: &[PathBuf],
    out: &std::path::Path,
    epochs: u32,
    agent: &AgentArgs,
    token_cap: Option<u64>,
) -> Result<Batch> {
    let world = agent.world()?;
    let harness = agent.harness.as_deref();
    let mut batch = Batch {
        graded: Vec::new(),
        errors: 0,
    };
    let mut spent: u64 = 0;
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
        if let Some(reason) = task.task.not_applicable.get(wanted) {
            for epoch in 1..=epochs {
                let record = TrialRecord::without_run(
                    &task,
                    wanted,
                    epoch,
                    TrialStatus::NotApplicable,
                    reason.clone(),
                );
                record::append(out, &record)?;
                println!("{:<20} epoch {epoch}  NotApplicable  {reason}", record.task);
                batch.graded.push(grade_trial(&task, &record, out));
            }
            continue;
        }
        for epoch in 1..=epochs {
            if token_cap.is_some_and(|cap| spent >= cap) {
                println!("token cap reached after {spent} tokens; no more trials are started");
                return Ok(batch);
            }
            let options = TrialOptions {
                out: out.to_owned(),
                epoch,
                harness: harness.map(str::to_owned),
                world: world.clone(),
            };
            let record = run_and_record(&task, &options)?;
            spent += record.counted_tokens.unwrap_or(0);
            println!(
                "{:<20} epoch {epoch}  {:?}  run {}  {}",
                record.task,
                record.status,
                record.run_state.as_deref().unwrap_or("-"),
                record.reason.as_deref().unwrap_or("")
            );
            if record.status == TrialStatus::Error {
                batch.errors += 1;
            }
            batch.graded.push(grade_trial(&task, &record, out));
        }
    }
    Ok(batch)
}
