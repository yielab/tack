//! Environment names a request may never set.
//!
//! The list covers the names that change what a spawned program runs or
//! loads: the search path (`PATH`), the home directory its configuration and
//! credentials are read from (`HOME`), code a loader or an interpreter injects
//! before the program starts (`LD_*`, `DYLD_*`, `NODE_OPTIONS`, `PYTHON*`,
//! `PERL5OPT`, `RUBYOPT`), a shell's start-up file (`BASH_ENV`, `ENV`), and how
//! git and ssh behave (`GIT_*`, `SSH_*`). A name ending in `_BASE_URL` is
//! refused because it redirects where a program sends its requests. Proxy
//! variables (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`) are not on
//! the list, so a request can route a program's traffic through a proxy.
//!
//! Names are matched without regard to case. An operator who wants one of them
//! grants it in the runner's own `[environment]` table, which this list does
//! not apply to.

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
