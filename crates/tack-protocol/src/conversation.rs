//! What a thread's turn and the harness running it say to each other. A turn is an ordinary run;
//! the board marks it with a [`TurnNote`] in the request's metadata, and the run answers with
//! a reply: the agent's final message, staged as the `reply` artifact.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::execution::{CapabilitySupport, CapabilityValue};

/// The key of the request's `metadata` that marks a run as a thread's turn.
pub const TURN_KEY: &str = "thread";

/// Where an agent that works in a folder leaves its reply, at the root of its work tree.
pub const REPLY_FILE: &str = "tack-reply.md";

/// The artifact kind the reply is staged as, and its media type.
pub const REPLY_KIND: &str = "reply";
pub const REPLY_MEDIA_TYPE: &str = "text/markdown";

/// The key of a run's `terminal_reason` in which a harness hands over the agent's final
/// message when it holds it.
pub const REPLY_REASON_KEY: &str = "reply";

/// A run is one turn of a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnNote {
    /// The thread's id.
    pub id: String,
    /// 1 for the first turn.
    pub turn: u32,
    /// The most turns the thread takes.
    pub max_turns: u32,
    /// The message that started the turn, without the rest of the conversation: all an agent
    /// that keeps its own context needs.
    pub message: String,
}

impl TurnNote {
    pub fn into_metadata(self) -> Value {
        json!(self)
    }

    /// The note a request's metadata carries, when it carries one.
    pub fn of(metadata: &Value) -> Option<TurnNote> {
        serde_json::from_value(metadata.get(TURN_KEY)?.clone()).ok()
    }
}

/// How a harness keeps a conversation going between turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConverseMode {
    /// The agent keeps the context of the previous turns; a turn sends it only the new message.
    Continues,
    /// Every turn is a new run given the transcript. Every agent can.
    Replays,
}

impl ConverseMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            ConverseMode::Continues => "continues",
            ConverseMode::Replays => "replays",
        }
    }

    /// The capability a harness reports: supported, saying which way it converses and why.
    pub fn capability(self, reason: &str) -> CapabilityValue {
        let mut value = CapabilityValue {
            support: CapabilitySupport::Supported,
            reason: Some(reason.to_owned()),
            additional: Default::default(),
        };
        value
            .additional
            .insert("mode".to_owned(), json!(self.as_str()));
        value
    }

    /// The way a reported capability converses; `None` when it is not supported or names none.
    pub fn of(capability: &CapabilityValue) -> Option<ConverseMode> {
        if capability.support != CapabilitySupport::Supported {
            return None;
        }
        match capability.additional.get("mode")?.as_str()? {
            "continues" => Some(ConverseMode::Continues),
            "replays" => Some(ConverseMode::Replays),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_note_round_trips_through_metadata() {
        let note = TurnNote {
            id: "t".into(),
            turn: 2,
            max_turns: 50,
            message: "and now?".into(),
        };
        let metadata = json!({TURN_KEY: note.clone().into_metadata(), "other": 1});
        assert_eq!(TurnNote::of(&metadata), Some(note));
        assert_eq!(TurnNote::of(&json!({})), None);
    }

    #[test]
    fn a_capability_names_how_the_harness_converses() {
        let capability = ConverseMode::Continues.capability("keeps its task");
        assert_eq!(ConverseMode::of(&capability), Some(ConverseMode::Continues));
        let mut unsupported = ConverseMode::Replays.capability("x");
        unsupported.support = CapabilitySupport::Unsupported;
        assert_eq!(ConverseMode::of(&unsupported), None);
    }
}
