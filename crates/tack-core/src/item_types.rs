//! A project's item types are a catalog the project owns: each has a key, a name, a colour, a
//! level (a type that holds work, or work itself) and exactly one is the default work type
//! (ADR 0036, decision 10). Pure; `tack_db::repo::item_types` stores it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::models::ProjectType;

/// Whether a type holds work or is work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
// Named apart in the API's schemas: the notifications' `Level` already has the plain name.
#[cfg_attr(feature = "openapi", schema(as = ItemTypeLevel))]
pub enum Level {
    Group,
    Work,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Group => "group",
            Level::Work => "work",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "group" => Some(Level::Group),
            "work" => Some(Level::Work),
            _ => None,
        }
    }
}

/// A colour by the design token it draws with, never a hex value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TypeColor {
    Accent,
    Accent2,
    Danger,
    Warning,
    Success,
    Neutral,
}

impl TypeColor {
    pub fn as_str(self) -> &'static str {
        match self {
            TypeColor::Accent => "accent",
            TypeColor::Accent2 => "accent2",
            TypeColor::Danger => "danger",
            TypeColor::Warning => "warning",
            TypeColor::Success => "success",
            TypeColor::Neutral => "neutral",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "accent" => Some(TypeColor::Accent),
            "accent2" => Some(TypeColor::Accent2),
            "danger" => Some(TypeColor::Danger),
            "warning" => Some(TypeColor::Warning),
            "success" => Some(TypeColor::Success),
            "neutral" => Some(TypeColor::Neutral),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemTypeDef {
    /// Lowercase `[a-z0-9_-]{1,40}`; stable, never renamed.
    pub key: String,
    /// What the project calls it ("Epic", "Building", "Course").
    pub name: String,
    pub level: Level,
    pub color: TypeColor,
    pub order: u32,
    /// Exactly one per project, and it is work: what an item made without a type becomes.
    pub default_work: bool,
}

/// The longest key and name a type may have.
pub const MAX_KEY_LEN: usize = 40;
pub const MAX_NAME_LEN: usize = 60;

/// Whether `key` is a key a type may have: lowercase letters, digits, `_` and `-`, 1 to 40 of them.
pub fn is_valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The key a word becomes when an outline or a file names a type the catalog lacks: lowercase,
/// every run of other characters one `-`. `None` when nothing is left of it.
pub fn key_of_word(word: &str) -> Option<String> {
    let mut key = String::new();
    for c in word.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() || c == '_' {
            key.push(c);
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    let key: String = key.chars().take(MAX_KEY_LEN).collect();
    let key = key.trim_matches('-');
    (!key.is_empty()).then(|| key.to_owned())
}

/// Checks one name: not empty and not longer than [`MAX_NAME_LEN`].
pub fn check_name(name: &str) -> Result<(), CoreError> {
    let len = name.trim().chars().count();
    if len == 0 || len > MAX_NAME_LEN {
        return Err(CoreError::Validation(format!(
            "a type's name must be 1-{MAX_NAME_LEN} characters"
        )));
    }
    Ok(())
}

/// The rules of a whole catalog: valid and unique keys, a name each, and exactly one default type,
/// which is work.
pub fn check_catalog(types: &[ItemTypeDef]) -> Result<(), CoreError> {
    let mut seen = HashSet::new();
    for def in types {
        if !is_valid_key(&def.key) {
            return Err(CoreError::Validation(format!(
                "'{}' is not a type key: use 1-{MAX_KEY_LEN} lowercase letters, digits, '_' or '-'",
                def.key
            )));
        }
        if !seen.insert(def.key.as_str()) {
            return Err(CoreError::Validation(format!(
                "the type key '{}' is used twice",
                def.key
            )));
        }
        check_name(&def.name)?;
    }
    let defaults: Vec<&ItemTypeDef> = types.iter().filter(|d| d.default_work).collect();
    match defaults.as_slice() {
        [one] if one.level == Level::Work => Ok(()),
        [_] => Err(CoreError::Validation(
            "the default type must be work, not a type that holds work".into(),
        )),
        [] => Err(CoreError::Validation(
            "a project needs one default work type".into(),
        )),
        _ => Err(CoreError::Validation(
            "a project has exactly one default work type".into(),
        )),
    }
}

/// The key of the default work type of a catalog that passed [`check_catalog`].
pub fn default_work_key(types: &[ItemTypeDef]) -> Option<&str> {
    types
        .iter()
        .find(|d| d.default_work)
        .map(|d| d.key.as_str())
}

fn def(
    key: &str,
    name: &str,
    level: Level,
    color: TypeColor,
    order: u32,
    default_work: bool,
) -> ItemTypeDef {
    ItemTypeDef {
        key: key.to_owned(),
        name: name.to_owned(),
        level,
        color,
        order,
        default_work,
    }
}

/// The catalog a new project of `project_type` starts with. Migration 107 holds a frozen copy of
/// this list; a test compares the two, so changing it is a decision and not drift.
pub fn preset(project_type: &ProjectType) -> Vec<ItemTypeDef> {
    use Level::{Group, Work};
    use TypeColor::{Accent, Accent2, Danger, Neutral, Success, Warning};
    // (key, name, level, colour, default) in display order.
    let rows: [(&str, &str, Level, TypeColor, bool); 6] = match project_type {
        ProjectType::Construction => [
            ("epic", "Building", Group, Accent, false),
            ("feature", "Section", Group, Accent2, false),
            ("task", "Work Order", Work, Neutral, true),
            ("activity", "Activity", Work, Success, false),
            ("bug", "Defect", Work, Danger, false),
            ("requirement", "Specification", Work, Warning, false),
        ],
        ProjectType::Homework => [
            ("epic", "Course", Group, Accent, false),
            ("feature", "Module", Group, Accent2, false),
            ("task", "Assignment", Work, Neutral, true),
            ("question", "Question", Work, Success, false),
            ("bug", "Correction", Work, Danger, false),
            ("requirement", "Rubric Item", Work, Warning, false),
        ],
        ProjectType::Legal => return domain(("Matter", "Case", "Filing", "Issue", "Statute")),
        ProjectType::Research => {
            return domain(("Study", "Experiment", "Protocol", "Anomaly", "Hypothesis"));
        }
        ProjectType::Event => {
            return domain(("Event", "Track", "Task", "Issue", "Requirement"));
        }
        ProjectType::Software
        | ProjectType::Web
        | ProjectType::Mobile
        | ProjectType::Maintenance
        | ProjectType::Personal
        | ProjectType::Custom => return domain(("Epic", "Feature", "Task", "Bug", "Requirement")),
    };
    rows.iter()
        .enumerate()
        .map(|(i, (key, name, level, color, default))| {
            def(key, name, *level, *color, i as u32, *default)
        })
        .collect()
}

/// One group type (the epic word) and four work types, the task word the default.
fn domain(names: (&str, &str, &str, &str, &str)) -> Vec<ItemTypeDef> {
    let (epic, feature, task, bug, requirement) = names;
    vec![
        def("epic", epic, Level::Group, TypeColor::Accent, 0, false),
        def(
            "feature",
            feature,
            Level::Work,
            TypeColor::Success,
            1,
            false,
        ),
        def("task", task, Level::Work, TypeColor::Neutral, 2, true),
        def("bug", bug, Level::Work, TypeColor::Danger, 3, false),
        def(
            "requirement",
            requirement,
            Level::Work,
            TypeColor::Warning,
            4,
            false,
        ),
    ]
}

/// A type with how many items use it, as the types route lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemTypeUse {
    #[serde(flatten)]
    pub def: ItemTypeDef,
    pub items: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateItemType {
    pub key: String,
    pub name: String,
    pub level: Level,
    pub color: TypeColor,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateItemType {
    pub name: Option<String>,
    pub color: Option<TypeColor>,
    /// The position among the types, from 0.
    pub order: Option<u32>,
    /// `true` makes this the default work type; the previous one stops being it.
    pub default_work: Option<bool>,
    /// Changes only while no item uses the type.
    pub level: Option<Level>,
}

fn renumber(types: &mut [ItemTypeDef]) {
    types.sort_by(|a, b| (a.order, &a.key).cmp(&(b.order, &b.key)));
    for (i, def) in types.iter_mut().enumerate() {
        def.order = i as u32;
    }
}

/// The catalog with one more type, last in order and not the default.
pub fn add(types: &[ItemTypeDef], new: &CreateItemType) -> Result<Vec<ItemTypeDef>, CoreError> {
    if types.iter().any(|t| t.key == new.key) {
        return Err(CoreError::Validation(format!(
            "the type key '{}' is already used",
            new.key
        )));
    }
    let mut next = types.to_vec();
    next.push(ItemTypeDef {
        key: new.key.clone(),
        name: new.name.trim().to_owned(),
        level: new.level,
        color: new.color,
        order: u32::MAX,
        default_work: false,
    });
    renumber(&mut next);
    check_catalog(&next)?;
    Ok(next)
}

/// The catalog after a change to the type `key`, of which `used` items use. Its level changes only
/// while no item uses it.
pub fn update(
    types: &[ItemTypeDef],
    key: &str,
    patch: &UpdateItemType,
    used: u32,
) -> Result<Vec<ItemTypeDef>, CoreError> {
    let mut next = types.to_vec();
    let Some(at) = next.iter().position(|t| t.key == key) else {
        return Err(CoreError::Validation(format!(
            "no type has the key '{key}'"
        )));
    };
    if let Some(name) = &patch.name {
        check_name(name)?;
        next[at].name = name.trim().to_owned();
    }
    if let Some(color) = patch.color {
        next[at].color = color;
    }
    if let Some(level) = patch.level
        && level != next[at].level
    {
        if used > 0 {
            return Err(CoreError::TypeInUse {
                key: key.to_owned(),
                items: used,
            });
        }
        next[at].level = level;
    }
    match patch.default_work {
        Some(true) => next.iter_mut().for_each(|t| t.default_work = t.key == key),
        Some(false) => next[at].default_work = false,
        None => {}
    }
    if let Some(order) = patch.order {
        // Between two types, the moved one goes before the one it lands on.
        next[at].order = if (order as usize) < at {
            order.saturating_mul(2).saturating_sub(1)
        } else {
            order.saturating_mul(2).saturating_add(1)
        };
        for (i, def) in next.iter_mut().enumerate() {
            if i != at {
                def.order = (def.order).saturating_mul(2);
            }
        }
    }
    renumber(&mut next);
    check_catalog(&next)?;
    Ok(next)
}

/// The catalog without the type `key`, of which `used` items use. Items move to `merge_into`,
/// which must be another type of the same level; the default work type is not removed.
pub fn remove(
    types: &[ItemTypeDef],
    key: &str,
    merge_into: Option<&str>,
    used: u32,
) -> Result<Vec<ItemTypeDef>, CoreError> {
    let Some(gone) = types.iter().find(|t| t.key == key) else {
        return Err(CoreError::Validation(format!(
            "no type has the key '{key}'"
        )));
    };
    if gone.default_work {
        return Err(CoreError::Validation(
            "the default work type cannot be deleted: make another type the default first".into(),
        ));
    }
    match merge_into {
        None if used > 0 => {
            return Err(CoreError::TypeInUse {
                key: key.to_owned(),
                items: used,
            });
        }
        None => {}
        Some(target) => {
            let Some(into) = types.iter().find(|t| t.key == target) else {
                return Err(CoreError::Validation(format!(
                    "merge_into: no type has the key '{target}'"
                )));
            };
            if target == key {
                return Err(CoreError::Validation(
                    "merge_into must be another type".into(),
                ));
            }
            if into.level != gone.level {
                return Err(CoreError::Validation(format!(
                    "'{target}' is not at the same level as '{key}'; items can only move to a type of their own level"
                )));
            }
        }
    }
    let mut next: Vec<ItemTypeDef> = types.iter().filter(|t| t.key != key).cloned().collect();
    renumber(&mut next);
    check_catalog(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests;
