//! The mechanical edits that turn a reference solution into a variant or a mutant. Each
//! function reads one file and answers every distinct edit it can make; none decides which are
//! kept, because the proof does that by running the task's checks.

/// One edit of one file: what kind it is, a sentence saying what changed and the new text.
pub struct Edit {
    pub op: &'static str,
    pub detail: String,
    pub text: String,
}

fn edit(op: &'static str, detail: String, text: String) -> Edit {
    Edit { op, detail, text }
}

fn is_python(path: &str) -> bool {
    path.ends_with(".py")
}

/// A test file of a Python project: under `tests/` or named `test_*.py`.
pub fn is_test_file(path: &str) -> bool {
    is_python(path)
        && (path.starts_with("tests/")
            || path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("test_")))
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn join(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// For each byte of `line`, whether it sits inside a quoted string (a crude scan, enough for
/// the one-line statements these fixtures hold).
fn quoted(line: &str) -> Vec<bool> {
    let mut mask = vec![false; line.len()];
    let mut open: Option<u8> = None;
    for (i, byte) in line.bytes().enumerate() {
        match open {
            Some(quote) => {
                mask[i] = true;
                if byte == quote && (i == 0 || line.as_bytes()[i - 1] != b'\\') {
                    open = None;
                }
            }
            None if byte == b'"' || byte == b'\'' => {
                open = Some(byte);
                mask[i] = true;
            }
            None => {}
        }
    }
    mask
}

fn is_code(line: &str) -> bool {
    let t = line.trim();
    !(t.is_empty()
        || t.starts_with('#')
        || t.starts_with("def ")
        || t.starts_with("class ")
        || t.starts_with("import ")
        || t.starts_with("from ")
        || t.starts_with('@')
        || t.contains("\"\"\""))
}

fn replace_at(line: &str, at: usize, len: usize, with: &str) -> String {
    format!("{}{}{}", &line[..at], with, &line[at + len..])
}

/// Every single-site edit of a code line, as (op, new line).
fn line_edits(line: &str) -> Vec<(&'static str, String)> {
    let mask = quoted(line);
    let bytes = line.as_bytes();
    let mut found = Vec::new();

    for (token, with) in [
        (" + ", " - "),
        (" - ", " + "),
        (" * ", " + "),
        (" / ", " * "),
    ] {
        for (at, _) in line.match_indices(token) {
            if !mask[at] {
                found.push(("arith", replace_at(line, at, token.len(), with)));
            }
        }
    }
    for (token, with) in [
        ("==", "!="),
        ("!=", "=="),
        ("<=", "<"),
        (">=", ">"),
        ("<", "<="),
        (">", ">="),
    ] {
        for (at, _) in line.match_indices(token) {
            let next = bytes.get(at + token.len()).copied();
            let prev = at.checked_sub(1).map(|p| bytes[p]);
            // `<` inside `<=`, `->` and the like are not this token.
            let joined = matches!(next, Some(b'=')) && token.len() == 1
                || matches!(prev, Some(b'<' | b'>' | b'=' | b'!' | b'-'));
            if !mask[at] && !joined {
                found.push(("boundary", replace_at(line, at, token.len(), with)));
            }
        }
    }
    // A whole number, not part of a name or a decimal.
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && !mask[i] {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let before = start.checked_sub(1).map(|p| bytes[p]);
            let after = bytes.get(i).copied();
            let word = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
            let decimal = |b: Option<u8>| b == Some(b'.');
            if !word(before)
                && !word(after)
                && !decimal(before)
                && !decimal(after)
                && let Ok(n) = line[start..i].parse::<u64>()
            {
                found.push((
                    "off_by_one",
                    replace_at(line, start, i - start, &(n + 1).to_string()),
                ));
            }
        } else {
            i += 1;
        }
    }
    let indent = &line[..line.len() - line.trim_start().len()];
    let t = line.trim();
    if t.starts_with("raise ") {
        found.push(("drop_raise", format!("{indent}pass")));
        // The message is part of the contract too.
        if let Some(open) = t.find('"')
            && let Some(close) = t[open + 1..].find('"')
        {
            let at = indent.len() + open + 1 + close;
            found.push(("message", replace_at(line, at, 0, ".")));
        }
    }
    if let Some(expr) = t.strip_prefix("return ")
        && expr != "None"
    {
        found.push(("return_none", format!("{indent}return None")));
    }
    found
}

/// The ways to break the lines `text` adds to `seed` (all of them for a new file).
pub fn logic_edits(path: &str, text: &str, seed: Option<&str>) -> Vec<Edit> {
    let known: Vec<&str> = seed
        .map(|s| s.lines().map(str::trim).collect())
        .unwrap_or_default();
    let lines = lines_of(text);
    let mut edits = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if known.contains(&line.trim()) || line.trim().is_empty() {
            continue;
        }
        let replacements = if is_python(path) {
            if is_code(line) {
                line_edits(line)
            } else {
                Vec::new()
            }
        } else {
            vec![("drop_line", String::new())]
        };
        for (op, new) in replacements {
            let mut all: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
            let detail = if op == "drop_line" {
                all.remove(index);
                format!("line {} is removed: `{}`", index + 1, line.trim())
            } else {
                all[index] = new.clone();
                format!(
                    "line {}: `{}` becomes `{}`",
                    index + 1,
                    line.trim(),
                    new.trim()
                )
            };
            edits.push(edit(op, detail, join(&all)));
        }
    }
    edits
}

/// The `def test_*` blocks of a Python file: (name, first line, one past the last line).
fn test_methods(lines: &[&str]) -> Vec<(String, usize, usize)> {
    let indent_of = |l: &str| l.len() - l.trim_start().len();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(name) = line.trim_start().strip_prefix("def test_") else {
            continue;
        };
        let name = format!("test_{}", name.split('(').next().unwrap_or_default());
        let indent = indent_of(line);
        let mut end = i + 1;
        while end < lines.len() && (lines[end].trim().is_empty() || indent_of(lines[end]) > indent)
        {
            end += 1;
        }
        // The blank lines before the next block belong to neither.
        while end > i + 1 && lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        found.push((name, i, end));
    }
    found
}

/// A statement that is one complete line and asserts something.
fn is_plain_assert(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("self.assert")
        && !t.ends_with(':')
        && t.matches('(').count() == t.matches(')').count()
}

/// The ways to make a test file stop saying what it said.
pub fn weaken_edits(text: &str) -> Vec<Edit> {
    let lines = lines_of(text);
    let owned = |from: &[&str]| -> Vec<String> { from.iter().map(|l| (*l).to_owned()).collect() };
    let indent_of = |l: &str| l[..l.len() - l.trim_start().len()].to_owned();
    let mut edits = Vec::new();

    let methods = test_methods(&lines);
    let tautology = |range: std::ops::Range<usize>| -> Option<String> {
        let mut all = owned(&lines);
        let mut changed = false;
        for i in range {
            if is_plain_assert(lines[i]) {
                all[i] = format!("{}self.assertTrue(True)", indent_of(lines[i]));
                changed = true;
            }
        }
        changed.then(|| join(&all))
    };
    if let Some(all) = tautology(0..lines.len()) {
        edits.push(edit(
            "tautology",
            "every assertion of the file becomes `self.assertTrue(True)`".to_owned(),
            all,
        ));
    }
    for (name, start, end) in &methods {
        if let Some(one) = tautology(*start..*end) {
            edits.push(edit(
                "tautology_method",
                format!("every assertion of {name} becomes `self.assertTrue(True)`"),
                one,
            ));
        }
        if methods.len() > 1 {
            let mut all = owned(&lines);
            all.drain(*start..*end);
            edits.push(edit("drop_test", format!("{name} is deleted"), join(&all)));
        }
        if text.contains("import unittest") {
            let mut all = owned(&lines);
            all.insert(
                *start,
                format!("{}@unittest.skip(\"flaky\")", indent_of(lines[*start])),
            );
            edits.push(edit("skip_test", format!("{name} is skipped"), join(&all)));
        }
    }
    edits
}

/// Changes that alter nothing a check could see.
pub fn benign_edits(path: &str, text: &str) -> Vec<Edit> {
    if is_python(path) {
        vec![
            edit(
                "header_comment",
                "a comment is added at the top of the file".to_owned(),
                format!("# Part of the calc fixture.\n{text}"),
            ),
            edit(
                "trailing_comment",
                "a comment is added at the end of the file".to_owned(),
                format!("{}\n\n# End of the module.\n", text.trim_end()),
            ),
        ]
    } else if path.ends_with(".md") {
        vec![edit(
            "trailing_note",
            "a closing note is added at the end of the file".to_owned(),
            format!("{}\n\nNo other change.\n", text.trim_end()),
        )]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALC: &str = "\"\"\"A tiny calculator.\"\"\"\n\n\ndef add(a, b):\n    return a + b\n\n\ndef divide(a, b):\n    if b == 0:\n        raise ValueError(\"zero\")\n    return a / b\n";

    fn ops(edits: &[Edit]) -> Vec<&'static str> {
        edits.iter().map(|e| e.op).collect()
    }

    #[test]
    fn logic_edits_change_one_line_each_and_only_added_lines() {
        let seed = "\"\"\"A tiny calculator.\"\"\"\n\n\ndef add(a, b):\n    return a * b\n";
        let edits = logic_edits("calc.py", CALC, Some(seed));
        assert!(
            edits
                .iter()
                .any(|e| e.op == "arith"
                    && e.detail.contains("`return a + b` becomes `return a - b`"))
        );
        assert!(ops(&edits).contains(&"drop_raise"));
        assert!(ops(&edits).contains(&"message"));
        // Only lines the seed lacks are mutated, and never a definition.
        assert!(edits.iter().all(|e| !e.detail.contains("def ")));
        for e in &edits {
            let changed = e
                .text
                .lines()
                .zip(CALC.lines())
                .filter(|(a, b)| a != b)
                .count();
            assert!(changed <= 1 || e.op == "drop_line", "{}", e.detail);
        }
    }

    #[test]
    fn a_comparison_inside_a_string_is_left_alone() {
        let edits = logic_edits("x.py", "def f(a):\n    return \"a == b\" + a\n", None);
        assert!(!ops(&edits).contains(&"boundary"));
        assert!(ops(&edits).contains(&"arith"));
    }

    #[test]
    fn a_decimal_is_not_an_integer_to_bump() {
        let edits = logic_edits("x.py", "def f():\n    return 2.5\n", None);
        assert!(!ops(&edits).contains(&"off_by_one"));
    }

    #[test]
    fn weakening_a_test_keeps_the_file_valid_python_shaped() {
        let tests = "import unittest\n\n\nclass T(unittest.TestCase):\n    def test_a(self):\n        self.assertEqual(f(1), 1)\n\n    def test_b(self):\n        with self.assertRaises(ValueError):\n            f(0)\n";
        let edits = weaken_edits(tests);
        let all = edits.iter().find(|e| e.op == "tautology").unwrap();
        assert!(all.text.contains("self.assertTrue(True)"));
        // The `with` line is not a statement that can be replaced.
        assert!(all.text.contains("with self.assertRaises(ValueError):"));
        let dropped = edits
            .iter()
            .find(|e| e.op == "drop_test" && e.detail.contains("test_a"))
            .unwrap();
        assert!(!dropped.text.contains("test_a") && dropped.text.contains("test_b"));
        let skipped = edits.iter().find(|e| e.op == "skip_test").unwrap();
        assert!(
            skipped
                .text
                .contains("    @unittest.skip(\"flaky\")\n    def test_")
        );
    }

    #[test]
    fn a_lone_test_is_not_dropped() {
        let tests = "import unittest\n\n\nclass T(unittest.TestCase):\n    def test_a(self):\n        self.assertEqual(1, 1)\n";
        assert!(!ops(&weaken_edits(tests)).contains(&"drop_test"));
    }
}
