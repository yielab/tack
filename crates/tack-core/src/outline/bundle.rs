//! A project file: the outline of ADR 0076 plus the sections of ADR 0081
//! (fields, profiles, roles, sprints, automation). Pure: [`check`] reads the
//! file and a description of the board and answers with the plan that can be
//! written, or with every problem at once; [`to_bundle`] writes a project back
//! as the same file.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Timelike, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::{
    BundleAutoStart, BundleAutomation, BundleAutonomy, BundleField, BundleProfile, BundleRole,
    BundleSprint, BundleStatus, BundleWorkflow, Outline, OutlineError, OutlineItem, OutlineProject,
    ResolvedOutline, label, parse_due_date, resolve, topological, trimmed,
};
use crate::autonomy::Level;
use crate::models::{
    CustomFieldDefinition, CustomFieldType, CustomFieldValue, Dependency, DependencyType,
    EstimateUnit, Item, Priority, Project, ProjectType, Role, Sprint, SprintStatus,
};
use crate::vocabulary::{self, VocabularyMap, vocabulary_for_type};
use crate::workflow::{
    StatusCategory, StatusDef, WorkflowConfig, WorkflowType, preset_workflow, workflow_for_type,
};

const PRESETS: &str = "scrum, kanban, simple, construction, legal, research, event";
const PROFILE_KINDS: [&str; 5] = ["implementer", "reviewer", "researcher", "planner", "custom"];

/// An agent profile as the board holds it.
#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub instructions: String,
    pub tool_policy: Value,
    pub limits: Value,
    pub fleet_id: Option<String>,
    pub delivers: Option<String>,
    pub builtin: bool,
}

/// What a file is checked against: the project's own settings and everything
/// the board already has. A new project passes empty slices.
pub struct Board<'a> {
    pub workflow: &'a WorkflowConfig,
    pub fields: &'a [CustomFieldDefinition],
    pub profiles: &'a [ProfileRow],
    pub roles: &'a [(Uuid, String)],
    pub sprints: &'a [(Uuid, String)],
    /// `(id, name)`
    pub fleets: &'a [(String, String)],
    pub harness_kinds: &'a [&'a str],
    /// The project's level today; L3 for a new one.
    pub level: Level,
    pub auto_start: bool,
}

/// A section's row: made new, or matched by name to one the board has.
#[derive(Debug, Clone)]
pub enum Upsert<T> {
    Create(T),
    Update { id: String, value: T },
}

#[derive(Debug)]
pub struct Plan {
    pub vocabulary: Option<VocabularyMap>,
    pub fields: Vec<Upsert<BundleField>>,
    pub profiles: Vec<Upsert<BundleProfile>>,
    pub roles: Vec<Upsert<BundleRole>>,
    pub sprints: Vec<Upsert<BundleSprint>>,
    /// Levels and auto-start already lowered to what an import may grant.
    pub automation: Option<BundleAutomation>,
    pub items: ResolvedOutline,
    pub sections: Vec<SectionSummary>,
    pub warnings: Vec<String>,
}

/// What one section of the file will do, for the preview.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SectionSummary {
    pub section: String,
    pub created: usize,
    pub updated: usize,
    /// Names the project has and the file does not carry; left alone.
    pub not_mentioned: usize,
    pub warnings: Vec<String>,
}

/// Everything `to_bundle` reads of one project.
pub struct Parts<'a> {
    pub project: &'a Project,
    pub fields: &'a [CustomFieldDefinition],
    pub profiles: &'a [ProfileRow],
    pub roles: &'a [Role],
    pub sprints: &'a [Sprint],
    pub autonomy: Option<BundleAutonomy>,
    pub auto_start: BundleAutoStart,
    pub items: &'a [Item],
    /// `(item, role)`
    pub item_roles: &'a [(Uuid, Uuid)],
    pub item_fields: &'a [CustomFieldValue],
    pub dependencies: &'a [Dependency],
    /// `(id, name)`
    pub fleets: &'a [(String, String)],
}

// ─── Workflow ────────────────────────────────────────────────

fn workflow_from(workflow: &BundleWorkflow) -> Result<WorkflowConfig, Vec<String>> {
    let mut problems = Vec::new();
    let config = match (&workflow.preset, workflow.statuses.is_empty()) {
        (Some(_), false) => {
            problems.push("project.workflow: give either a preset or statuses, not both".into());
            None
        }
        (None, true) => {
            problems.push(format!(
                "project.workflow: give a preset ({PRESETS}) or the statuses"
            ));
            None
        }
        (Some(name), true) => match preset_workflow(name) {
            Some(mut config) => {
                if let Some(transitions) = &workflow.transitions {
                    config.transitions = Some(transitions.clone());
                }
                Some(config)
            }
            None => {
                problems.push(format!(
                    "project.workflow: preset \"{}\" is not one of {PRESETS}",
                    name.trim()
                ));
                None
            }
        },
        (None, false) => {
            let mut statuses = Vec::new();
            for (i, status) in workflow.statuses.iter().enumerate() {
                let category = match status.category.trim() {
                    "todo" => Some(StatusCategory::Todo),
                    "in_progress" => Some(StatusCategory::InProgress),
                    "done" => Some(StatusCategory::Done),
                    other => {
                        problems.push(format!(
                            "project.workflow: status \"{}\" has category \"{other}\", which is \
                             not todo, in_progress or done",
                            status.name.trim()
                        ));
                        None
                    }
                };
                if let Some(category) = category {
                    statuses.push(StatusDef {
                        name: status.name.trim().to_string(),
                        category,
                        wip_limit: status.wip_limit,
                        order: i32::try_from(i).unwrap_or(i32::MAX),
                    });
                }
            }
            (problems.is_empty()).then(|| WorkflowConfig {
                workflow_type: WorkflowType::Custom,
                statuses,
                transitions: workflow.transitions.clone(),
            })
        }
    };
    match config {
        Some(config) => match config.validate() {
            Ok(()) => Ok(config),
            Err(e) => Err(vec![format!("project.workflow: {e}")]),
        },
        None => Err(problems),
    }
}

/// The workflow a new project gets: the file's preset or statuses (checked
/// with `WorkflowConfig::validate`), else the type's.
pub fn workflow_of(
    project: Option<&OutlineProject>,
    project_type: &ProjectType,
) -> Result<WorkflowConfig, OutlineError> {
    match project.and_then(|p| p.workflow.as_ref()) {
        None => Ok(workflow_for_type(project_type)),
        Some(workflow) => workflow_from(workflow).map_err(|problems| OutlineError { problems }),
    }
}

// ─── Check ───────────────────────────────────────────────────

fn name_of(text: &str) -> &str {
    text.trim()
}

/// Match one section's rows to the board's by name. A repeated or empty name
/// is a problem. Returns the rows and how many board names the file leaves out.
fn match_names<T: Clone>(
    section: &str,
    rows: &[T],
    name: impl Fn(&T) -> &str,
    existing: &[(String, String)],
    problems: &mut Vec<String>,
) -> (Vec<Upsert<T>>, usize) {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let wanted = name_of(name(row));
        if wanted.is_empty() {
            problems.push(format!("{section}: entry {} has no name", i + 1));
            continue;
        }
        if !seen.insert(wanted.to_string()) {
            problems.push(format!("{section}: \"{wanted}\" is named twice"));
            continue;
        }
        out.push(match existing.iter().find(|(_, n)| n.trim() == wanted) {
            Some((id, _)) => Upsert::Update {
                id: id.clone(),
                value: row.clone(),
            },
            None => Upsert::Create(row.clone()),
        });
    }
    let not_mentioned = existing
        .iter()
        .filter(|(_, n)| !seen.contains(n.trim()))
        .count();
    (out, not_mentioned)
}

fn counts<T>(rows: &[Upsert<T>]) -> (usize, usize) {
    let updated = rows
        .iter()
        .filter(|r| matches!(r, Upsert::Update { .. }))
        .count();
    (rows.len() - updated, updated)
}

fn parse_field_type(text: &str) -> Option<CustomFieldType> {
    serde_json::from_value(Value::String(text.trim().to_string())).ok()
}

fn field_definition(field: &BundleField, field_type: CustomFieldType) -> CustomFieldDefinition {
    CustomFieldDefinition {
        id: Uuid::nil(),
        project_id: None,
        name: field.name.trim().to_string(),
        field_type,
        description: field.description.clone(),
        required: field.required,
        default_value: field.default.clone(),
        options: (!field.options.is_empty()).then(|| field.options.clone()),
        validation: field.validation.clone(),
        created_at: DateTime::<Utc>::UNIX_EPOCH,
        updated_at: DateTime::<Utc>::UNIX_EPOCH,
    }
}

fn team_is_valid(team: &Value) -> bool {
    team.as_str().is_some_and(|t| {
        t.len() <= 64
            && t.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && t.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

fn is_date(text: &str) -> bool {
    parse_due_date(text).is_some()
}

/// A level word, lowered to the board's when it asks for more.
fn lowered_level(
    what: &str,
    asked: &str,
    board: Level,
    problems: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> String {
    match asked.trim().parse::<Level>() {
        Ok(level) if level > board => {
            warnings.push(format!(
                "{what} asked: {level}; applied: {board}. Raise it from Settings → Automation"
            ));
            board.to_string()
        }
        Ok(level) => level.to_string(),
        Err(_) => {
            problems.push(format!("{what} \"{}\" is not L3, L4 or L5", asked.trim()));
            asked.to_string()
        }
    }
}

/// Check a whole file against the board. Either it can all be written, with
/// the plan, or nothing is, with every problem found.
pub fn check(outline: &Outline, board: &Board<'_>) -> Result<Plan, OutlineError> {
    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut sections: Vec<SectionSummary> = Vec::new();

    // project: vocabulary and workflow
    let mut plan_vocabulary = None;
    if let Some(project) = &outline.project {
        if let Some(words) = &project.vocabulary {
            let mut keys: Vec<&String> = words.keys().collect();
            keys.sort();
            for key in keys {
                let one = VocabularyMap::from([(key.clone(), String::new())]);
                if vocabulary::validate(&one).is_err() {
                    problems.push(format!(
                        "project.vocabulary: \"{key}\" is not a vocabulary key"
                    ));
                }
            }
            plan_vocabulary = Some(words.clone());
        }
        if let Some(workflow) = &project.workflow
            && let Err(found) = workflow_from(workflow)
        {
            problems.extend(found);
        }
    }

    // fields
    let mut field_defs: BTreeMap<String, CustomFieldDefinition> = BTreeMap::new();
    for (i, field) in outline.fields.iter().enumerate() {
        let name = name_of(&field.name);
        let at = if name.is_empty() {
            format!("fields: entry {}", i + 1)
        } else {
            format!("fields: \"{name}\"")
        };
        let Some(field_type) = parse_field_type(&field.field_type) else {
            problems.push(format!(
                "{at} has type \"{}\", which is not one of text, number, date, boolean, select, \
                 multi_select, url, email, long_text",
                field.field_type.trim()
            ));
            continue;
        };
        if matches!(
            field_type,
            CustomFieldType::Select | CustomFieldType::MultiSelect
        ) && field.options.is_empty()
        {
            problems.push(format!(
                "{at} is a {} field and needs options",
                field.field_type.trim()
            ));
        }
        field_defs.insert(name.to_string(), field_definition(field, field_type));
    }
    let existing_fields: Vec<(String, String)> = board
        .fields
        .iter()
        .map(|f| (f.id.to_string(), f.name.clone()))
        .collect();
    let (fields, fields_left) = match_names(
        "fields",
        &outline.fields,
        |f| &f.name,
        &existing_fields,
        &mut problems,
    );
    // an existing field's rules come from the board
    for definition in board.fields {
        field_defs.insert(definition.name.trim().to_string(), definition.clone());
    }
    if !outline.fields.is_empty() {
        let (created, updated) = counts(&fields);
        sections.push(SectionSummary {
            section: "fields".into(),
            created,
            updated,
            not_mentioned: fields_left,
            warnings: Vec::new(),
        });
    }

    // profiles
    let mut profile_warnings = Vec::new();
    let builtin_names: HashSet<&str> = board
        .profiles
        .iter()
        .filter(|p| p.builtin)
        .map(|p| p.name.trim())
        .collect();
    let mut own_profiles: Vec<BundleProfile> = Vec::new();
    for profile in &outline.profiles {
        let name = name_of(&profile.name);
        if builtin_names.contains(name) {
            profile_warnings.push(format!(
                "profiles: \"{name}\" is built in and the file does not change it"
            ));
            continue;
        }
        let at = format!("profiles: \"{name}\"");
        let mut profile = profile.clone();
        if let Some(kind) = profile.kind.as_deref().map(str::trim)
            && !PROFILE_KINDS.contains(&kind)
        {
            problems.push(format!(
                "{at} has kind \"{kind}\", which is not one of {}",
                PROFILE_KINDS.join(", ")
            ));
        }
        if let Some(agent) = profile.tool_policy.get("agent")
            && !agent
                .as_str()
                .is_some_and(|a| board.harness_kinds.contains(&a))
        {
            problems.push(format!(
                "{at}: tool_policy.agent must be one of {}",
                board.harness_kinds.join(", ")
            ));
        }
        if let Some(team) = profile
            .tool_policy
            .get("docket")
            .and_then(|d| d.get("team"))
            && !team_is_valid(team)
        {
            problems.push(format!(
                "{at}: tool_policy.docket.team must match ^[a-z0-9][a-z0-9-]{{0,63}}$"
            ));
        }
        if let Err(message) = crate::run_defaults::check_network(&profile.tool_policy) {
            problems.push(format!("{at}: {message}"));
        }
        if let Err(message) = crate::run_defaults::check_limits(&profile.limits) {
            problems.push(format!("{at}: {message}"));
        }
        if let Some(Err(message)) = profile
            .delivers
            .as_deref()
            .map(crate::run_defaults::check_delivers)
        {
            problems.push(format!("{at}: {message}"));
        }
        if let Some(fleet) = profile.fleet.as_deref().map(str::trim)
            && !board.fleets.iter().any(|(_, n)| n.trim() == fleet)
        {
            profile_warnings.push(format!(
                "{at}: no fleet is named \"{fleet}\", so the profile runs on any computer"
            ));
            profile.fleet = None;
        }
        own_profiles.push(profile);
    }
    let existing_profiles: Vec<(String, String)> = board
        .profiles
        .iter()
        .filter(|p| !p.builtin)
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    let (profiles, profiles_left) = match_names(
        "profiles",
        &own_profiles,
        |p| &p.name,
        &existing_profiles,
        &mut problems,
    );
    if !outline.profiles.is_empty() {
        let (created, updated) = counts(&profiles);
        sections.push(SectionSummary {
            section: "profiles".into(),
            created,
            updated,
            not_mentioned: profiles_left,
            warnings: profile_warnings.clone(),
        });
    }
    warnings.extend(profile_warnings);

    let profile_known = |name: &str| {
        let name = name.trim();
        outline.profiles.iter().any(|p| p.name.trim() == name)
            || board.profiles.iter().any(|p| p.name.trim() == name)
    };

    // roles
    for role in &outline.roles {
        if let Some(profile) = role.profile.as_deref().map(str::trim)
            && !profile_known(profile)
        {
            problems.push(format!(
                "roles: \"{}\" names profile \"{profile}\", which is in neither the file nor the \
                 board",
                name_of(&role.name)
            ));
        }
    }
    let existing_roles: Vec<(String, String)> = board
        .roles
        .iter()
        .map(|(id, n)| (id.to_string(), n.clone()))
        .collect();
    let (roles, roles_left) = match_names(
        "roles",
        &outline.roles,
        |r| &r.name,
        &existing_roles,
        &mut problems,
    );
    if !outline.roles.is_empty() {
        let (created, updated) = counts(&roles);
        sections.push(SectionSummary {
            section: "roles".into(),
            created,
            updated,
            not_mentioned: roles_left,
            warnings: Vec::new(),
        });
    }

    // sprints
    for sprint in &outline.sprints {
        let at = format!("sprints: \"{}\"", name_of(&sprint.name));
        if let Some(status) = sprint.status.as_deref().map(str::trim)
            && !["planning", "closed"].contains(&status)
        {
            problems.push(format!(
                "{at} has status \"{status}\", which is not planning or closed"
            ));
        }
        for (key, value) in [("start", &sprint.start), ("end", &sprint.end)] {
            if let Some(text) = value.as_deref().map(str::trim)
                && !is_date(text)
            {
                problems.push(format!(
                    "{at}: {key} \"{text}\" is not a date (write YYYY-MM-DD)"
                ));
            }
        }
    }
    let existing_sprints: Vec<(String, String)> = board
        .sprints
        .iter()
        .map(|(id, n)| (id.to_string(), n.clone()))
        .collect();
    let (sprints, sprints_left) = match_names(
        "sprints",
        &outline.sprints,
        |s| &s.name,
        &existing_sprints,
        &mut problems,
    );
    if !outline.sprints.is_empty() {
        let (created, updated) = counts(&sprints);
        sections.push(SectionSummary {
            section: "sprints".into(),
            created,
            updated,
            not_mentioned: sprints_left,
            warnings: Vec::new(),
        });
    }

    // automation
    let mut automation = outline.automation.clone();
    if let Some(auto) = automation.as_mut() {
        let mut found = Vec::new();
        check_automation(auto, board, &profile_known, &mut problems, &mut found);
        sections.push(SectionSummary {
            section: "automation".into(),
            created: 0,
            updated: 1,
            not_mentioned: 0,
            warnings: found.clone(),
        });
        warnings.extend(found);
    }

    // items
    let mut role_names: HashSet<&str> = outline.roles.iter().map(|r| r.name.trim()).collect();
    role_names.extend(board.roles.iter().map(|(_, n)| n.trim()));
    let mut sprint_names: HashSet<&str> = outline.sprints.iter().map(|s| s.name.trim()).collect();
    sprint_names.extend(board.sprints.iter().map(|(_, n)| n.trim()));
    let mut item_warnings = Vec::new();
    let mut resolved = None;
    match resolve(&outline.items, board.workflow) {
        Ok(found) => {
            item_warnings.extend(found.warnings.iter().cloned());
            resolved = Some(found);
        }
        Err(e) => problems.extend(e.problems),
    }
    for (i, item) in outline.items.iter().enumerate() {
        check_item(
            item,
            &label(i, item),
            &role_names,
            &sprint_names,
            &field_defs,
            &mut problems,
            &mut item_warnings,
        );
    }
    if !outline.items.is_empty() {
        sections.push(SectionSummary {
            section: "items".into(),
            created: outline.items.len(),
            updated: 0,
            not_mentioned: 0,
            warnings: item_warnings.clone(),
        });
    }
    // resolve's warnings are already in item_warnings
    warnings.extend(item_warnings);

    if !problems.is_empty() {
        return Err(OutlineError { problems });
    }
    Ok(Plan {
        vocabulary: plan_vocabulary,
        fields,
        profiles,
        roles,
        sprints,
        automation,
        items: resolved.expect("items resolved when there are no problems"),
        sections,
        warnings,
    })
}

fn check_automation(
    auto: &mut BundleAutomation,
    board: &Board<'_>,
    profile_known: &dyn Fn(&str) -> bool,
    problems: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let origin = auto.code_origin.as_deref().map(str::trim);
    if let Some(origin) = origin
        && !["folder", "new_folder", "url", "none"].contains(&origin)
    {
        problems.push(format!(
            "automation.code_origin \"{origin}\" is not folder, new_folder, url or none"
        ));
    }
    if let Some(mode) = auto.workspace_mode.as_deref().map(str::trim)
        && !["local_branch", "in_place", "clone"].contains(&mode)
    {
        problems.push(format!(
            "automation.workspace_mode \"{mode}\" is not local_branch, in_place or clone"
        ));
    }
    if let Some(repository) = auto.repository.as_deref() {
        match origin {
            Some("folder" | "new_folder") if !repository.starts_with('/') => problems.push(
                "automation.repository must be an absolute path for a folder code origin".into(),
            ),
            Some("url")
                if !repository
                    .split_once("://")
                    .is_some_and(|(scheme, rest)| !scheme.is_empty() && !rest.is_empty()) =>
            {
                problems.push("automation.repository must be a URL for a url code origin".into());
            }
            _ => {}
        }
    }
    if let Some(prepare) = auto.prepare.as_deref() {
        match crate::prepare::clean_command(prepare) {
            Ok(command) => auto.prepare = Some(command),
            Err(_) => problems.push("automation.prepare must be a single line".into()),
        }
    }
    if let Some(harness) = auto.default_harness.as_deref().map(str::trim)
        && !board.harness_kinds.contains(&harness)
    {
        problems.push(format!(
            "automation.default_harness \"{harness}\" is not one of {}",
            board.harness_kinds.join(", ")
        ));
    }
    if let Some(profile) = auto.default_profile.as_deref().map(str::trim)
        && !profile_known(profile)
    {
        problems.push(format!(
            "automation.default_profile \"{profile}\" is in neither the file nor the board"
        ));
    }
    let in_workflow = |status: &str| {
        board
            .workflow
            .statuses
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(status.trim()))
            .map(|s| s.name.clone())
    };
    if let Some(status) = auto.on_finish_status.as_deref() {
        match in_workflow(status) {
            Some(found) => auto.on_finish_status = Some(found),
            None => problems.push(format!(
                "automation.on_finish_status \"{}\" is not a status of the workflow ({})",
                status.trim(),
                board.workflow.status_names().join(", ")
            )),
        }
    }
    if let Some(autonomy) = auto.autonomy.as_mut() {
        if let Some(level) = autonomy.level.as_deref() {
            autonomy.level = Some(lowered_level(
                "autonomy.level",
                level,
                board.level,
                problems,
                warnings,
            ));
        }
        for domain in &mut autonomy.domains {
            let what = format!("autonomy.domains[{}].level", domain.name.trim());
            domain.level = lowered_level(&what, &domain.level, board.level, problems, warnings);
        }
    }
    if let Some(start) = auto.auto_start.as_mut() {
        if let Some(status) = start.status.as_deref() {
            match in_workflow(status) {
                Some(found) => start.status = Some(found),
                None => problems.push(format!(
                    "automation.auto_start.status \"{}\" is not a status of the workflow ({})",
                    status.trim(),
                    board.workflow.status_names().join(", ")
                )),
            }
        }
        if start.max_open.is_some_and(|n| n < 1) {
            problems.push("automation.auto_start.max_open must be at least 1".into());
        }
        if start.enabled == Some(true) && !board.auto_start {
            start.enabled = Some(false);
            warnings.push(
                "auto_start asked: true; applied: false. Turn it on from Settings → Automation"
                    .into(),
            );
        }
    }
}

fn check_item(
    item: &OutlineItem,
    at: &str,
    roles: &HashSet<&str>,
    sprints: &HashSet<&str>,
    fields: &BTreeMap<String, CustomFieldDefinition>,
    problems: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    for role in item
        .roles
        .iter()
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
    {
        if !roles.contains(role) {
            problems.push(format!(
                "{at}: role \"{role}\" is in neither the file nor the project"
            ));
        }
    }
    if let Some(sprint) = trimmed(item.sprint.as_deref())
        && !sprints.contains(sprint)
    {
        problems.push(format!(
            "{at}: sprint \"{sprint}\" is in neither the file nor the project"
        ));
    }
    for (key, value) in &item.fields {
        match fields.get(key.trim()) {
            None => problems.push(format!(
                "{at}: field \"{}\" is in neither the file nor the project",
                key.trim()
            )),
            Some(definition) => {
                if let Err(message) = definition.validate_value(value) {
                    problems.push(format!("{at}: {message}"));
                }
            }
        }
    }
    for (name, definition) in fields {
        if definition.required
            && !item
                .fields
                .iter()
                .any(|(k, v)| k.trim() == name && !v.is_null())
        {
            warnings.push(format!("{at}: required field \"{name}\" has no value"));
        }
    }
}

// ─── Export ──────────────────────────────────────────────────

fn category_word(category: &StatusCategory) -> &'static str {
    match category {
        StatusCategory::Todo => "todo",
        StatusCategory::InProgress => "in_progress",
        StatusCategory::Done => "done",
    }
}

fn day_or_instant(at: DateTime<Utc>) -> String {
    if at.hour() == 0 && at.minute() == 0 && at.second() == 0 {
        at.format("%Y-%m-%d").to_string()
    } else {
        at.to_rfc3339()
    }
}

/// The export: the same file back, deterministic.
pub fn to_bundle(parts: &Parts<'_>) -> Outline {
    let project = parts.project;
    let defaults = vocabulary_for_type(&project.project_type);
    let differing: VocabularyMap = project
        .vocabulary
        .iter()
        .filter(|(k, v)| defaults.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut statuses: Vec<&StatusDef> = project.workflow.statuses.iter().collect();
    statuses.sort_by_key(|s| s.order);
    let outline_project = OutlineProject {
        name: Some(project.name.clone()),
        project_type: Some(project.project_type.to_string()),
        description: project.description.clone(),
        vocabulary: (!differing.is_empty()).then_some(differing),
        workflow: Some(BundleWorkflow {
            preset: None,
            statuses: statuses
                .into_iter()
                .map(|s| BundleStatus {
                    name: s.name.clone(),
                    category: category_word(&s.category).to_string(),
                    wip_limit: s.wip_limit,
                })
                .collect(),
            transitions: project.workflow.transitions.clone(),
        }),
    };

    let mut fields: Vec<BundleField> = parts
        .fields
        .iter()
        .map(|f| BundleField {
            name: f.name.clone(),
            field_type: serde_json::to_value(&f.field_type)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            description: f.description.clone(),
            required: f.required,
            default: f.default_value.clone(),
            options: f.options.clone().unwrap_or_default(),
            validation: f.validation.clone(),
        })
        .collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));

    let profile_by_id: HashMap<&str, &ProfileRow> =
        parts.profiles.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut referenced: Vec<&ProfileRow> = Vec::new();
    for id in parts
        .roles
        .iter()
        .filter_map(|r| r.agent_profile_id.as_deref())
        .chain(project.default_profile_id.as_deref())
    {
        if let Some(row) = profile_by_id.get(id)
            && !row.builtin
            && !referenced.iter().any(|r| r.id == row.id)
        {
            referenced.push(row);
        }
    }
    let profiles: Vec<BundleProfile> = referenced
        .into_iter()
        .map(|p| BundleProfile {
            name: p.name.clone(),
            kind: Some(p.kind.clone()),
            instructions: p.instructions.clone(),
            tool_policy: p.tool_policy.clone(),
            limits: p.limits.clone(),
            fleet: p.fleet_id.as_deref().and_then(|id| {
                parts
                    .fleets
                    .iter()
                    .find(|(fleet, _)| fleet == id)
                    .map(|(_, name)| name.clone())
            }),
            summary: None,
            delivers: p.delivers.clone(),
        })
        .collect();

    let roles: Vec<BundleRole> = parts
        .roles
        .iter()
        .map(|r| BundleRole {
            name: r.name.clone(),
            color: Some(r.color.clone()),
            icon: r.icon.clone(),
            profile: r
                .agent_profile_id
                .as_deref()
                .and_then(|id| profile_by_id.get(id))
                .map(|p| p.name.clone()),
        })
        .collect();

    let sprints: Vec<BundleSprint> = parts
        .sprints
        .iter()
        .map(|s| BundleSprint {
            name: s.name.clone(),
            goal: s.goal.clone(),
            start: s.start_date.map(|d| d.format("%Y-%m-%d").to_string()),
            end: s.end_date.map(|d| d.format("%Y-%m-%d").to_string()),
            status: (s.status == SprintStatus::Closed).then(|| "closed".to_string()),
        })
        .collect();

    let automation = BundleAutomation {
        code_origin: Some(project.code_origin.to_string()),
        repository: project.repository.clone(),
        default_branch: project.default_branch.clone(),
        workspace_mode: Some(project.workspace_mode.to_string()),
        prepare: project.prepare_command.clone(),
        push_after_run: Some(project.push_after_run),
        default_harness: project.default_harness.clone(),
        default_profile: project
            .default_profile_id
            .as_deref()
            .and_then(|id| profile_by_id.get(id))
            .map(|p| p.name.clone()),
        default_model: project.default_model.clone(),
        on_finish_status: project.on_finish_status.clone(),
        definition_of_done: project.definition_of_done.clone(),
        autonomy: parts.autonomy.clone(),
        auto_start: Some(parts.auto_start.clone()),
    };

    Outline {
        project: Some(outline_project),
        fields,
        profiles,
        roles,
        sprints,
        automation: Some(automation),
        items: items_of(parts),
    }
}

fn items_of(parts: &Parts<'_>) -> Vec<OutlineItem> {
    let index: HashMap<Uuid, usize> = parts
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.id, i))
        .collect();
    let edges: Vec<(usize, usize)> = parts
        .items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            item.parent_id
                .and_then(|p| index.get(&p))
                .map(|&parent| (parent, i))
        })
        .collect();
    let mut order = topological(parts.items.len(), &edges);
    // a parent loop cannot be written; keep the rest in the slice's order
    for i in 0..parts.items.len() {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    let reference: HashMap<Uuid, String> = order
        .iter()
        .enumerate()
        .map(|(n, &i)| (parts.items[i].id, (n + 1).to_string()))
        .collect();
    let role_name: HashMap<Uuid, &str> = parts
        .roles
        .iter()
        .map(|r| (r.id, r.name.as_str()))
        .collect();
    let field_name: HashMap<Uuid, &str> = parts
        .fields
        .iter()
        .map(|f| (f.id, f.name.as_str()))
        .collect();
    let sprint_name: HashMap<Uuid, &str> = parts
        .sprints
        .iter()
        .map(|s| (s.id, s.name.as_str()))
        .collect();

    order
        .into_iter()
        .map(|i| {
            let item = &parts.items[i];
            let mut roles: Vec<String> = parts
                .item_roles
                .iter()
                .filter(|(item_id, _)| *item_id == item.id)
                .filter_map(|(_, role)| role_name.get(role).map(|n| n.to_string()))
                .collect();
            roles.sort();
            let fields: BTreeMap<String, Value> = parts
                .item_fields
                .iter()
                .filter(|v| v.item_id == item.id)
                .filter_map(|v| {
                    field_name
                        .get(&v.field_id)
                        .map(|n| (n.to_string(), v.value.clone()))
                })
                .collect();
            let depends_on: Vec<String> = parts
                .dependencies
                .iter()
                .filter(|d| {
                    d.dependency_type == DependencyType::Blocks && d.target_item_id == item.id
                })
                .filter_map(|d| reference.get(&d.source_item_id).cloned())
                .collect();
            OutlineItem {
                reference: reference.get(&item.id).cloned(),
                title: item.title.clone(),
                description: item.description.clone(),
                item_type: Some(item.item_type.to_string()),
                status: Some(item.status.clone()),
                priority: (item.priority != Priority::Medium).then(|| item.priority.to_string()),
                assignee: item.assignee.clone(),
                estimate: item.estimate,
                due_date: item.due_date.map(day_or_instant),
                tags: item.tags.clone(),
                recurrence: item.recurrence.map(|r| r.to_string()),
                parent: item.parent_id.and_then(|p| reference.get(&p)).cloned(),
                depends_on,
                roles,
                sprint: item
                    .sprint_id
                    .and_then(|s| sprint_name.get(&s))
                    .map(|n| n.to_string()),
                fields,
                estimate_unit: match &item.estimate_unit {
                    EstimateUnit::StoryPoints => None,
                    EstimateUnit::Hours => Some("hours".to_string()),
                    EstimateUnit::Days => Some("days".to_string()),
                    EstimateUnit::Custom(word) => Some(word.clone()),
                },
                origin: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
