//! What configures the background loops, owned by the crate that runs them.
//!
//! `tack-api`'s `AppConfig` embeds a [`LoopConfig`] (flattened, so `tack.toml` keys and
//! `TACK_*` variable names are what they always were) and the composition root, `tack serve`,
//! hands it to [`LoopConfig::start`] as a value. Nothing here reads the environment: whoever
//! loads the configuration does that once.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tack_db::Repository;
use tack_protocol::Clock;
use tracing::{info, warn};

use crate::background::Loops;
use crate::due_soon::DueSoonSink;
use crate::github_sync::GitHub;
use crate::ports::Ports;
use crate::remote_backup::BackupConfig;
use crate::{auto_start, due_soon, github_sync, observability, remote_backup, retention};

/// The floor of the automatic backup interval: `tokio::time::interval` panics on zero, and
/// anything faster would hammer the object store.
pub const MIN_BACKUP_INTERVAL_SECS: u64 = 60;

/// The settings of the loops `tack serve` runs and of the GitHub and backup destinations they
/// reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopConfig {
    /// Optional GitHub personal access token (`repo` scope). When set, item status changes and
    /// comments are pushed to linked GitHub issues, and the inbound poll may start. Never logged.
    #[serde(default)]
    pub github_token: Option<String>,

    /// GitHub API base URL. Override for GitHub Enterprise or to point tests at a mock server.
    #[serde(default = "default_github_api_base")]
    pub github_api_base: String,

    /// Inbound GitHub poll interval in seconds. `0` (the default) is off; the poll only starts
    /// when this is nonzero **and** `github_token` is set. See `docs/GITHUB-SYNC.md`.
    #[serde(default)]
    pub github_poll_seconds: u64,

    /// How often the board looks for tasks a project starts by itself, in seconds. `0` is off.
    #[serde(default = "default_auto_start_seconds")]
    pub auto_start_seconds: u64,

    /// S3-compatible endpoint URL. Omit for AWS S3; set for R2/B2/MinIO.
    #[serde(default)]
    pub backup_endpoint: Option<String>,
    /// Bucket name. Required to enable remote backup.
    #[serde(default)]
    pub backup_bucket: Option<String>,
    /// AWS/S3 region. Cloudflare R2 uses `auto`; AWS requires the real region.
    #[serde(default = "default_backup_region")]
    pub backup_region: String,
    /// S3 access key ID. Required to enable remote backup.
    #[serde(default)]
    pub backup_access_key: Option<String>,
    /// S3 secret access key. Required to enable remote backup. Never logged.
    #[serde(default)]
    pub backup_secret_key: Option<String>,
    /// Object key prefix inside the bucket. Default: `tack`.
    #[serde(default = "default_backup_prefix")]
    pub backup_prefix: String,
    /// Automatic backup interval in seconds. Omit (None) for manual-only backups.
    #[serde(default)]
    pub backup_interval_secs: Option<u64>,
    /// How many remote backups to retain after each upload. Default: 10.
    #[serde(default = "default_backup_retention")]
    pub backup_retention: usize,

    /// Enables the execution-domain retention sweep (see [`crate::retention`]). **Off by
    /// default**: this sweep deletes rows, so data deletion is an operator opt-in.
    #[serde(default)]
    pub execution_retention_enable: bool,
    /// Days of replay/idempotency bookkeeping and terminal `execution_events` history kept before
    /// the retention sweep purges them. Default: 90.
    #[serde(default = "default_execution_retention_days")]
    pub execution_retention_days: u32,
    /// Seconds between execution-retention sweeps. Default: 3600.
    #[serde(default = "default_execution_retention_interval_secs")]
    pub execution_retention_interval_secs: u64,

    /// Enables the execution-domain health watch (see [`crate::observability`]). **On by
    /// default**: it reads and logs only and deletes nothing.
    #[serde(default = "default_execution_health_enable")]
    pub execution_health_enable: bool,
    /// Seconds between execution health-watch checks. Default: 60.
    #[serde(default = "default_execution_health_interval_secs")]
    pub execution_health_interval_secs: u64,
}

fn default_github_api_base() -> String {
    "https://api.github.com".into()
}
fn default_auto_start_seconds() -> u64 {
    30
}
fn default_backup_region() -> String {
    "auto".into()
}
fn default_backup_prefix() -> String {
    "tack".into()
}
fn default_backup_retention() -> usize {
    10
}
fn default_execution_retention_days() -> u32 {
    retention::DEFAULT_RETENTION_DAYS
}
fn default_execution_retention_interval_secs() -> u64 {
    3600
}
fn default_execution_health_enable() -> bool {
    true
}
fn default_execution_health_interval_secs() -> u64 {
    60
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            github_token: None,
            github_api_base: default_github_api_base(),
            github_poll_seconds: 0,
            auto_start_seconds: default_auto_start_seconds(),
            backup_endpoint: None,
            backup_bucket: None,
            backup_region: default_backup_region(),
            backup_access_key: None,
            backup_secret_key: None,
            backup_prefix: default_backup_prefix(),
            backup_interval_secs: None,
            backup_retention: default_backup_retention(),
            execution_retention_enable: false,
            execution_retention_days: default_execution_retention_days(),
            execution_retention_interval_secs: default_execution_retention_interval_secs(),
            execution_health_enable: default_execution_health_enable(),
            execution_health_interval_secs: default_execution_health_interval_secs(),
        }
    }
}

impl LoopConfig {
    /// The GitHub this board is configured with: its API root and the environment's token.
    pub fn github(&self) -> GitHub {
        GitHub {
            github_token: self.github_token.clone(),
            api_base: self.github_api_base.clone(),
        }
    }

    /// The backup settings the environment configured, with the local files a bundle is made of.
    pub fn backup_config(
        &self,
        db_file: Option<std::path::PathBuf>,
        storage_dir: &str,
    ) -> BackupConfig {
        BackupConfig {
            endpoint: self.backup_endpoint.clone(),
            bucket: self.backup_bucket.clone(),
            region: self.backup_region.clone(),
            access_key: self.backup_access_key.clone(),
            secret_key: self.backup_secret_key.clone(),
            prefix: self.backup_prefix.clone(),
            retention: self.backup_retention,
            db_file,
            storage_dir: storage_dir.to_owned(),
        }
    }

    /// Whether the GitHub poll will run: an interval and a token are both set.
    pub fn github_poll_enabled(&self) -> bool {
        self.github_poll_seconds > 0 && self.github_token.is_some()
    }

    /// The backup interval after the floor, `None` when none is configured.
    pub fn backup_interval(&self) -> Option<Duration> {
        let requested = self.backup_interval_secs?;
        let secs = if requested < MIN_BACKUP_INTERVAL_SECS {
            warn!(
                requested,
                clamped_to = MIN_BACKUP_INTERVAL_SECS,
                "TACK_BACKUP_INTERVAL_SECS is below the 60s minimum; clamping"
            );
            MIN_BACKUP_INTERVAL_SECS
        } else {
            requested
        };
        Some(Duration::from_secs(secs))
    }

    /// Adds every loop this configuration turns on to `loops`. `ports` is what the loops reach
    /// outside the store, `backup` the files a backup bundle is made from (see
    /// [`Self::backup_config`]), and `due_soon` where the hourly announcement goes, `None` when
    /// no webhook is there to hear it.
    pub fn start(
        &self,
        loops: &mut Loops,
        repo: &Repository,
        clock: Arc<dyn Clock>,
        ports: Ports,
        backup: BackupConfig,
        due_soon: Option<Arc<dyn DueSoonSink>>,
    ) {
        if let Some(interval) = self.backup_interval() {
            // The destination can come from UI-saved settings too, so the loop spawns whenever an
            // interval is set and re-checks the effective destination each pass.
            remote_backup::spawn(loops, repo.clone(), clock.clone(), backup, interval);
            info!(
                interval_secs = interval.as_secs(),
                "Remote backup scheduler enabled"
            );
        }

        // Tasks a project starts by itself; off when its interval is 0.
        if self.auto_start_seconds > 0 {
            auto_start::spawn(
                loops,
                repo.clone(),
                clock.clone(),
                ports.clone(),
                Duration::from_secs(self.auto_start_seconds),
            );
        }

        // Inbound GitHub poll: starts nothing when polling is off or no token is set.
        github_sync::spawn(
            loops,
            repo.clone(),
            clock.clone(),
            ports,
            Duration::from_secs(self.github_poll_seconds),
        );

        if let Some(sink) = due_soon {
            due_soon::spawn(loops, repo.clone(), clock.clone(), sink, due_soon::INTERVAL);
        }

        // Purges stale replay bookkeeping and terminal events and expires overdue decisions; off
        // unless the operator opts in, since it deletes rows.
        if self.execution_retention_enable {
            retention::spawn(
                loops,
                repo.clone(),
                clock.clone(),
                retention::Settings {
                    retention_days: self.execution_retention_days,
                    batch_size: retention::DEFAULT_BATCH_SIZE,
                },
                Duration::from_secs(self.execution_retention_interval_secs),
            );
        }

        // Logs a stale lease or a request stuck in needs_operator; on by default, it only reads.
        if self.execution_health_enable {
            observability::spawn(
                loops,
                repo.clone(),
                clock,
                observability::Settings::default(),
                Duration::from_secs(self.execution_health_interval_secs),
            );
        }
    }
}
