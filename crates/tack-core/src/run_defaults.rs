//! Which agent profile (and so which fleet) a run gets when the caller names none.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub profile_id: String,
    pub fleet_id: Option<String>,
    /// The agent the profile names (`tool_policy.agent`).
    pub harness: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Explicit,
    RunSettings,
    Role(String),
    ProjectDefault,
    Implementer,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub profile_id: String,
    pub fleet_id: Option<String>,
    /// The agent the resolved profile names (`tool_policy.agent`).
    pub harness: Option<String>,
    pub source: Source,
}

pub fn resolve(
    explicit: Option<&str>,
    run_settings: Option<&str>,
    roles: &[(String, Option<String>)],
    project_default: Option<&str>,
    implementer: Option<&str>,
    profiles: &[Candidate],
) -> Option<Resolved> {
    let found = |profile_id: &str, source: Source| {
        let candidate = profiles.iter().find(|c| c.profile_id == profile_id);
        Some(Resolved {
            profile_id: profile_id.to_owned(),
            fleet_id: candidate.and_then(|c| c.fleet_id.clone()),
            harness: candidate.and_then(|c| c.harness.clone()),
            source,
        })
    };
    if let Some(id) = explicit {
        return found(id, Source::Explicit);
    }
    if let Some(id) = run_settings {
        return found(id, Source::RunSettings);
    }
    let mut named = roles
        .iter()
        .filter_map(|(role, profile)| profile.as_deref().map(|p| (role.as_str(), p)));
    if let Some((first_role, first)) = named.next() {
        if named.all(|(_, other)| other == first) {
            return found(first, Source::Role(first_role.to_owned()));
        }
        if let Some(id) = project_default {
            return found(id, Source::Ambiguous);
        }
    }
    if let Some(id) = project_default {
        return found(id, Source::ProjectDefault);
    }
    implementer.and_then(|id| found(id, Source::Implementer))
}

/// A token count as every sentence prints it: digits in groups of three,
/// separated by a space ("1 600 000").
pub fn token_count(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let grouped = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ");
    if n < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// How many successful runs a profile needs before a limit is proposed.
pub const RUNS_BEFORE_PROPOSAL: usize = 5;

/// The value at percentile `pct` (1..=100) of `sorted` (ascending), by nearest rank.
/// `None` for an empty list.
pub fn percentile(sorted: &[i64], pct: usize) -> Option<i64> {
    let rank = (sorted.len() * pct).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// The limit proposed for a profile whose runs' 90th percentile is `p90`: that × 1.5, rounded up
/// to two significant figures (523 000 → 790 000, 1 234 → 1 900).
pub fn proposed_limit(p90: i64) -> i64 {
    let raised = (p90.max(0).cast_unsigned() * 3).div_ceil(2);
    let mut unit = 1;
    while raised / unit >= 100 {
        unit *= 10;
    }
    i64::try_from(raised.div_ceil(unit) * unit).unwrap_or(i64::MAX)
}

/// What a profile's `limits` may hold: `budgets.tokens` and `timeout_seconds` are positive whole
/// numbers or absent, and `budgets.cost_usd` was removed. The message names the field in full.
pub fn check_limits(limits: &serde_json::Value) -> Result<(), String> {
    let whole = |value: &serde_json::Value| value.as_u64().filter(|n| *n > 0).is_some();
    let Some(limits) = limits.as_object() else {
        return if limits.is_null() {
            Ok(())
        } else {
            Err("limits must be an object".to_owned())
        };
    };
    if let Some(budgets) = limits.get("budgets") {
        let Some(budgets) = budgets.as_object() else {
            return Err("limits.budgets must be an object".to_owned());
        };
        if budgets.contains_key("cost_usd") {
            return Err(
                "limits.budgets.cost_usd was removed; a limit is in tokens (limits.budgets.tokens)"
                    .to_owned(),
            );
        }
        if budgets.get("tokens").is_some_and(|v| !whole(v)) {
            return Err("limits.budgets.tokens must be a positive whole number".to_owned());
        }
    }
    if limits.get("timeout_seconds").is_some_and(|v| !whole(v)) {
        return Err("limits.timeout_seconds must be a positive whole number".to_owned());
    }
    Ok(())
}

/// Whether a profile's role needs the network (`tool_policy.network`): true only when it is
/// the boolean `true`; absent, `false` or anything else is off, as every profile was before.
pub fn profile_network(tool_policy: &serde_json::Value) -> bool {
    tool_policy.get("network").and_then(|v| v.as_bool()) == Some(true)
}

/// What a profile's `tool_policy.network` may hold: a boolean or absent.
pub fn check_network(tool_policy: &serde_json::Value) -> Result<(), String> {
    match tool_policy.get("network") {
        None | Some(serde_json::Value::Bool(_)) => Ok(()),
        Some(_) => Err("tool_policy.network must be true or false".to_owned()),
    }
}

/// What a profile's `delivers` line may be: one line of at most 160 characters. An empty line is
/// no line; the caller stores it as absent.
pub fn check_delivers(delivers: &str) -> Result<(), String> {
    if delivers.contains(['\n', '\r']) || delivers.chars().count() > 160 {
        return Err("delivers must be one line of at most 160 characters".to_owned());
    }
    Ok(())
}

/// What a profile lets its agent do, read from its tools and never typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProfileCapabilities {
    pub changes_files: bool,
    pub runs_commands: bool,
    pub uses_network: bool,
}

/// The capabilities of a profile's `tool_policy`. The tools are `tools[agent]`, with `agent`
/// absent read as `claude-code`, the one agent whose tool names Tack knows; `*` is all three.
/// Edit, Write, MultiEdit and NotebookEdit change files, Bash runs commands, WebFetch and
/// WebSearch use the network (names match in any case, as the runner matches them). A docket
/// profile is all three: its pipeline decides and the profile does not record it. The profile's
/// `network` flag also counts as the network.
pub fn profile_capabilities(tool_policy: &serde_json::Value) -> ProfileCapabilities {
    let agent = tool_policy
        .get("agent")
        .and_then(|a| a.as_str())
        .unwrap_or("claude-code");
    let every = ProfileCapabilities {
        changes_files: true,
        runs_commands: true,
        uses_network: true,
    };
    if agent == "docket" {
        return every;
    }
    let names: Vec<&str> = tool_policy["tools"][agent]
        .as_array()
        .map(|tools| tools.iter().filter_map(|t| t.as_str()).collect())
        .unwrap_or_default();
    if names.contains(&"*") {
        return every;
    }
    let has = |wanted: &[&str]| {
        names
            .iter()
            .any(|name| wanted.iter().any(|w| name.eq_ignore_ascii_case(w)))
    };
    ProfileCapabilities {
        changes_files: has(&["Edit", "Write", "MultiEdit", "NotebookEdit"]),
        runs_commands: has(&["Bash"]),
        uses_network: has(&["WebFetch", "WebSearch"]) || profile_network(tool_policy),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_read_from_the_tools_of_the_agent() {
        let caps = |policy: serde_json::Value| profile_capabilities(&policy);
        let none = ProfileCapabilities::default();
        assert_eq!(caps(serde_json::json!({})), none);
        assert_eq!(
            caps(serde_json::json!({"tools": {"claude-code": ["Read", "Grep", "WebFetch"]}})),
            ProfileCapabilities {
                uses_network: true,
                ..none
            }
        );
        assert_eq!(
            caps(serde_json::json!({"tools": {"claude-code": ["Read", "edit", "Bash"]}})),
            ProfileCapabilities {
                changes_files: true,
                runs_commands: true,
                ..none
            }
        );
        // The list of the profile's own agent is the one that counts, and `*` is everything.
        let codex = serde_json::json!({"agent": "codex", "tools": {"claude-code": ["Bash"], "codex": ["*"]}});
        assert!(caps(codex).uses_network);
        let docket = serde_json::json!({"agent": "docket", "tools": {"docket": []}});
        assert!(caps(docket).changes_files);
        // The role's network flag is the network too.
        assert!(caps(serde_json::json!({"network": true})).uses_network);
        assert!(check_delivers("a change on a branch").is_ok());
        assert!(check_delivers("two\nlines").is_err());
    }

    fn cands() -> Vec<Candidate> {
        ["pe", "ps", "pc", "pd", "pi", "pf"]
            .iter()
            .map(|p| Candidate {
                profile_id: (*p).into(),
                fleet_id: (*p == "pf").then(|| "fleet-1".to_owned()),
                harness: match *p {
                    "pc" => Some("claude-code".to_owned()),
                    "pf" => Some("codex".to_owned()),
                    _ => None,
                },
            })
            .collect()
    }

    fn role(name: &str, profile: Option<&str>) -> (String, Option<String>) {
        (name.into(), profile.map(Into::into))
    }

    #[test]
    fn each_source_wins_in_order_and_two_different_role_profiles_are_ambiguous() {
        let c = cands();
        let one = [role("Content", Some("pc")), role("QA", None)];
        let two = [role("Content", Some("pc")), role("QA", Some("pf"))];
        let unnamed = [role("QA", None)];
        let same = [role("A", Some("pc")), role("B", Some("pc"))];
        // (explicit, run_settings, roles, project_default, implementer) -> (profile, fleet, harness, source)
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Option<&str>,
            Option<&str>,
            &[(String, Option<String>)],
            Option<&str>,
            Option<&str>,
            Option<(&str, Option<&str>, Option<&str>, Source)>,
        )> = vec![
            (
                Some("pe"),
                Some("ps"),
                &one,
                Some("pd"),
                Some("pi"),
                Some(("pe", None, None, Source::Explicit)),
            ),
            (
                None,
                Some("ps"),
                &one,
                Some("pd"),
                Some("pi"),
                Some(("ps", None, None, Source::RunSettings)),
            ),
            (
                None,
                None,
                &one,
                Some("pd"),
                Some("pi"),
                Some((
                    "pc",
                    None,
                    Some("claude-code"),
                    Source::Role("Content".into()),
                )),
            ),
            (
                None,
                None,
                &same,
                Some("pd"),
                Some("pi"),
                Some(("pc", None, Some("claude-code"), Source::Role("A".into()))),
            ),
            (
                None,
                None,
                &two,
                Some("pd"),
                Some("pi"),
                Some(("pd", None, None, Source::Ambiguous)),
            ),
            (
                None,
                None,
                &[],
                Some("pd"),
                Some("pi"),
                Some(("pd", None, None, Source::ProjectDefault)),
            ),
            (
                None,
                None,
                &unnamed,
                None,
                Some("pi"),
                Some(("pi", None, None, Source::Implementer)),
            ),
            (
                Some("pf"),
                None,
                &[],
                None,
                None,
                Some(("pf", Some("fleet-1"), Some("codex"), Source::Explicit)),
            ),
            (None, None, &[], None, None, None),
        ];
        for (i, (e, s, r, d, im, want)) in rows.into_iter().enumerate() {
            let got = resolve(e, s, r, d, im, &c);
            let want = want.map(|(p, f, h, source)| Resolved {
                profile_id: p.into(),
                fleet_id: f.map(Into::into),
                harness: h.map(Into::into),
                source,
            });
            assert_eq!(got, want, "row {i}");
        }
    }

    #[test]
    fn a_profiles_limits_hold_positive_whole_numbers_and_no_dollars() {
        use serde_json::json;
        // (limits, the message it is refused with, None when it is fine)
        let rows = [
            (json!({}), None),
            (json!(null), None),
            (
                json!({"budgets": {"tokens": 300_000}, "timeout_seconds": 1800}),
                None,
            ),
            (json!({"budgets": {}}), None),
            (json!({"tokens": 1000}), None),
            (
                json!({"budgets": {"tokens": 0}}),
                Some("limits.budgets.tokens must be a positive whole number"),
            ),
            (
                json!({"budgets": {"tokens": -5}}),
                Some("limits.budgets.tokens must be a positive whole number"),
            ),
            (
                json!({"budgets": {"tokens": "many"}}),
                Some("limits.budgets.tokens must be a positive whole number"),
            ),
            (
                json!({"budgets": {"tokens": 1.5}}),
                Some("limits.budgets.tokens must be a positive whole number"),
            ),
            (
                json!({"budgets": {"tokens": null}}),
                Some("limits.budgets.tokens must be a positive whole number"),
            ),
            (
                json!({"timeout_seconds": 0}),
                Some("limits.timeout_seconds must be a positive whole number"),
            ),
            (
                json!({"timeout_seconds": "1h"}),
                Some("limits.timeout_seconds must be a positive whole number"),
            ),
            (
                json!({"budgets": {"cost_usd": 2.0}}),
                Some(
                    "limits.budgets.cost_usd was removed; a limit is in tokens (limits.budgets.tokens)",
                ),
            ),
            (
                json!({"budgets": 5}),
                Some("limits.budgets must be an object"),
            ),
            (json!(["tokens"]), Some("limits must be an object")),
        ];
        for (limits, refused) in rows {
            assert_eq!(check_limits(&limits).err().as_deref(), refused, "{limits}");
        }
    }

    #[test]
    fn a_profile_needs_the_network_only_when_it_says_true() {
        use serde_json::json;
        for (policy, network) in [
            (json!({}), false),
            (json!({"network": false}), false),
            (json!({"network": true}), true),
            (json!({"agent": "docket"}), false),
        ] {
            assert_eq!(profile_network(&policy), network, "{policy}");
            assert_eq!(check_network(&policy), Ok(()), "{policy}");
        }
        for bad in [
            json!({"network": "yes"}),
            json!({"network": 1}),
            json!({"network": null}),
        ] {
            assert_eq!(
                check_network(&bad).err().as_deref(),
                Some("tool_policy.network must be true or false"),
                "{bad}"
            );
            assert!(!profile_network(&bad));
        }
    }

    #[test]
    fn the_proposal_is_the_90th_percentile_times_one_and_a_half_rounded_up_to_two_figures() {
        for (p90, proposed) in [
            (523_000, 790_000),
            (1_234, 1_900),
            (400_000, 600_000),
            (60, 90),
            (1, 2),
            (0, 0),
        ] {
            assert_eq!(proposed_limit(p90), proposed, "p90 {p90}");
        }
    }

    #[test]
    fn a_percentile_is_the_value_at_its_nearest_rank() {
        let runs: Vec<i64> = (1..=10).map(|n| n * 100).collect();
        assert_eq!(percentile(&runs, 50), Some(500));
        assert_eq!(percentile(&runs, 90), Some(900));
        assert_eq!(percentile(&runs[..5], 90), Some(500));
        assert_eq!(percentile(&[], 90), None);
    }
}
