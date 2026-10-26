use super::*;
use chrono::{DateTime, Utc};
use tack_test_support::setup_test_db;

struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[test]
fn the_default_window_is_ninety_days_in_batches_of_five_hundred() {
    let settings = Settings::default();
    assert_eq!(settings.retention_days, 90);
    assert_eq!(settings.batch_size, 500);
}

#[tokio::test]
async fn a_pass_over_an_empty_store_purges_nothing_and_does_not_fail() {
    let repo = setup_test_db().await;
    let clock = FixedClock(Utc::now());

    let outcome = tick(&repo, &clock, &Settings::default())
        .await
        .expect("an empty store is not an error");

    assert_eq!(outcome, Outcome::default());
}
