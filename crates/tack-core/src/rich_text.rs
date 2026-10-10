//! An item's description as an agent reads it. The board's editor stores a
//! small HTML vocabulary (`frontend/src/shared/ui/RichTextEditor.tsx`); the
//! agent is given that as Markdown-flavoured plain text — paragraphs, lists,
//! headings, emphasis — instead of markup. A description with none of that
//! vocabulary, as written through the API, the CLI or MCP, comes back as is.

use std::sync::LazyLock;

use regex::Regex;

/// A tag the editor writes, or the `div` a browser inserts on Enter.
static EDITOR_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)</?(?:p|br|div|strong|b|em|i|u|s|ul|ol|li|h[1-6]|blockquote|pre|code|a)(?:\s[^>]*)?/?>",
    )
    .expect("a valid pattern")
});

static HREF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).expect("a valid pattern")
});

/// `description` as plain text, when it is the editor's HTML; unchanged otherwise.
pub fn to_plain_text(description: &str) -> String {
    if !EDITOR_TAG.is_match(description) {
        return description.to_owned();
    }
    let mut writer = Writer::default();
    let mut rest = description;
    while let Some(open) = rest.find('<') {
        writer.text(&rest[..open]);
        rest = &rest[open..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let is_tag = rest[1..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_alphabetic() || next == '/');
        match (is_tag, rest.find('>')) {
            (true, Some(close)) => {
                writer.tag(&rest[1..close]);
                rest = &rest[close + 1..];
            }
            _ => {
                writer.text("<");
                rest = &rest[1..];
            }
        }
    }
    writer.text(rest);
    writer.out.trim().to_owned()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Break {
    #[default]
    None,
    Line,
    Paragraph,
}

#[derive(Default)]
struct Writer {
    out: String,
    /// The break owed before the next text.
    pending: Break,
    /// Open lists, innermost last: `None` is bulleted, `Some(n)` numbered at `n`.
    lists: Vec<Option<u32>>,
    /// The list item marker the next line opens with.
    marker: Option<String>,
    heading: Option<usize>,
    quote: usize,
    pre: usize,
    /// An open link: its target and where its text starts in `out`.
    link: Option<(String, usize)>,
    /// Inside `script` or `style`, whose content is never text.
    skipping: Option<String>,
}

impl Writer {
    fn owe(&mut self, owed: Break) {
        self.pending = self.pending.max(owed);
    }

    /// A paragraph between blocks, a line between blocks inside a list.
    fn block(&self) -> Break {
        if self.lists.is_empty() {
            Break::Paragraph
        } else {
            Break::Line
        }
    }

    fn text(&mut self, raw: &str) {
        if self.skipping.is_some() || raw.is_empty() {
            return;
        }
        let decoded = decode_entities(raw);
        if self.pre > 0 {
            self.write(&decoded);
            return;
        }
        let collapsed = decoded
            .split(|c: char| c.is_whitespace())
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let lead = decoded.starts_with(char::is_whitespace) && !collapsed.is_empty();
        let tail = decoded.ends_with(char::is_whitespace);
        let mut text = String::new();
        if lead || (collapsed.is_empty() && tail) {
            text.push(' ');
        }
        text.push_str(&collapsed);
        if tail && !collapsed.is_empty() {
            text.push(' ');
        }
        self.write(&text);
    }

    /// Writes `content`, opening the line it owes first. Leading spaces of a
    /// line are dropped, so a whitespace-only text never opens one.
    fn write(&mut self, content: &str) {
        let opens_line = self.pending != Break::None || self.out.is_empty();
        let content = if opens_line && self.pre == 0 {
            content.trim_start()
        } else {
            content
        };
        if content.is_empty() {
            return;
        }
        if opens_line {
            if !self.out.is_empty() {
                let kept = self.out.trim_end_matches(' ').len();
                self.out.truncate(kept);
                self.out.push_str(match self.pending {
                    Break::Paragraph => "\n\n",
                    _ => "\n",
                });
            }
            for _ in 0..self.quote {
                self.out.push_str("> ");
            }
            if !self.lists.is_empty() {
                self.out.push_str(&"  ".repeat(self.lists.len() - 1));
                match self.marker.take() {
                    Some(marker) => self.out.push_str(&marker),
                    None => self.out.push_str("  "),
                }
            }
            if let Some(level) = self.heading.take() {
                self.out.push_str(&"#".repeat(level));
                self.out.push(' ');
            }
            self.pending = Break::None;
        }
        self.out.push_str(content);
    }

    fn tag(&mut self, body: &str) {
        let closing = body.starts_with('/');
        let name: String = body
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some(skipped) = &self.skipping {
            if closing && *skipped == name {
                self.skipping = None;
            }
            return;
        }
        match (name.as_str(), closing) {
            ("script" | "style", false) => self.skipping = Some(name),
            ("p" | "div", _) => self.owe(self.block()),
            ("br", _) => {
                self.pending = if self.pending >= Break::Line {
                    Break::Paragraph
                } else {
                    Break::Line
                };
            }
            ("ul" | "ol", false) => {
                self.owe(self.block());
                self.lists.push((name == "ol").then_some(1));
            }
            ("ul" | "ol", true) => {
                self.lists.pop();
                self.owe(self.block());
            }
            ("li", false) => {
                self.owe(Break::Line);
                let marker = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}. ");
                        *number += 1;
                        marker
                    }
                    _ => "- ".to_owned(),
                };
                self.marker = Some(marker);
            }
            ("li", true) => self.owe(Break::Line),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                self.owe(Break::Paragraph);
                self.heading = name[1..].parse().ok();
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => {
                self.heading = None;
                self.owe(Break::Paragraph);
            }
            ("blockquote", false) => {
                self.owe(Break::Paragraph);
                self.quote += 1;
            }
            ("blockquote", true) => {
                self.quote = self.quote.saturating_sub(1);
                self.owe(Break::Paragraph);
            }
            ("pre", false) => {
                self.owe(Break::Paragraph);
                self.write("```");
                self.pre += 1;
                self.owe(Break::Line);
            }
            ("pre", true) => {
                self.pre = self.pre.saturating_sub(1);
                self.owe(Break::Line);
                self.write("```");
                self.owe(Break::Paragraph);
            }
            ("code", _) if self.pre == 0 => self.write("`"),
            ("strong" | "b", _) => self.write("**"),
            ("em" | "i", _) => self.write("*"),
            ("s", _) => self.write("~~"),
            ("a", false) => {
                let href = HREF
                    .captures(body)
                    .and_then(|found| found.get(1).or(found.get(2)).or(found.get(3)))
                    .map(|target| decode_entities(target.as_str()))
                    .unwrap_or_default();
                self.link = Some((href, self.out.len()));
            }
            ("a", true) => {
                if let Some((href, start)) = self.link.take() {
                    let shown = self.out.get(start..).unwrap_or_default().trim();
                    if !href.is_empty() && shown != href {
                        self.write(&format!(" ({href})"));
                    }
                }
            }
            _ => {}
        }
    }
}

/// HTML's named entities the editor emits, and numeric references.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let character = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .map_or_else(
                        || entity.strip_prefix('#')?.parse().ok(),
                        |hex| u32::from_str_radix(hex, 16).ok(),
                    )
                    .and_then(char::from_u32),
            };
            character.map(|character| (character, end))
        });
        match decoded {
            Some((character, end)) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "rich_text/tests.rs"]
mod tests;
