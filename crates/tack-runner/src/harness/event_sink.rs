//! Bounded, backpressured, redacted harness event streaming.
//!
//! `process.rs` bounds raw stdout/stderr *bytes*; this module bounds the
//! *structured* event stream an adapter derives from a harness's output
//! (one JSON object per line, a parsed tool-call, a progress update — the
//! `docs/contracts/runner-v1/event-batch.request.json` `events[]` shape).
//! Wiring that shape onto the wire (`PullProtocol` has no event-batch method
//! yet — a known C3 limitation, see its handoff) is future work; this module
//! is the local, always-available half: what an adapter's `wait()`
//! implementation accumulates before it is ever turned into a wire payload
//! or a log line.
//!
//! Two independent bounds apply, matching the two different ways "memory
//! bounded" can fail:
//!
//! - **Per-payload size** ([`EventSinkLimits::max_payload_bytes`], aligned
//!   with `limits.json`'s `event_payload_bytes_max`): one oversized event
//!   cannot blow the bound by itself. A payload over the cap is replaced
//!   with an explicit truncation marker (never a silently shortened value —
//!   rule 7), tracked in [`EventSinkReport::payloads_truncated`].
//! - **Backpressure** ([`EventSinkLimits::channel_capacity`]): events are
//!   delivered over a bounded `tokio::sync::mpsc` channel. Once it is full,
//!   [`EventSink::push`] genuinely waits for the consumer rather than
//!   growing an internal buffer — see `push_backpressure_blocks_the_producer`
//!   below for a test that proves this is real backpressure, not merely a
//!   size cap on individual sends.
//!
//! A third, harder bound — [`EventSinkLimits::max_events`] — exists because
//! backpressure alone only bounds the *instantaneous* buffer, not the total
//! number of events a run could ever produce; a sink with nobody consuming
//! it would otherwise block the producer forever rather than give a
//! deterministic, testable outcome. Once the lifetime cap is reached, further
//! events are counted in [`EventSinkReport::dropped_after_limit`] and never
//! buffered at all.

use tokio::sync::mpsc;

use super::redact::SecretMaterial;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSource {
    Harness,
    Runner,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HarnessEvent {
    pub sequence: u64,
    pub source: EventSource,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub kind: String,
    pub payload: serde_json::Value,
    /// True when `payload` was replaced with a truncation marker because it
    /// exceeded `max_payload_bytes`.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSinkLimits {
    pub channel_capacity: usize,
    pub max_payload_bytes: usize,
    pub max_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventSinkReport {
    pub emitted: u64,
    pub payloads_truncated: u64,
    pub dropped_after_limit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Accepted,
    /// The lifetime `max_events` cap was already reached; nothing was
    /// buffered or sent for this call.
    DroppedAfterLimit,
    /// The receiver has been dropped; nothing more can ever be delivered.
    /// Distinct from `DroppedAfterLimit` so a caller can tell "the sink is
    /// full" apart from "nobody is listening anymore".
    ReceiverClosed,
}

/// The producer half. Construction always returns the paired
/// [`mpsc::Receiver`] so a sink can never exist detached from something that
/// can eventually apply backpressure.
pub struct EventSink<C> {
    sender: mpsc::Sender<HarnessEvent>,
    limits: EventSinkLimits,
    secrets: SecretMaterial,
    clock: C,
    next_sequence: u64,
    report: EventSinkReport,
}

impl<C> EventSink<C>
where
    C: crate::Clock,
{
    pub fn new(
        limits: EventSinkLimits,
        secrets: SecretMaterial,
        clock: C,
    ) -> (Self, mpsc::Receiver<HarnessEvent>) {
        let (sender, receiver) = mpsc::channel(limits.channel_capacity.max(1));
        (
            Self {
                sender,
                limits,
                secrets,
                clock,
                next_sequence: 0,
                report: EventSinkReport::default(),
            },
            receiver,
        )
    }

    /// Redacts `payload` (recursively, every string leaf), caps its
    /// serialized size, assigns the next monotonic sequence number and an
    /// `occurred_at` timestamp from the injected clock, then sends it.
    /// Awaiting this call is exactly where backpressure is felt: it only
    /// resolves once the bounded channel has room.
    pub async fn push(
        &mut self,
        source: EventSource,
        kind: impl Into<String>,
        mut payload: serde_json::Value,
    ) -> PushOutcome {
        if self.report.emitted >= self.limits.max_events {
            self.report.dropped_after_limit += 1;
            return PushOutcome::DroppedAfterLimit;
        }

        self.secrets.scrub_json(&mut payload);
        let (payload, truncated) = cap_payload(payload, self.limits.max_payload_bytes);
        if truncated {
            self.report.payloads_truncated += 1;
        }

        let event = HarnessEvent {
            sequence: self.next_sequence,
            source,
            occurred_at: chrono::DateTime::<chrono::Utc>::from(self.clock.now()),
            kind: kind.into(),
            payload,
            truncated,
        };
        self.next_sequence += 1;

        if self.sender.send(event).await.is_err() {
            return PushOutcome::ReceiverClosed;
        }
        self.report.emitted += 1;
        PushOutcome::Accepted
    }

    pub fn report(&self) -> EventSinkReport {
        self.report
    }
}

fn cap_payload(payload: serde_json::Value, max_bytes: usize) -> (serde_json::Value, bool) {
    let serialized = serde_json::to_string(&payload).unwrap_or_default();
    if serialized.len() <= max_bytes {
        return (payload, false);
    }
    let cut = serialized
        .as_bytes()
        .get(..max_bytes)
        .unwrap_or(serialized.as_bytes());
    let prefix = String::from_utf8_lossy(cut).into_owned();
    (
        serde_json::json!({
            "truncated": true,
            "original_bytes": serialized.len(),
            "text_prefix": prefix,
        }),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[derive(Clone, Copy)]
    struct FixedClock(SystemTime);

    impl crate::Clock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    fn clock() -> FixedClock {
        FixedClock(SystemTime::UNIX_EPOCH + Duration::from_secs(1_754_000_000))
    }

    fn generous_limits() -> EventSinkLimits {
        EventSinkLimits {
            channel_capacity: 16,
            max_payload_bytes: 65_536,
            max_events: 10_000,
        }
    }

    #[tokio::test]
    async fn accepted_events_are_sequenced_and_carry_the_injected_clock_time() {
        let (mut sink, mut receiver) =
            EventSink::new(generous_limits(), SecretMaterial::new(), clock());

        assert_eq!(
            sink.push(
                EventSource::Harness,
                "message",
                serde_json::json!({"text": "hi"})
            )
            .await,
            PushOutcome::Accepted
        );
        assert_eq!(
            sink.push(
                EventSource::Runner,
                "progress",
                serde_json::json!({"percent": 50})
            )
            .await,
            PushOutcome::Accepted
        );

        let first = receiver.recv().await.expect("first event");
        let second = receiver.recv().await.expect("second event");
        assert_eq!(first.sequence, 0);
        assert_eq!(second.sequence, 1);
        assert_eq!(
            first.occurred_at,
            chrono::DateTime::<chrono::Utc>::from(clock().0)
        );
        assert_eq!(sink.report().emitted, 2);
    }

    /// Acceptance: truncation is explicit. An oversized payload is replaced
    /// with a typed marker (never a silently shortened value), and the
    /// report counts it.
    #[tokio::test]
    async fn oversized_payloads_are_explicitly_truncated_not_silently_shortened() {
        let limits = EventSinkLimits {
            channel_capacity: 4,
            max_payload_bytes: 32,
            max_events: 100,
        };
        let (mut sink, mut receiver) = EventSink::new(limits, SecretMaterial::new(), clock());
        let large_text = "x".repeat(1000);

        let outcome = sink
            .push(
                EventSource::Harness,
                "message",
                serde_json::json!({"text": large_text}),
            )
            .await;
        assert_eq!(outcome, PushOutcome::Accepted);

        let event = receiver.recv().await.expect("event");
        assert!(event.truncated);
        assert_eq!(event.payload["truncated"], true);
        assert!(event.payload["original_bytes"].as_u64().unwrap() > 32);
        assert!(!event.payload.to_string().contains(&large_text));
        assert_eq!(sink.report().payloads_truncated, 1);
    }

    /// Acceptance: high-volume output stays memory-bounded, for the
    /// structured-event path. Once `max_events` is reached, further pushes
    /// are counted as dropped and never buffered or sent — proving the
    /// total lifetime footprint is bounded independent of how long a run
    /// keeps producing events or whether anything ever drains them.
    #[tokio::test]
    async fn events_beyond_the_lifetime_cap_are_dropped_and_counted_not_buffered() {
        let limits = EventSinkLimits {
            channel_capacity: 100,
            max_payload_bytes: 1024,
            max_events: 3,
        };
        let (mut sink, mut receiver) = EventSink::new(limits, SecretMaterial::new(), clock());

        for index in 0..5 {
            sink.push(
                EventSource::Runner,
                "tick",
                serde_json::json!({"index": index}),
            )
            .await;
        }
        let report = sink.report();
        assert_eq!(report.emitted, 3);
        assert_eq!(report.dropped_after_limit, 2);

        // Dropping the sink closes the channel's sending half, so the
        // `recv()` loop below terminates once the buffered events are drained.
        drop(sink);
        let mut received = Vec::new();
        while let Some(event) = receiver.recv().await {
            received.push(event);
        }
        assert_eq!(received.len(), 3, "only the accepted events were ever sent");
    }

    /// Acceptance: bounded ... event streaming *with backpressure* — proven
    /// as a real block, not merely inferred from a size cap. With channel
    /// capacity 1, a second push while the first event is still unconsumed
    /// must not resolve until the consumer drains it.
    #[tokio::test]
    async fn push_backpressure_blocks_the_producer_until_the_consumer_drains() {
        let limits = EventSinkLimits {
            channel_capacity: 1,
            max_payload_bytes: 1024,
            max_events: 1000,
        };
        let (mut sink, mut receiver) = EventSink::new(limits, SecretMaterial::new(), clock());
        assert_eq!(
            sink.push(EventSource::Runner, "first", serde_json::json!({}))
                .await,
            PushOutcome::Accepted
        );

        let mut blocked = tokio::spawn(async move {
            sink.push(EventSource::Runner, "second", serde_json::json!({}))
                .await;
            sink
        });

        let too_soon = tokio::time::timeout(Duration::from_millis(100), &mut blocked).await;
        assert!(
            too_soon.is_err(),
            "push must genuinely block while the bounded channel is full"
        );

        let _ = receiver.recv().await.expect("drain the first event");
        let sink = tokio::time::timeout(Duration::from_secs(2), blocked)
            .await
            .expect("push completes once the consumer has room")
            .expect("producer task must not panic");
        assert_eq!(sink.report().emitted, 2);
    }

    /// Acceptance: secret canaries are absent from events. A canary is
    /// embedded (nested, inside an array) in the pushed payload; it must not
    /// survive into the delivered event.
    #[tokio::test]
    async fn secret_canaries_are_scrubbed_from_nested_event_payloads() {
        const CANARY: &str = "tack-test-event-canary-4f2a";
        let mut secrets = SecretMaterial::new();
        secrets.register(CANARY);
        let (mut sink, mut receiver) = EventSink::new(generous_limits(), secrets, clock());

        sink.push(
            EventSource::Harness,
            "message",
            serde_json::json!({"outer": {"inner": [format!("leaked {CANARY} value")]}}),
        )
        .await;

        let event = receiver.recv().await.expect("event");
        assert!(!event.payload.to_string().contains(CANARY));
        assert!(event.payload.to_string().contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn receiver_closed_is_reported_distinctly_from_the_event_limit() {
        let (mut sink, receiver) =
            EventSink::new(generous_limits(), SecretMaterial::new(), clock());
        drop(receiver);

        let outcome = sink
            .push(EventSource::Runner, "orphaned", serde_json::json!({}))
            .await;
        assert_eq!(outcome, PushOutcome::ReceiverClosed);
        assert_eq!(sink.report().dropped_after_limit, 0);
    }
}
