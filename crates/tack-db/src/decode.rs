//! Reading a stored value back. Every row mapping goes through these, so a
//! value that cannot be read is a `StoreError::Corrupt` naming the table and
//! the column, never a panic and never a silent default.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::StoreError;

/// An id column.
pub(crate) fn uuid(
    table: &'static str,
    column: &'static str,
    text: &str,
) -> Result<Uuid, StoreError> {
    Uuid::parse_str(text).map_err(|error| StoreError::corrupt(table, column, error))
}

/// A nullable id column.
pub(crate) fn optional_uuid(
    table: &'static str,
    column: &'static str,
    text: Option<&str>,
) -> Result<Option<Uuid>, StoreError> {
    text.map(|text| uuid(table, column, text)).transpose()
}

/// An instant column: RFC 3339 as this layer writes it, or SQLite's
/// `datetime('now')` form (`2026-01-31 08:00:00`, UTC) as a column default and a
/// raw `INSERT` write it.
pub(crate) fn instant(
    table: &'static str,
    column: &'static str,
    text: &str,
) -> Result<DateTime<Utc>, StoreError> {
    text.parse::<DateTime<Utc>>().or_else(|_| {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
            .map(|naive| naive.and_utc())
            .map_err(|error| StoreError::corrupt(table, column, error))
    })
}

/// A nullable instant column.
pub(crate) fn optional_instant(
    table: &'static str,
    column: &'static str,
    text: Option<&str>,
) -> Result<Option<DateTime<Utc>>, StoreError> {
    text.map(|text| instant(table, column, text)).transpose()
}

/// A JSON column holding a `T`.
pub(crate) fn json<T: DeserializeOwned>(
    table: &'static str,
    column: &'static str,
    text: &str,
) -> Result<T, StoreError> {
    serde_json::from_str(text).map_err(|error| StoreError::corrupt(table, column, error))
}

/// A nullable JSON column holding a `T`.
pub(crate) fn optional_json<T: DeserializeOwned>(
    table: &'static str,
    column: &'static str,
    text: Option<&str>,
) -> Result<Option<T>, StoreError> {
    text.map(|text| json(table, column, text)).transpose()
}

/// A text column holding the serde name of an enum's variant.
pub(crate) fn variant<T: DeserializeOwned>(
    table: &'static str,
    column: &'static str,
    text: &str,
) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .map_err(|error| StoreError::corrupt(table, column, error))
}

/// A document written into a JSON column. A type this layer owns serializes;
/// one that does not is reported as a write that was refused.
pub(crate) fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(StoreError::unwritable)
}
