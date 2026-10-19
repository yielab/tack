use super::*;

const LIMIT: Duration = Duration::from_secs(30);

/// A state directory and a checkout under it holding `lockfile`, as the runner lays them out.
struct Setup {
    _dir: tempfile::TempDir,
    state: PathBuf,
}

impl Setup {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temporary directory");
        let state = dir.path().to_path_buf();
        Self { _dir: dir, state }
    }

    /// A checkout named `name` holding `package-lock.json` with `lock` in it.
    fn checkout(&self, name: &str, lock: &str) -> PathBuf {
        let checkout = self.state.join("workspaces").join(name);
        fs::create_dir_all(&checkout).expect("checkout");
        fs::write(checkout.join("package-lock.json"), lock).expect("lockfile");
        checkout
    }

    /// A command that installs a `node_modules` holding `x`, noting each time it runs.
    fn install_command(&self) -> String {
        format!(
            "echo ran >> {} && mkdir node_modules && touch node_modules/x",
            self.state.join("ran").display()
        )
    }

    fn runs(&self) -> usize {
        fs::read_to_string(self.state.join("ran")).map_or(0, |ran| ran.lines().count())
    }

    fn entry(&self, lock: &str) -> PathBuf {
        self.state
            .join("prepare-cache")
            .join(sha256_hex(lock.as_bytes()))
    }
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Whether two paths are the same file on disk.
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let (a, b) = (fs::metadata(a).expect("a"), fs::metadata(b).expect("b"));
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[tokio::test]
async fn a_detected_install_runs_once_and_the_next_attempt_links_what_it_kept() {
    let setup = Setup::new();
    let command = setup.install_command();
    let first = setup.checkout("a1", "lock one");

    install(
        &first,
        &command,
        Some("package-lock.json"),
        &setup.state,
        LIMIT,
    )
    .await
    .expect("first install");

    assert_eq!(setup.runs(), 1);
    assert!(
        !is_link(&first.join("node_modules")),
        "the folder is the checkout's own"
    );
    assert!(first.join("node_modules/.tack-prepared").is_file());
    let entry = setup.entry("lock one");
    assert!(same_file(
        &first.join("node_modules/x"),
        &entry.join("node_modules/x")
    ));
    assert!(
        !entry.join("node_modules/.tack-prepared").exists(),
        "the cache has no marker"
    );
    let leftovers: Vec<_> = fs::read_dir(setup.state.join("prepare-cache"))
        .expect("cache")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(
        leftovers,
        [entry.file_name().expect("name")],
        "no staging left"
    );

    let second = setup.checkout("a2", "lock one");
    install(
        &second,
        &command,
        Some("package-lock.json"),
        &setup.state,
        LIMIT,
    )
    .await
    .expect("second install");
    assert_eq!(
        setup.runs(),
        1,
        "the second attempt does not run the command"
    );
    assert!(!is_link(&second.join("node_modules")));
    assert!(same_file(
        &second.join("node_modules/x"),
        &entry.join("node_modules/x")
    ));
    let marker = second.join("node_modules/.tack-prepared");
    assert!(marker.is_file(), "the reused install is marked");
    assert!(
        !same_file(&marker, &first.join("node_modules/.tack-prepared")),
        "the marker is its own file"
    );

    let other = setup.checkout("a3", "lock two");
    install(
        &other,
        &command,
        Some("package-lock.json"),
        &setup.state,
        LIMIT,
    )
    .await
    .expect("another lockfile");
    assert_eq!(setup.runs(), 2, "a different lockfile installs again");

    // A checkout's removal leaves what the cache kept.
    fs::remove_dir_all(&second).expect("remove");
    assert!(entry.join("node_modules/x").exists());
}

#[tokio::test]
async fn a_detected_command_is_resolved_from_the_lockfile_and_hits_the_cache() {
    let setup = Setup::new();
    let entry = setup.entry("kept");
    fs::create_dir_all(entry.join("node_modules")).expect("entry");
    fs::write(entry.join("node_modules/x"), "").expect("x");
    let checkout = setup.checkout("a1", "kept");

    // `npm ci` would fail here; the kept install answers instead.
    prepare(&checkout, &PrepareSetting::Detect, &setup.state, LIMIT)
        .await
        .expect("prepared");

    assert!(checkout.join("node_modules/x").exists());
    assert!(checkout.join("node_modules/.tack-prepared").is_file());
}

#[tokio::test]
async fn an_install_turned_off_or_without_a_lockfile_runs_nothing() {
    let setup = Setup::new();
    let with_lockfile = setup.checkout("a1", "lock");
    prepare(&with_lockfile, &PrepareSetting::Off, &setup.state, LIMIT)
        .await
        .expect("off");
    let without = setup.state.join("workspaces/a2");
    fs::create_dir_all(&without).expect("checkout");
    prepare(&without, &PrepareSetting::Detect, &setup.state, LIMIT)
        .await
        .expect("no lockfile");

    for checkout in [&with_lockfile, &without] {
        assert!(!checkout.join("node_modules").exists());
    }
    assert!(!setup.state.join("prepare-cache").exists());
}

#[tokio::test]
async fn a_replaced_command_runs_every_time_and_keeps_nothing() {
    let setup = Setup::new();
    let setting = PrepareSetting::Replace {
        command: setup.install_command(),
    };
    for name in ["a1", "a2"] {
        let checkout = setup.checkout(name, "lock");
        prepare(&checkout, &setting, &setup.state, LIMIT)
            .await
            .expect("replaced");
        assert!(checkout.join("node_modules/x").exists());
        assert!(!checkout.join("node_modules/.tack-prepared").exists());
    }
    assert_eq!(setup.runs(), 2);
    assert!(!setup.state.join("prepare-cache").exists());
}

#[tokio::test]
async fn a_folder_the_repository_holds_is_left_alone() {
    let setup = Setup::new();
    let checkout = setup.checkout("a1", "lock");
    fs::create_dir(checkout.join("node_modules")).expect("tracked folder");

    install(
        &checkout,
        "touch node_modules/y",
        Some("package-lock.json"),
        &setup.state,
        LIMIT,
    )
    .await
    .expect("install");

    assert!(checkout.join("node_modules/y").exists());
    assert!(!checkout.join("node_modules/.tack-prepared").exists());
    assert!(!setup.state.join("prepare-cache").exists());
}

#[tokio::test]
async fn an_attempt_that_loses_the_race_keeps_its_own_install_and_the_winners_entry() {
    let setup = Setup::new();
    let winner = setup.entry("lock").join("node_modules");
    let checkout = setup.checkout("a1", "lock");
    // The other attempt finishes while this one's command is still running.
    let command = format!(
        "mkdir node_modules && touch node_modules/mine && mkdir -p {0} && touch {0}/theirs",
        winner.display()
    );

    install(
        &checkout,
        &command,
        Some("package-lock.json"),
        &setup.state,
        LIMIT,
    )
    .await
    .expect("install");

    assert!(checkout.join("node_modules/mine").exists());
    assert!(winner.join("theirs").exists());
    assert!(!winner.join("mine").exists());
    assert_eq!(
        fs::read_dir(setup.state.join("prepare-cache"))
            .expect("cache")
            .count(),
        1,
        "the losing copy is dropped"
    );
}

#[tokio::test]
async fn an_install_that_cannot_be_hard_linked_still_runs_and_is_just_not_kept() {
    let setup = Setup::new();
    // A cache entry that is not a directory cannot be linked from, and is not replaced.
    let entry = setup.entry("lock");
    fs::create_dir_all(entry.parent().expect("cache")).expect("cache");
    fs::write(&entry, "in the way").expect("entry");
    let checkout = setup.checkout("a1", "lock");

    install(
        &checkout,
        &setup.install_command(),
        Some("package-lock.json"),
        &setup.state,
        LIMIT,
    )
    .await
    .expect("install");

    assert_eq!(setup.runs(), 1);
    assert!(checkout.join("node_modules/x").exists());
    assert!(checkout.join("node_modules/.tack-prepared").is_file());
}

#[tokio::test]
async fn a_python_install_is_never_kept() {
    let setup = Setup::new();
    let checkout = setup.state.join("workspaces/a1");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::write(checkout.join("uv.lock"), "lock").expect("lockfile");
    let command = format!("echo ran >> {}", setup.state.join("ran").display());

    for _ in 0..2 {
        prepare_with(&checkout, &command, "uv.lock", &setup.state).await;
    }

    assert_eq!(setup.runs(), 2);
    assert!(!setup.state.join("prepare-cache").exists());
}

async fn prepare_with(checkout: &Path, command: &str, lockfile: &str, state: &Path) {
    install(checkout, command, Some(lockfile), state, LIMIT)
        .await
        .expect("install");
}

#[tokio::test]
async fn a_failed_command_says_how_it_ended_and_shows_the_end_of_its_output() {
    let setup = Setup::new();
    let checkout = setup.checkout("a1", "lock");

    let error = install(
        &checkout,
        "echo nope >&2; exit 3",
        None,
        &setup.state,
        LIMIT,
    )
    .await
    .expect_err("fails");
    assert_eq!(
        error.to_string(),
        "Could not prepare the dependencies: `echo nope >&2; exit 3` exited 3\nnope"
    );

    let error = install(&checkout, "seq 1 100; exit 1", None, &setup.state, LIMIT)
        .await
        .expect_err("fails");
    let reason = error.to_string();
    assert!(reason.contains("exited 1\n61\n"), "{reason}");
    assert!(reason.ends_with("\n100"), "{reason}");
    assert!(!reason.contains("\n60\n"), "{reason}");

    assert!(!setup.state.join("prepare-cache").exists());
}

#[tokio::test]
async fn a_command_that_outlasts_the_time_limit_is_stopped() {
    let setup = Setup::new();
    let checkout = setup.checkout("a1", "lock");

    let error = install(
        &checkout,
        "sleep 30",
        None,
        &setup.state,
        Duration::from_secs(1),
    )
    .await
    .expect_err("times out");

    assert_eq!(
        error.to_string(),
        "Could not prepare the dependencies: `sleep 30` timed out after 1 s"
    );
}

#[test]
fn only_a_folder_with_the_marker_is_reported_as_prepared() {
    let setup = Setup::new();
    let checkout = setup.checkout("a1", "lock");
    fs::create_dir(checkout.join("node_modules")).expect("real folder");
    assert!(prepared_folders(&checkout).is_empty());

    fs::write(checkout.join("node_modules/.tack-prepared"), "").expect("marker");
    assert_eq!(prepared_folders(&checkout), ["node_modules"]);
}

#[test]
fn a_run_installs_nothing_only_when_its_tools_run_no_command_and_it_has_no_check() {
    let request = |harness: &str, tools: &[&str], checked: bool| {
        let mut request = crate::harness::test_support::spec(harness, Path::new("unused"))
            .work
            .request;
        request.permission_policy.tools = tools.iter().map(|tool| (*tool).to_owned()).collect();
        if !checked {
            request.brief = None;
        }
        request
    };
    let reads = ["Read", "grep", "Glob", "WebFetch", "WebSearch"];
    assert!(needs_no_install(&request("codex", &reads, false)));
    // A command, an unnamed list, a check, or docket's own pipeline each keep the install.
    assert!(!needs_no_install(&request(
        "codex",
        &["Read", "Bash"],
        false
    )));
    assert!(!needs_no_install(&request("codex", &["*"], false)));
    assert!(!needs_no_install(&request("codex", &[], false)));
    assert!(!needs_no_install(&request("codex", &reads, true)));
    assert!(!needs_no_install(&request("docket", &reads, false)));

    // The profile's own list for the run's agent counts when the run names none.
    let mut from_profile = request("codex", &[], false);
    from_profile.resolved_agent_profile.tool_policy =
        serde_json::json!({"tools": {"codex": ["Read", "Grep"]}});
    assert!(needs_no_install(&from_profile));
}
