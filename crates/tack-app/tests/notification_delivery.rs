//! A notification reaches the webhook at least once: a 2xx settles it, a 5xx is tried again after
//! 30 s, 60 s and so on, a 4xx is refused for good, and the eighth failure gives up with the last
//! error kept. The body names the event and carries the notification's `id`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tack_app::background::{Loops, STOP_GRACE};
use tack_app::notifications::Inbox;
use tack_app::notifications::channel::Channel;
use tack_app::notifications::delivery::{self, MAX_ATTEMPTS};
use tack_app::notifications::webhook::WebhookChannel;
use tack_core::notification::{Key, Kind, Level, Notice, Notification};
use tack_db::Repository;
use tack_protocol::{Clock, SystemClock};
use tack_test_support::{ControllableClock, create_test_workspace, make_project, setup_test_db};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct FakeClock(ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

struct Fixture {
    repo: Repository,
    inbox: Inbox,
    clock: Arc<FakeClock>,
    start: DateTime<Utc>,
    server: MockServer,
    channels: Vec<Arc<dyn Channel>>,
}

async fn fixture(status: u16) -> Fixture {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    fixture_on(server).await
}

async fn fixture_on(server: MockServer) -> Fixture {
    let repo = setup_test_db().await;
    let start = Utc.with_ymd_and_hms(2026, 11, 3, 12, 0, 0).unwrap();
    let clock = Arc::new(FakeClock(ControllableClock::new(start)));
    let channels: Vec<Arc<dyn Channel>> = vec![Arc::new(WebhookChannel::new(
        format!("{}/hook", server.uri()),
        Some("shared-secret".into()),
    ))];
    let inbox = Inbox::new(repo.clone(), clock.clone(), vec!["webhook".into()]);
    Fixture {
        repo,
        inbox,
        clock,
        start,
        server,
        channels,
    }
}

async fn open_question(f: &Fixture) -> Notification {
    let workspace = create_test_workspace(&f.repo).await;
    let project = make_project(&f.repo, workspace).await.id;
    f.inbox
        .open(&Notice {
            kind: Kind::Decision,
            level: Level::NeedsYou,
            key: Key::decision("d-1"),
            project_id: project,
            item_id: None,
            request_id: None,
            title: "A run is asking a question".into(),
            cause: "runner:local".into(),
        })
        .await
        .expect("open")
        .expect("opened")
}

async fn pass(f: &Fixture) -> usize {
    delivery::tick(&f.repo, f.clock.as_ref(), &f.channels)
        .await
        .expect("tick")
}

async fn due_at(f: &Fixture, at: DateTime<Utc>) -> usize {
    f.repo.deliveries_due(at, 10).await.expect("due").len()
}

fn secs(n: i64) -> chrono::Duration {
    chrono::Duration::seconds(n)
}

#[test]
fn the_wait_doubles_from_30_seconds_and_stops_at_six_hours() {
    let waits: Vec<i64> = (0..4)
        .map(|attempts| delivery::backoff(attempts).num_seconds())
        .collect();
    assert_eq!(waits, [30, 60, 120, 240]);
    assert_eq!(delivery::backoff(10).num_seconds(), 6 * 3600);
    assert_eq!(delivery::backoff(u32::MAX).num_seconds(), 6 * 3600);
}

#[tokio::test]
async fn a_200_marks_the_delivery_done_and_it_is_not_sent_again() {
    let f = fixture(200).await;
    open_question(&f).await;

    assert_eq!(pass(&f).await, 1);
    f.clock.0.advance(chrono::Duration::days(1));
    assert_eq!(pass(&f).await, 0);

    assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
    let status = &f.repo.channel_statuses().await.unwrap()[0];
    assert_eq!((status.pending, status.failed), (0, 0));
    assert_eq!(status.last_error, None);
}

#[tokio::test]
async fn a_500_is_tried_again_after_30_seconds_and_then_after_60() {
    let f = fixture(500).await;
    open_question(&f).await;

    assert_eq!(pass(&f).await, 1);
    assert_eq!(due_at(&f, f.start + secs(29)).await, 0);
    assert_eq!(due_at(&f, f.start + secs(30)).await, 1);

    f.clock.0.set(f.start + secs(30));
    assert_eq!(pass(&f).await, 1);
    assert_eq!(due_at(&f, f.start + secs(30 + 59)).await, 0);
    assert_eq!(due_at(&f, f.start + secs(30 + 60)).await, 1);

    let status = &f.repo.channel_statuses().await.unwrap()[0];
    assert_eq!((status.pending, status.failed), (1, 0));
    assert!(status.last_error.as_deref().unwrap().contains("500"));
}

#[tokio::test]
async fn a_404_is_refused_for_good_with_its_error() {
    let f = fixture(404).await;
    open_question(&f).await;

    assert_eq!(pass(&f).await, 1);
    f.clock.0.advance(chrono::Duration::days(1));
    assert_eq!(pass(&f).await, 0);

    assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
    let status = &f.repo.channel_statuses().await.unwrap()[0];
    assert_eq!((status.pending, status.failed), (0, 1));
    assert!(status.last_error.as_deref().unwrap().contains("404"));
}

#[tokio::test]
async fn the_eighth_failure_gives_up_and_keeps_the_error() {
    let f = fixture(503).await;
    open_question(&f).await;

    for attempt in 1..=MAX_ATTEMPTS {
        f.clock.0.advance(chrono::Duration::days(1));
        assert_eq!(pass(&f).await, 1, "attempt {attempt}");
        let status = &f.repo.channel_statuses().await.unwrap()[0];
        let gave_up = attempt == MAX_ATTEMPTS;
        assert_eq!(
            (status.pending, status.failed),
            if gave_up { (0, 1) } else { (1, 0) },
            "after attempt {attempt}"
        );
    }

    f.clock.0.advance(chrono::Duration::days(30));
    assert_eq!(pass(&f).await, 0);
    assert_eq!(
        f.server.received_requests().await.unwrap().len(),
        MAX_ATTEMPTS as usize
    );
    let status = &f.repo.channel_statuses().await.unwrap()[0];
    assert!(status.last_error.as_deref().unwrap().contains("503"));
}

#[tokio::test]
async fn the_body_names_the_event_and_carries_the_notification_signed() {
    let f = fixture(200).await;
    let opened = open_question(&f).await;

    pass(&f).await;

    let requests = f.server.received_requests().await.unwrap();
    let request = &requests[0];
    assert_eq!(
        request.headers.get("x-tack-event").unwrap(),
        "notification.decision"
    );
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["event"], "notification.decision");
    assert_eq!(body["notification"]["id"], opened.id.to_string());
    assert_eq!(body["notification"]["key"], "decision:d-1");
    assert_eq!(body["notification"]["level"], "needs_you");

    let mut mac = Hmac::<Sha256>::new_from_slice(b"shared-secret").unwrap();
    mac.update(&request.body);
    assert_eq!(
        request
            .headers
            .get("x-tack-signature")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    );
}

#[tokio::test]
async fn a_delivery_queued_for_a_channel_no_longer_configured_is_given_up() {
    let mut f = fixture(200).await;
    open_question(&f).await;
    f.channels.clear();

    assert_eq!(pass(&f).await, 1);

    assert!(f.server.received_requests().await.unwrap().is_empty());
    let status = &f.repo.channel_statuses().await.unwrap()[0];
    assert_eq!((status.pending, status.failed), (0, 1));
    assert!(
        status
            .last_error
            .as_deref()
            .unwrap()
            .contains("not configured")
    );
}

#[tokio::test]
async fn opening_wakes_the_loop_without_waiting_for_its_tick() {
    let f = fixture(200).await;
    let mut loops = Loops::new();
    let real: Arc<dyn Clock> = Arc::new(SystemClock);
    // The inbox and the loop share the real clock, so the delivery is due the moment it is queued.
    let inbox = Inbox::new(f.repo.clone(), real.clone(), vec!["webhook".into()]);
    delivery::spawn(
        &mut loops,
        f.repo.clone(),
        real,
        f.channels.clone(),
        inbox.queued(),
    );
    // Let the loop's first pass go by, long before anything is queued.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let workspace = create_test_workspace(&f.repo).await;
    let project = make_project(&f.repo, workspace).await.id;
    inbox
        .open(&Notice {
            kind: Kind::RunFailed,
            level: Level::NeedsYou,
            key: Key::decision("d-wake"),
            project_id: project,
            item_id: None,
            request_id: None,
            title: "A run failed".into(),
            cause: "runner:local".into(),
        })
        .await
        .unwrap();

    let waited = tokio::time::timeout(Duration::from_secs(5), async {
        while f.server.received_requests().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    loops.stop(STOP_GRACE).await;
    assert!(
        waited.is_ok(),
        "the webhook heard of it before the 10 s tick"
    );
}
