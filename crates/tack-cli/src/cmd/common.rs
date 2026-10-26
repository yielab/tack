pub(crate) fn print_table_header(cols: &[&str]) {
    print_table_row(cols);
    let divider: Vec<String> = cols.iter().map(|c| "─".repeat(col_width(c))).collect();
    println!("{}", divider.join("  "));
}

pub(crate) fn print_table_row(cols: &[&str]) {
    let widths = [8, 32, 12, 16, 10, 12];
    let parts: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, &val)| {
            let w = widths.get(i).copied().unwrap_or(12);
            trunc_pad(val, w)
        })
        .collect();
    println!("{}", parts.join("  "));
}

pub(crate) fn col_width(header: &str) -> usize {
    match header {
        "ID" => 8,
        "NAME" | "TITLE" => 32,
        "TYPE" | "ITEM TYPE" => 12,
        "STATUS" => 16,
        "PRIORITY" => 10,
        "ASSIGNEE" => 12,
        _ => 12,
    }
}

pub(crate) fn trunc_pad(s: &str, width: usize) -> String {
    if s.len() > width {
        format!("{}…", &s[..width - 1])
    } else {
        format!("{:<width$}", s)
    }
}

pub(crate) fn short_id(id: Option<&str>) -> &str {
    id.map(|s| &s[..8.min(s.len())]).unwrap_or("?")
}

pub(crate) fn short_date(dt: Option<&str>) -> &str {
    dt.map(|s| &s[..10.min(s.len())]).unwrap_or("-")
}

/// Minimal percent-encoding for query parameter values (spaces → %20, etc.)
pub(crate) fn urlenc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}
