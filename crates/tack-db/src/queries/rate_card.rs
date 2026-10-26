use tack_core::rate_card::RateCard;

use crate::{Repository, StoreError};

/// The stored rate card, `None` when the operator never entered one.
pub async fn stored_card(repo: &Repository) -> Result<Option<RateCard>, StoreError> {
    Ok(crate::repo::rate_card::load(repo.pool()).await?)
}
