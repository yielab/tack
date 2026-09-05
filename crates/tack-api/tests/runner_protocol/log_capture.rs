//! Shared process-global `tracing` capture guard backing every
//! redaction test in this binary (`handlers`, `decisions`,
//! `artifact_events`).
//!
//! A bare `tracing::subscriber::set_default` thread-local override was
//! tried first and is flaky under parallel execution (~1 in 10):
//! `tracing`'s callsite-interest cache is process-global, not
//! per-subscriber. The first time any thread in this binary hits a given
//! `event!` callsite (e.g. the `tracing::info!(runner_id = ..., "runner
//! enrolled")` every `setup()` in this binary's tests can reach), the
//! `Interest` returned by whatever subscriber is active *on that thread at
//! that exact moment* is cached forever for that callsite — a thread-local
//! override active on a *different* thread is never consulted. Most tests
//! in this binary install no subscriber of their own, so if one of them is
//! the first to touch a shared callsite (a near coin-flip under `cargo
//! test`'s parallel-by-default execution), the interest gets cached as
//! `never` against the ambient no-op dispatcher, and a capturing test's own
//! `set_default` guard can no longer make that callsite fire for it — its
//! capture buffer stays silently empty.
//!
//! The fix: install a single, permanent, process-global default subscriber,
//! idempotently, before any test's first HTTP request. Once a global
//! default exists, `tracing` never falls back to the no-op dispatcher again
//! on any thread, so no sibling test can poison a callsite's cached
//! interest; every first-touch, from any thread, resolves against this real
//! subscriber. Per-test isolation is then just a thread-local flag
//! (`LOG_CAPTURE`) the global writer consults to decide whether to keep
//! what it's given — safe here specifically because every `#[tokio::test]`
//! in this binary runs its whole async body on one dedicated OS thread (the
//! default current-thread runtime flavor), so the flag never needs to
//! survive a cross-thread hop mid-`.await`. The subscriber itself is still
//! real `tracing_subscriber::fmt` formatting real output from the
//! production handlers — nothing here is mocked or hand-constructed.
//!
//! This module used to be three separate copies, one per file, because each
//! was its own compiled test binary and a `tracing` global default is
//! process-wide — two binaries can't share one guard. Now that
//! `lifecycle`, `decisions` and `artifact_events` are modules of the same
//! binary, one copy covers all three, and more of this binary's tests share
//! the one process it protects. Under nextest, where each test is its own
//! process, the race this guard exists to close cannot occur at all — but
//! under `cargo test`, which runs a whole binary's tests in one process,
//! the guard is still doing real work.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, Once};

thread_local! {
    /// Set only by `CaptureGuard::start`, and only on the calling test's own
    /// thread; `None` on every other thread in the binary, so the shared
    /// global writer below silently discards everything it's given for
    /// tests that never call it.
    static LOG_CAPTURE: RefCell<Option<Arc<Mutex<Vec<u8>>>>> = const { RefCell::new(None) };
}

static GLOBAL_LOG_CAPTURE_INIT: Once = Once::new();

/// Installs the one `tracing` subscriber this test binary ever installs, as
/// the process-wide global default — exactly once, idempotently. See this
/// module's own doc comment for why a global default (rather than a
/// thread-local `set_default`) is what actually closes the race, not just
/// narrows it.
pub(crate) fn ensure_global_log_capture_installed() {
    GLOBAL_LOG_CAPTURE_INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(GlobalLogWriter)
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect(
            "GLOBAL_LOG_CAPTURE_INIT guards the only global tracing subscriber \
             this binary ever installs",
        );
    });
}

/// Zero-sized `MakeWriter` for the global subscriber. Every event it's given
/// is routed through the calling thread's `LOG_CAPTURE` slot, so writes from
/// tests that never call `CaptureGuard::start` go nowhere.
struct GlobalLogWriter;

impl std::io::Write for GlobalLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        LOG_CAPTURE.with(|cell| {
            if let Some(buffer) = cell.borrow().as_ref() {
                buffer.lock().unwrap().extend_from_slice(buf);
            }
        });
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for GlobalLogWriter {
    type Writer = GlobalLogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        GlobalLogWriter
    }
}

/// RAII scope: while alive, real `tracing` output from this thread is
/// collected into the returned buffer instead of being discarded.
pub(crate) struct CaptureGuard;

impl CaptureGuard {
    pub(crate) fn start() -> (Self, Arc<Mutex<Vec<u8>>>) {
        ensure_global_log_capture_installed();
        let buffer = Arc::new(Mutex::new(Vec::new()));
        LOG_CAPTURE.with(|cell| *cell.borrow_mut() = Some(buffer.clone()));
        (Self, buffer)
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        LOG_CAPTURE.with(|cell| *cell.borrow_mut() = None);
    }
}
