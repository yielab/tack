//! Captures what reached a `tracing` log line during one test, so a test can
//! assert that a secret did not.
//!
//! The subscriber is global, not scoped: `tracing` caches per-callsite
//! interest process-wide, so a callsite first evaluated with no subscriber
//! installed is cached as uninteresting and a later thread-local subscriber
//! never sees it — the assertion then passes on nothing. A thread-local
//! buffer keeps each test's output apart.

thread_local! {
    static CAPTURED: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct ThreadLocalCapture;

impl std::io::Write for ThreadLocalCapture {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        CAPTURED.with(|captured| captured.borrow_mut().extend_from_slice(buffer));
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for ThreadLocalCapture {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        ThreadLocalCapture
    }
}

/// Starts capturing on this thread, discarding anything captured before.
pub fn install() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        tracing_subscriber::fmt()
            .with_writer(ThreadLocalCapture)
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .init();
    });
    CAPTURED.with(|captured| captured.borrow_mut().clear());
}

pub fn captured() -> String {
    CAPTURED.with(|captured| String::from_utf8_lossy(&captured.borrow()).into_owned())
}
