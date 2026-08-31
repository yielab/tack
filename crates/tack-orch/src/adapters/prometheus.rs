//! A tolerant parser for the Prometheus text exposition format, as emitted
//! by docket's `GET /metrics` (`docket/serve.py`'s `render_metrics()`).
//!
//! Scope is deliberately narrow — this is not a general Prometheus client:
//! no `# TYPE`/`# HELP` semantics are retained, no histogram/summary
//! bucket-line reconstruction, no exemplars. It turns each data line into a
//! flat [`MetricSample`] (`name` + `labels` + `value`) and nothing else,
//! which is all [`crate::ControlPlane::metrics`] promises callers.
//!
//! The metrics-ingestion path reuses this module as-is — **do not write a
//! second parser**.
//!
//! # Public path
//!
//! This lives at `adapters::prometheus` (physically
//! `src/adapters/prometheus.rs`), not at the crate root — import it as
//! `tack_orch::adapters::prometheus::parse`.
//!
//! # Never panics
//!
//! Every code path here works over `char`s (via `Peekable<Chars>`), never
//! raw byte offsets — a raw `&str[a..b]` slice computed from a byte position
//! that lands mid-UTF-8-codepoint panics, and this parser's whole job is to
//! survive input it doesn't control. A line (or one label within a line)
//! that doesn't parse is dropped; it never aborts the rest of the scrape.

use std::collections::BTreeMap;
use std::iter::Peekable;
use std::str::Chars;

use crate::MetricSample;

/// Parse a full `/metrics` response body into zero or more samples.
///
/// - Blank lines and any line whose first non-whitespace character is `#`
///   (docket's `# HELP ...` / `# TYPE ...` lines) are skipped entirely.
/// - A line that doesn't parse cleanly (malformed labels, an unterminated
///   string, a non-numeric value, ...) is dropped rather than aborting the
///   whole scrape — one bad line must never lose every other metric.
pub fn parse(input: &str) -> Vec<MetricSample> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(parse_line)
        .collect()
}

/// Parse one non-comment, non-blank line: `name{k="v",...} value` or the
/// label-less `name value`. Returns `None` for anything that doesn't fit
/// that shape — never panics.
fn parse_line(line: &str) -> Option<MetricSample> {
    let mut chars = line.chars().peekable();

    let name = take_while_name(&mut chars);
    if name.is_empty() {
        return None;
    }

    skip_whitespace(&mut chars);

    let labels = if chars.peek() == Some(&'{') {
        chars.next(); // consume '{'
        parse_labels(&mut chars)?
    } else {
        BTreeMap::new()
    };

    skip_whitespace(&mut chars);

    // Whatever remains on the line: the value is its first whitespace-
    // delimited token. Trailing garbage after it (a stray comment, an extra
    // field a future docket version tacks on, ...) is ignored rather than
    // rejected — see the module doc's "never panics" note.
    let remainder: String = chars.collect();
    let value_token = remainder.split_whitespace().next()?;
    let value = parse_value(value_token)?;

    Some(MetricSample {
        name,
        labels,
        value,
    })
}

/// Consume characters up to (not including) `{` or whitespace.
fn take_while_name(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c == '{' || c.is_whitespace() {
            break;
        }
        name.push(c);
        chars.next();
    }
    name
}

fn skip_whitespace(chars: &mut Peekable<Chars<'_>>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

/// Parse a `{k="v", k2="v2", ...}` label set. The caller has already
/// consumed the opening `{`. Returns `None` on any structural problem
/// (missing `=`, missing quotes, an unterminated string, ...) so the whole
/// line is dropped rather than partially applied.
fn parse_labels(chars: &mut Peekable<Chars<'_>>) -> Option<BTreeMap<String, String>> {
    let mut labels = BTreeMap::new();
    loop {
        // Skip separators between entries: whitespace and commas.
        loop {
            match chars.peek() {
                Some(&c) if c.is_whitespace() || c == ',' => {
                    chars.next();
                }
                _ => break,
            }
        }
        match chars.peek() {
            Some('}') => {
                chars.next();
                return Some(labels);
            }
            None => return None, // unterminated label set
            _ => {}
        }

        // Key: up to '=' or whitespace.
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if key.is_empty() {
            return None;
        }

        skip_whitespace(chars);
        if chars.next() != Some('=') {
            return None; // missing labels entirely is fine (no `{...}` at
            // all); a label with no `=` inside a `{...}` we
            // do have is malformed and voids the whole line.
        }
        skip_whitespace(chars);
        if chars.next() != Some('"') {
            return None;
        }

        let value = parse_quoted_value(chars)?;
        labels.insert(key, value);
    }
}

/// Parse a double-quoted label value (opening quote already consumed),
/// honouring `\"`, `\\`, and `\n` escapes exactly like docket's own
/// (Python `json`-flavoured) escaping. Any other `\X` keeps `X` verbatim
/// rather than erroring on an escape sequence this parser doesn't know.
fn parse_quoted_value(chars: &mut Peekable<Chars<'_>>) -> Option<String> {
    let mut value = String::new();
    loop {
        match chars.next()? {
            '\\' => match chars.next()? {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                other => value.push(other),
            },
            '"' => return Some(value),
            c => value.push(c),
        }
    }
}

/// Parse a metric value token, honouring Prometheus's `NaN`/`+Inf`/`-Inf`
/// spellings explicitly (rather than trusting `f64::from_str` to accept
/// exactly this casing/sign combination) plus any ordinary float Rust's own
/// parser accepts.
fn parse_value(token: &str) -> Option<f64> {
    match token {
        "NaN" | "nan" | "NAN" => Some(f64::NAN),
        "+Inf" | "Inf" | "inf" | "+inf" | "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Inf" | "-inf" | "-Infinity" => Some(f64::NEG_INFINITY),
        other => other.parse::<f64>().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_name_value() {
        let samples = parse("docket_agents_total 3");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "docket_agents_total");
        assert!(samples[0].labels.is_empty());
        assert_eq!(samples[0].value, 3.0);
    }

    #[test]
    fn parses_labelled_metric() {
        let samples = parse(r#"docket_agent_cost_usd{agent="demo-lead",model="claude"} 1.5"#);
        assert_eq!(samples.len(), 1);
        let s = &samples[0];
        assert_eq!(s.name, "docket_agent_cost_usd");
        assert_eq!(s.labels.get("agent").map(String::as_str), Some("demo-lead"));
        assert_eq!(s.labels.get("model").map(String::as_str), Some("claude"));
        assert_eq!(s.value, 1.5);
    }

    #[test]
    fn skips_help_and_type_and_blank_lines() {
        let input = "# HELP x a thing\n# TYPE x gauge\n\nx 1\n";
        let samples = parse(input);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "x");
    }

    #[test]
    fn tolerates_extra_whitespace_around_labels() {
        let samples = parse(r#"docket_x{  label = "v"  ,  other="w"  }    12.5"#);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].value, 12.5);
        assert_eq!(
            samples[0].labels.get("label").map(String::as_str),
            Some("v")
        );
        assert_eq!(
            samples[0].labels.get("other").map(String::as_str),
            Some("w")
        );
    }

    #[test]
    fn handles_escaped_quotes_in_label_values() {
        let samples = parse(r#"docket_x{label="a \"quoted\" value"} 3"#);
        assert_eq!(samples.len(), 1);
        assert_eq!(
            samples[0].labels.get("label").map(String::as_str),
            Some(r#"a "quoted" value"#)
        );
    }

    #[test]
    fn parses_nan_and_inf_values() {
        assert!(parse(r#"x{a="b"} NaN"#)[0].value.is_nan());
        assert_eq!(parse(r#"x{a="b"} +Inf"#)[0].value, f64::INFINITY);
        assert_eq!(parse(r#"x{a="b"} -Inf"#)[0].value, f64::NEG_INFINITY);
    }

    #[test]
    fn missing_labels_is_fine() {
        let samples = parse("bare_metric_no_labels 42");
        assert_eq!(samples.len(), 1);
        assert!(samples[0].labels.is_empty());
    }

    #[test]
    fn malformed_lines_are_dropped_not_panicking() {
        let input = "\
this line has no braces and no trailing number\n\
docket_bad_no_value{label=\"x\"}\n\
docket_bad_unterminated_brace{label=\"x\" 5\n\
docket_weird_label{label=} 5\n\
good_metric_after_garbage 9\n";
        // Must not panic; the one well-formed line still comes through.
        let samples = parse(input);
        assert!(
            samples
                .iter()
                .any(|s| s.name == "good_metric_after_garbage")
        );
    }

    #[test]
    fn real_metrics_fixture_never_panics_and_parses_known_series() {
        let raw = include_str!("../../tests/fixtures/metrics_with_agent.txt");
        let samples = parse(raw);
        let names: Vec<&str> = samples.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"docket_agents_total"));
        assert!(names.contains(&"docket_agent_cost_usd"));
        assert!(names.contains(&"docket_approvals_pending_total"));
        assert!(names.contains(&"docket_turn_duration_seconds_sum"));
    }

    #[test]
    fn malformed_fixture_never_panics() {
        let raw = include_str!("../../tests/fixtures/metrics_malformed.txt");
        // The only assertion that matters here is "did not panic"; still
        // check the two well-formed lines in that fixture survive.
        let samples = parse(raw);
        let names: Vec<&str> = samples.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"a_gauge"));
        assert!(names.contains(&"bare_metric_no_labels"));
    }

    #[test]
    fn non_utf8_boundary_hostile_input_does_not_panic() {
        // Unicode content in a label value / metric name must not panic a
        // byte-offset-based implementation; this parser is char-based, but
        // assert the property directly since it's the whole point.
        let input = "docket_x{label=\"emoji \u{1F600} boundary\"} 1\n\u{1F600}weird_name 2\n";
        let samples = parse(input);
        assert!(!samples.is_empty());
    }
}
