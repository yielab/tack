//! The files that decide how a piece of work is checked.
//!
//! An agent that changes one of these changes the question the verifier
//! answers, so the change is recorded on its own (`checks_changed` in the
//! evidence) and holds the automatic merge. The list is one for the four
//! agents: it lives here so the runner that records it and the board that
//! reads it cannot differ.

/// The heading a pack gives these changes.
pub const SECTION: &str = "The agent changed how it is checked";

/// File names that decide how the work is checked, wherever they sit.
const NAMES: &[&str] = &[
    "package.json",
    "cargo.toml",
    "makefile",
    "justfile",
    "prill.yaml",
    "pyproject.toml",
    "tox.ini",
    "setup.cfg",
    "pytest.ini",
    "conftest.py",
    "go.mod",
];

/// Directories whose whole content decides how the work is checked.
const DIRECTORIES: &[&str] = &[".github", ".githooks"];

/// Directories that hold the tests themselves.
const TEST_DIRECTORIES: &[&str] = &["test", "tests", "__tests__", "spec", "specs", "e2e"];

/// The config files of the JavaScript test runners: `<runner>.config.<ext>`.
const RUNNERS: &[&str] = &["jest", "vitest", "playwright"];

/// Whether `path` (workspace-relative, `/`-separated) is a file that decides
/// how the work is checked. Names are matched exactly, never by a substring,
/// so `latest/notes.md` and `contest.md` are not tests: everything under
/// `.github/` or `.githooks/`; everything under a directory named `test`,
/// `tests`, `__tests__`, `spec`, `specs` or `e2e`; files named
/// `test_*.py`, `*_test.py`, `*_test.go`, `*.test.<ext>` or `*.spec.<ext>`;
/// the `jest`, `vitest` and `playwright` config files; and the manifests and
/// build files in [`NAMES`] at any depth (`package.json` holds the scripts).
pub fn matches(path: &str) -> bool {
    let lower = path.trim_start_matches("./").to_ascii_lowercase();
    let mut segments: Vec<&str> = lower.split('/').collect();
    let Some(name) = segments.pop() else {
        return false;
    };
    if segments
        .iter()
        .any(|segment| DIRECTORIES.contains(segment) || TEST_DIRECTORIES.contains(segment))
    {
        return true;
    }
    if NAMES.contains(&name) || is_test_file(name) {
        return true;
    }
    RUNNERS.iter().any(|runner| {
        name.strip_prefix(runner)
            .is_some_and(|rest| rest.starts_with(".config."))
    })
}

fn is_test_file(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    stem.ends_with(".test")
        || stem.ends_with(".spec")
        || stem.ends_with("_test")
        || (extension == "py" && stem.starts_with("test_"))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn exact_names_decide_not_substrings() {
        let cases = [
            // What decides how the work is checked.
            ("tests/x.rs", true),
            ("./tests/x.rs", true),
            ("crates/a/tests/it.rs", true),
            ("src/__tests__/x.ts", true),
            ("test/x.go", true),
            ("spec/models/user_spec.rb", true),
            ("e2e/login.ts", true),
            ("src/widget.test.tsx", true),
            ("src/widget.spec.ts", true),
            ("pkg/test_widget.py", true),
            ("pkg/widget_test.py", true),
            ("pkg/widget_test.go", true),
            ("conftest.py", true),
            ("pytest.ini", true),
            ("package.json", true),
            ("web/package.json", true),
            ("Cargo.toml", true),
            ("crates/a/Cargo.toml", true),
            ("Makefile", true),
            ("justfile", true),
            ("prill.yaml", true),
            ("pyproject.toml", true),
            ("tox.ini", true),
            ("setup.cfg", true),
            ("go.mod", true),
            ("jest.config.js", true),
            ("web/vitest.config.ts", true),
            ("playwright.config.ts", true),
            (".github/workflows/ci.yml", true),
            (".githooks/pre-push", true),
            ("TESTS/X.RS", true),
            // What only has "test" or a name in its text.
            ("latest/x.md", false),
            ("contest.md", false),
            ("docs/attestation.md", false),
            ("src/protest.rs", false),
            ("src/main.rs", false),
            ("src/greatest_hits.py", false),
            ("src/cargo.toml.bak", false),
            ("docs/package.json.md", false),
            ("jestconfig.js", false),
            ("src/github/client.rs", false),
            ("", false),
        ];
        for (path, expected) in cases {
            assert_eq!(matches(path), expected, "{path}");
        }
    }
}
