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
    assert!(is_link(&first.join("node_modules")), "the folder is linked");
    assert!(first.join("node_modules/x").exists());
    let entry = setup.entry("lock one");
    assert!(entry.join("node_modules/x").exists(), "the cache keeps it");
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
    assert!(is_link(&second.join("node_modules")));
    assert!(second.join("node_modules/x").exists());

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

    // A checkout's removal takes the link and leaves what it linked to.
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

    assert!(is_link(&checkout.join("node_modules")));
    assert!(checkout.join("node_modules/x").exists());
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
        assert!(!is_link(&checkout.join("node_modules")));
        assert!(checkout.join("node_modules/x").exists());
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

    assert!(!is_link(&checkout.join("node_modules")));
    assert!(checkout.join("node_modules/y").exists());
    assert!(!setup.state.join("prepare-cache").exists());
}

#[tokio::test]
async fn an_attempt_that_loses_the_race_uses_the_winners_install() {
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

    assert!(is_link(&checkout.join("node_modules")));
    assert!(checkout.join("node_modules/theirs").exists());
    assert!(!checkout.join("node_modules/mine").exists());
    assert_eq!(
        fs::read_dir(setup.state.join("prepare-cache"))
            .expect("cache")
            .count(),
        1,
        "the losing copy is dropped"
    );
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
fn only_a_linked_folder_is_reported_as_linked() {
    let setup = Setup::new();
    let checkout = setup.checkout("a1", "lock");
    fs::create_dir(checkout.join("node_modules")).expect("real folder");
    let target = setup.state.join("target");
    fs::create_dir(&target).expect("target");
    std::os::unix::fs::symlink(&target, checkout.join(".venv")).expect("link");

    assert_eq!(linked_folders(&checkout), [".venv"]);
}
