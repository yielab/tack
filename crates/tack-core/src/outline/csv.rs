//! CSV in and out of the outline shape. A CSV row is an outline item, so an
//! import goes through [`resolve`](super::resolve) like any other outline, and
//! an export carries the columns an import reads back: `id` becomes the row's
//! ref and `parent_id` names it, so an exported project re-imports with its
//! hierarchy.

use super::{OutlineError, OutlineItem};
use crate::models::Item;

/// The rows of a CSV file as outline items, plus what the file held that
/// does not become an item.
#[derive(Debug, Default)]
pub struct CsvOutline {
    pub items: Vec<OutlineItem>,
    /// Rows with no title.
    pub skipped: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy)]
enum Column {
    Ref,
    Title,
    Description,
    Type,
    Status,
    Priority,
    Assignee,
    Estimate,
    DueDate,
    Tags,
    Recurrence,
    Parent,
    DependsOn,
    /// A column an export writes that an import has no field for.
    Ignored,
}

fn column(header: &str) -> Option<Column> {
    Some(match header {
        "ref" | "id" | "key" => Column::Ref,
        "title" => Column::Title,
        "description" => Column::Description,
        "type" | "item_type" => Column::Type,
        "status" => Column::Status,
        "priority" => Column::Priority,
        "assignee" => Column::Assignee,
        "estimate" => Column::Estimate,
        "due_date" | "due" => Column::DueDate,
        "tags" | "labels" => Column::Tags,
        "recurrence" | "repeats" => Column::Recurrence,
        "parent" | "parent_id" | "parent_ref" => Column::Parent,
        "depends_on" | "blocked_by" => Column::DependsOn,
        "created_at" => Column::Ignored,
        _ => return None,
    })
}

/// Cells that hold several values (`tags`, `depends_on`) separate them with
/// `;` or `|`, never `,`, which a title may contain.
fn split_list(cell: &str) -> Vec<String> {
    cell.split([';', '|'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read a CSV with a header row. Headers are matched in any case; only
/// `title` is required. Quoted cells may span lines.
pub fn items_from_csv(text: &str) -> Result<CsvOutline, OutlineError> {
    let text = text.trim_start_matches('\u{feff}');
    let mut reader = ::csv::ReaderBuilder::new()
        .flexible(true)
        .trim(::csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| OutlineError::one(format!("the CSV header row cannot be read: {e}")))?
        .clone();
    if headers.iter().all(str::is_empty) {
        return Err(OutlineError::one("the CSV has no header row"));
    }

    let mut out = CsvOutline::default();
    let mut columns = Vec::with_capacity(headers.len());
    let mut unread = Vec::new();
    for header in &headers {
        let name = header.to_lowercase();
        let found = column(&name);
        if found.is_none() && !name.is_empty() {
            unread.push(header.to_string());
        }
        columns.push(found);
    }
    if !columns.iter().any(|c| matches!(c, Some(Column::Title))) {
        return Err(OutlineError::one("the CSV must have a title column"));
    }
    if !unread.is_empty() {
        out.warnings.push(format!(
            "columns not read: {}",
            unread
                .iter()
                .map(|h| format!("\"{h}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let mut problems = Vec::new();
    for record in reader.records() {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("the CSV cannot be read: {e}"));
                break;
            }
        };
        let row = record.position().map_or(0, |p| p.line());
        let mut item = OutlineItem {
            origin: Some(format!("row {row}")),
            ..OutlineItem::default()
        };
        for (cell, col) in record.iter().zip(&columns) {
            if cell.is_empty() {
                continue;
            }
            let text = Some(cell.to_string());
            match col {
                Some(Column::Ref) => item.reference = text,
                Some(Column::Title) => item.title = cell.to_string(),
                Some(Column::Description) => item.description = text,
                Some(Column::Type) => item.item_type = text,
                Some(Column::Status) => item.status = text,
                Some(Column::Priority) => item.priority = text,
                Some(Column::Assignee) => item.assignee = text,
                Some(Column::Estimate) => match cell.parse() {
                    Ok(n) => item.estimate = Some(n),
                    Err(_) => {
                        problems.push(format!("row {row}: estimate \"{cell}\" is not a number"))
                    }
                },
                Some(Column::DueDate) => item.due_date = text,
                Some(Column::Tags) => item.tags = split_list(cell),
                Some(Column::Recurrence) => item.recurrence = text,
                Some(Column::Parent) => item.parent = text,
                Some(Column::DependsOn) => item.depends_on = split_list(cell),
                Some(Column::Ignored) | None => {}
            }
        }
        if item.title.is_empty() {
            if record.iter().any(|c| !c.is_empty()) {
                out.skipped += 1;
            }
            continue;
        }
        out.items.push(item);
    }
    if out.skipped > 0 {
        out.warnings.push(format!(
            "{} row{} with no title skipped",
            out.skipped,
            if out.skipped == 1 { "" } else { "s" }
        ));
    }
    if !problems.is_empty() {
        return Err(OutlineError { problems });
    }
    Ok(out)
}

/// A due date at midnight UTC — how a date-only one is stored — as the date
/// alone; any other instant in full.
fn due_date_cell(at: chrono::DateTime<chrono::Utc>) -> String {
    if at.time() == chrono::NaiveTime::MIN {
        at.format("%Y-%m-%d").to_string()
    } else {
        at.to_rfc3339()
    }
}

/// The CSV export: one row per item, in the columns [`items_from_csv`] reads.
pub fn items_to_csv(items: &[Item]) -> String {
    let mut writer = ::csv::Writer::from_writer(Vec::new());
    let header = [
        "id",
        "title",
        "description",
        "type",
        "status",
        "priority",
        "assignee",
        "estimate",
        "due_date",
        "tags",
        "recurrence",
        "parent_id",
        "created_at",
    ];
    writer
        .write_record(header)
        .expect("writing to memory cannot fail");
    for item in items {
        writer
            .write_record([
                item.id.to_string(),
                item.title.clone(),
                item.description.clone().unwrap_or_default(),
                item.item_type.to_string(),
                item.status.clone(),
                item.priority.to_string(),
                item.assignee.clone().unwrap_or_default(),
                item.estimate.map(|e| e.to_string()).unwrap_or_default(),
                item.due_date.map(due_date_cell).unwrap_or_default(),
                item.tags.join(";"),
                item.recurrence.map(|r| r.to_string()).unwrap_or_default(),
                item.parent_id.map(|p| p.to_string()).unwrap_or_default(),
                item.created_at.to_rfc3339(),
            ])
            .expect("writing to memory cannot fail");
    }
    let bytes = writer.into_inner().expect("writing to memory cannot fail");
    String::from_utf8(bytes).expect("every field written was a String")
}
