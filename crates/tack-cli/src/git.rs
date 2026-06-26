//! Git conveniences: turn a Tack item into a ready-to-use branch name.
//!
//! `tack branch <item-id>` fetches the item over the HTTP API (like every other
//! CLI command — never the DB directly) and derives a conventional branch name
//! of the form `<prefix>/<short-id>-<title-slug>`, e.g.
//! `feat/a1b2c3d4-add-table-view`.

/// Maximum number of characters kept from the slugified title.
const MAX_TITLE_SLUG_LEN: usize = 40;

/// Map an item type to a short, conventional branch prefix.
///
/// Mirrors the spirit of Conventional Commits so the branch name reads the way
/// developers expect (`feature` → `feat`, `bug` → `fix`). Unknown/custom types
/// fall back to a safe slug of the type itself, or `task` when empty.
pub fn type_prefix(item_type: &str) -> String {
    match item_type {
        "feature" => "feat".to_string(),
        "bug" => "fix".to_string(),
        "epic" => "epic".to_string(),
        "task" => "task".to_string(),
        "subtask" => "task".to_string(),
        "requirement" => "req".to_string(),
        other => {
            let s = slugify(other);
            if s.is_empty() { "task".to_string() } else { s }
        }
    }
}

/// Lowercase, replace any run of non-alphanumeric characters with a single
/// hyphen, and trim leading/trailing hyphens. ASCII-only output, safe for git
/// ref names.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_hyphen = true; // treat the start as a boundary to swallow leading hyphens
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Truncate a slug to `MAX_TITLE_SLUG_LEN` characters without cutting a word in
/// half where avoidable, and never leaving a trailing hyphen.
fn truncate_slug(slug: &str) -> String {
    if slug.len() <= MAX_TITLE_SLUG_LEN {
        return slug.to_string();
    }
    let mut cut = &slug[..MAX_TITLE_SLUG_LEN];
    if let Some(idx) = cut.rfind('-') {
        // Prefer cutting on a word boundary, but only if it keeps something useful.
        if idx >= MAX_TITLE_SLUG_LEN / 2 {
            cut = &slug[..idx];
        }
    }
    cut.trim_end_matches('-').to_string()
}

/// The leading segment of a UUID (`a1b2c3d4-...` → `a1b2c3d4`), used as a stable,
/// short, collision-resistant handle in the branch name.
pub fn short_id(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_ascii_lowercase()
}

/// Build the full branch name from the item's type, id, and title.
///
/// `prefix_override`, when provided, replaces the type-derived prefix entirely.
pub fn branch_name(
    item_type: &str,
    id: &str,
    title: &str,
    prefix_override: Option<&str>,
) -> String {
    let prefix = match prefix_override {
        Some(p) => {
            let s = slugify(p);
            if s.is_empty() { "task".to_string() } else { s }
        }
        None => type_prefix(item_type),
    };
    let sid = short_id(id);
    let title_slug = truncate_slug(&slugify(title));
    if title_slug.is_empty() {
        format!("{prefix}/{sid}")
    } else {
        format!("{prefix}/{sid}-{title_slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Add Table View"), "add-table-view");
        assert_eq!(slugify("  Hello,  World!  "), "hello-world");
        assert_eq!(slugify("already-slugged"), "already-slugged");
        assert_eq!(slugify("Multiple   spaces"), "multiple-spaces");
    }

    #[test]
    fn slugify_strips_symbols_and_unicode() {
        assert_eq!(slugify("Fix: café/login (#42)"), "fix-caf-login-42");
        assert_eq!(slugify("___"), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn type_prefix_maps_conventional() {
        assert_eq!(type_prefix("feature"), "feat");
        assert_eq!(type_prefix("bug"), "fix");
        assert_eq!(type_prefix("task"), "task");
        assert_eq!(type_prefix("requirement"), "req");
        assert_eq!(type_prefix("Marketing Idea"), "marketing-idea");
        assert_eq!(type_prefix(""), "task");
    }

    #[test]
    fn short_id_takes_first_segment() {
        assert_eq!(short_id("a1b2c3d4-e5f6-7890-abcd-ef1234567890"), "a1b2c3d4");
        assert_eq!(short_id("nodash"), "nodash");
    }

    #[test]
    fn branch_name_full() {
        let b = branch_name(
            "feature",
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
            "Add Table View",
            None,
        );
        assert_eq!(b, "feat/a1b2c3d4-add-table-view");
    }

    #[test]
    fn branch_name_prefix_override() {
        let b = branch_name("bug", "a1b2c3d4-xxxx", "Crash on load", Some("hotfix"));
        assert_eq!(b, "hotfix/a1b2c3d4-crash-on-load");
    }

    #[test]
    fn branch_name_empty_title() {
        let b = branch_name("task", "a1b2c3d4-xxxx", "!!!", None);
        assert_eq!(b, "task/a1b2c3d4");
    }

    #[test]
    fn branch_name_truncates_long_title() {
        let title =
            "This is an extremely long item title that should be truncated at a word boundary";
        let b = branch_name("task", "a1b2c3d4-xxxx", title, None);
        // prefix + short id + at most MAX_TITLE_SLUG_LEN of slug
        let slug_part = b.split_once('-').unwrap().1;
        assert!(
            slug_part.len() <= MAX_TITLE_SLUG_LEN,
            "slug part too long: {slug_part}"
        );
        assert!(!b.ends_with('-'));
        assert!(b.starts_with("task/a1b2c3d4-this-is-an"));
    }
}
