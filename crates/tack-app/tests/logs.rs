//! The log lines a test reads: a subscriber that keeps what is logged on the test's thread (a
//! `#[tokio::test]` runs its spawned tasks there too) as text.

use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing::Level;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
pub struct Logged(Arc<Mutex<Vec<u8>>>);

impl Logged {
    /// How many times `needle` appears in what was logged so far.
    pub fn count(&self, needle: &str) -> usize {
        let text = String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned();
        text.matches(needle).count()
    }
}

impl Write for Logged {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl MakeWriter<'_> for Logged {
    type Writer = Logged;

    fn make_writer(&self) -> Logged {
        self.clone()
    }
}

/// Starts keeping the lines logged at `debug` and above until the guard drops.
pub fn capture() -> (Logged, DefaultGuard) {
    let logged = Logged::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logged.clone())
        .with_max_level(Level::DEBUG)
        .with_ansi(false)
        .finish();
    (logged, tracing::subscriber::set_default(subscriber))
}
