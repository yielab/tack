//! The project folder as files (ADR 0025 decisions 9 to 11): `project.yaml` holds the plan's
//! configuration and `items/<key>.md` one item each, its plan fields as YAML and its description
//! as Markdown. Paths are relative to `.tack/`; nothing here touches a disk.

use std::collections::BTreeMap;

use crate::outline::key::{is_key, key_of};
use crate::outline::{Outline, OutlineItem};

const PROJECT_FILE: &str = "project.yaml";
const ITEMS_DIR: &str = "items/";
const ITEM_EXTENSION: &str = ".md";
const FENCE: &str = "---";

/// The outline the files describe, or every problem in them, each naming its file. Items come
/// back in key order, each with the key of its file name.
pub fn read(files: &BTreeMap<String, String>) -> Result<Outline, Vec<String>> {
    let mut problems = Vec::new();
    let mut outline = Outline::default();
    let mut items = Vec::new();
    for (path, text) in files {
        if path == PROJECT_FILE {
            match serde_saphyr::from_str::<Outline>(text) {
                Ok(read) => {
                    outline = Outline {
                        items: Vec::new(),
                        ..read
                    }
                }
                Err(error) => problems.push(format!("{path}: {error}")),
            }
            continue;
        }
        let key = path
            .strip_prefix(ITEMS_DIR)
            .and_then(|name| name.strip_suffix(ITEM_EXTENSION));
        match key {
            Some(key) if is_key(key) => match read_item(key, text) {
                Ok(item) => items.push(item),
                Err(problem) => problems.push(format!("{path}: {problem}")),
            },
            Some(key) => problems.push(format!(
                "{path}: \"{key}\" is not a key (lowercase letters, digits and `-`, at most 64)"
            )),
            None => problems.push(format!("{path}: not a file of the project folder")),
        }
    }
    // Without its configuration the folder would read as a project with no roles, fields or
    // sprints, and a plan against the board would propose removing every one of them.
    if !files.contains_key(PROJECT_FILE) {
        problems.push(format!("{PROJECT_FILE}: the project folder has none"));
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    items.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Outline { items, ..outline })
}

fn read_item(key: &str, text: &str) -> Result<OutlineItem, String> {
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some(FENCE) {
        return Err(format!("it must start with a `{FENCE}` line"));
    }
    let mut front = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == FENCE {
            closed = true;
            break;
        }
        front.push_str(line);
    }
    if !closed {
        return Err(format!("it has no closing `{FENCE}` line"));
    }
    let body: String = lines.collect();
    let mut item: OutlineItem =
        serde_saphyr::from_str(&front).map_err(|error| error.to_string())?;
    if item.key.is_some() {
        return Err("`key` is the file's name; remove the `key` line".into());
    }
    if item.description.is_some() {
        return Err("the description goes below the closing `---` line, not in it".into());
    }
    item.key = Some(key.to_owned());
    item.description = (!body.is_empty()).then_some(body);
    Ok(item)
}

/// The files of a project folder: `project.yaml` without items and one file per item, in key
/// order, with the item's plan fields and none of the board's.
pub fn write(outline: &Outline) -> BTreeMap<String, String> {
    let project = Outline {
        items: Vec::new(),
        ..outline.clone()
    };
    let mut files = BTreeMap::new();
    files.insert(PROJECT_FILE.to_owned(), yaml(&project));
    for item in &outline.items {
        let key = item.key.clone().unwrap_or_else(|| key_of(&item.title));
        let plan = OutlineItem {
            key: None,
            reference: None,
            description: None,
            status: None,
            priority: None,
            assignee: None,
            sprint: None,
            due_date: None,
            origin: None,
            ..item.clone()
        };
        let body = item.description.as_deref().unwrap_or_default();
        files.insert(
            format!("{ITEMS_DIR}{key}{ITEM_EXTENSION}"),
            format!("{FENCE}\n{}{FENCE}\n{body}", yaml(&plan)),
        );
    }
    files
}

fn yaml<T: serde::Serialize>(value: &T) -> String {
    let mut text = serde_saphyr::to_string(value).expect("a plan is always serialisable");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests;
