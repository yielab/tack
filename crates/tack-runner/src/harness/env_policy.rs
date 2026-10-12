//! Environment names a request may never set.
//!
//! These change what a spawned program loads, where it looks for its
//! configuration, or where its traffic goes, so a request that sets one is
//! steering the runner's machine rather than describing its own work. An
//! operator who wants one of them grants it in the runner's own
//! `[environment]` table, which is not subject to this list.

const EXACT: &[(&str, &str)] = &[
    (
        "HOME",
        "it moves where the program reads its configuration and credentials",
    ),
    ("PATH", "it changes which programs the run executes"),
    (
        "NODE_OPTIONS",
        "it makes node load code before the program starts",
    ),
    (
        "PERL5OPT",
        "it makes perl load code before the program starts",
    ),
    (
        "RUBYOPT",
        "it makes ruby load code before the program starts",
    ),
    (
        "BASH_ENV",
        "it makes a shell source a file before the program starts",
    ),
    (
        "ENV",
        "it makes a shell source a file before the program starts",
    ),
];

const PREFIXES: &[(&str, &str)] = &[
    (
        "GIT_",
        "it changes how git reads configuration and runs commands",
    ),
    ("SSH_", "it changes which keys and agents ssh uses"),
    (
        "LD_",
        "it changes which libraries the dynamic loader injects",
    ),
    (
        "DYLD_",
        "it changes which libraries the dynamic loader injects",
    ),
    (
        "PYTHON",
        "it makes python load code before the program starts",
    ),
];

const SUFFIXES: &[(&str, &str)] = &[(
    "_BASE_URL",
    "it redirects where the program sends its requests",
)];

/// Why `name` may not be set from a request, or `None` when it may.
pub fn refuse(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_uppercase();
    let name = name.as_str();
    EXACT
        .iter()
        .find(|(exact, _)| *exact == name)
        .or_else(|| PREFIXES.iter().find(|(prefix, _)| name.starts_with(prefix)))
        .or_else(|| SUFFIXES.iter().find(|(suffix, _)| name.ends_with(suffix)))
        .map(|(_, reason)| *reason)
}
