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

#[cfg(test)]
mod tests {
    use super::*;

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
}
