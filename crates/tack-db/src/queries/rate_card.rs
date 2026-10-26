use tack_core::rate_card::RateCard;

use crate::{Repository, StoreError};

/// The stored rate card, `None` when the operator never entered one.
pub async fn stored_card(repo: &Repository) -> Result<Option<RateCard>, StoreError> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(crate::repo::rate_card::KEY)
        .fetch_optional(repo.pool())
        .await?;
    Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
}
