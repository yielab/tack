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
}
