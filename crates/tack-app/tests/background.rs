//! The loops of one server: a pass on its interval, a stop that waits, a panic that is logged
//! with the loop's name.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tack_app::background::{Loops, Start};
use tack_test_support::poll_until;

use crate::logs::capture;

const SECONDS: Duration = Duration::from_secs(5);

fn counted() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let passes = Arc::new(AtomicUsize::new(0));
    (passes.clone(), passes)
}

#[tokio::test]
async fn a_loop_passes_on_its_interval_until_it_is_stopped() {
    let (seen, passes) = counted();
    let mut loops = Loops::new();
    loops.spawn_tick(
        "counting",
        Start::Immediately,
        Duration::from_millis(10),
        move || {
            let passes = passes.clone();
            async move {
                passes.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    let ran = poll_until(SECONDS, async || {
        (seen.load(Ordering::SeqCst) >= 3).then_some(())
    })
    .await;
    loops.stop(SECONDS).await;
    assert!(ran.is_some(), "three passes within the budget");

    let after_stop = seen.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        seen.load(Ordering::SeqCst),
        after_stop,
        "no pass starts once the loops are stopped"
    );
}

#[tokio::test]
async fn a_loop_that_starts_after_the_interval_does_not_pass_at_once() {
    let (seen, passes) = counted();
    let mut loops = Loops::new();
    loops.spawn_tick(
        "late",
        Start::AfterInterval,
        Duration::from_secs(3600),
        move || {
            let passes = passes.clone();
            async move {
                passes.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    loops.stop(SECONDS).await;

    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stopping_lets_the_pass_in_progress_finish() {
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let (began, ended) = (started.clone(), finished.clone());
    let mut loops = Loops::new();
    loops.spawn_tick(
        "slow",
        Start::Immediately,
        Duration::from_millis(10),
        move || {
            let (began, ended) = (began.clone(), ended.clone());
            async move {
                began.store(true, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(200)).await;
                ended.store(true, Ordering::SeqCst);
            }
        },
    );
    poll_until(SECONDS, async || {
        started.load(Ordering::SeqCst).then_some(())
    })
    .await
    .expect("the first pass began");

    loops.stop(SECONDS).await;

    assert!(
        finished.load(Ordering::SeqCst),
        "stop returned only after the pass ended"
    );
}

#[tokio::test]
async fn stopping_gives_up_on_a_pass_that_never_returns() {
    let (logged, _guard) = capture();
    let started = Arc::new(AtomicBool::new(false));
    let began = started.clone();
    let mut loops = Loops::new();
    loops.spawn_tick(
        "stuck",
        Start::Immediately,
        Duration::from_millis(10),
        move || {
            let began = began.clone();
            async move {
                began.store(true, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
        },
    );
    poll_until(SECONDS, async || {
        started.load(Ordering::SeqCst).then_some(())
    })
    .await
    .expect("the first pass began");

    loops.stop(Duration::from_millis(100)).await;

    assert_eq!(logged.count("background loops did not stop in time"), 1);
}

#[tokio::test]
async fn a_panic_is_logged_with_the_loops_name_and_ends_only_that_loop() {
    let (logged, _guard) = capture();
    let (seen, passes) = counted();
    let mut loops = Loops::new();
    loops.spawn_tick(
        "explodes",
        Start::Immediately,
        Duration::from_millis(10),
        || async { panic!("the pass broke") },
    );
    loops.spawn_tick(
        "carries_on",
        Start::Immediately,
        Duration::from_millis(10),
        move || {
            let passes = passes.clone();
            async move {
                passes.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    let logged_it = poll_until(SECONDS, async || {
        (logged.count("background loop panicked") == 1).then_some(())
    })
    .await;
    let before = seen.load(Ordering::SeqCst);
    let went_on = poll_until(SECONDS, async || {
        (seen.load(Ordering::SeqCst) > before).then_some(())
    })
    .await;
    loops.stop(SECONDS).await;

    assert!(logged_it.is_some(), "the panic was logged");
    assert_eq!(logged.count("name=\"explodes\""), 1);
    assert_eq!(logged.count("the pass broke"), 1);
    assert!(went_on.is_some(), "the other loop kept passing");
}
