//! The key of a thing in a project folder: the stable name a file and the board use to say
//! they mean the same role, field, sprint, profile or item (ADR 0025 decision 2).
//! A key is made once from the name and does not follow it when the name changes.

/// The most characters a key has.
const MAX_KEY_CHARS: usize = 64;

/// The key a name or title gets when nobody gave one.
pub fn key_of(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let cut: String = words.join("-").chars().take(MAX_KEY_CHARS).collect();
    match cut.trim_end_matches('-') {
        "" => "unnamed".to_owned(),
        key => key.to_owned(),
    }
}

/// Whether `text` can be a key a file creates a row with.
pub fn is_key(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_KEY_CHARS
        && text.to_lowercase() == text
        && !text.starts_with('-')
        && !text.ends_with('-')
        && text.chars().all(|c| c == '-' || c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_key_by_these_rules() {
        let sixty_three = "a".repeat(63);
        let seventy = "b".repeat(70);
        let cut_on_dash = format!("{sixty_three} tail");
        let rows: [(&str, String); 7] = [
            ("Contenido", "contenido".into()),
            ("En revisión", "en-revisión".into()),
            ("  Dev  Team!! ", "dev-team".into()),
            ("!!!", "unnamed".into()),
            ("", "unnamed".into()),
            (&seventy, "b".repeat(64)),
            (&cut_on_dash, sixty_three.clone()),
        ];
        for (text, key) in rows {
            assert_eq!(key_of(text), key, "key_of({text:?})");
        }
    }

    #[test]
    fn only_a_made_key_is_a_key() {
        let rows = [
            ("dev", true),
            ("dev-2", true),
            ("en-revisión", true),
            ("Dev", false),
            ("-dev", false),
            ("dev-", false),
            ("dev team", false),
            ("", false),
        ];
        for (text, expected) in rows {
            assert_eq!(is_key(text), expected, "is_key({text:?})");
        }
        assert!(!is_key(&"a".repeat(65)));
        assert!(is_key(&"a".repeat(64)));
    }
}
