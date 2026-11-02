//! A conversation between a person and a profile (ADR 0034, decision 7), as pure values and the
//! rules that bound it. A [`Thread`] belongs to a project or to a credential's projects and has
//! one profile; a [`Message`] has a principal, a body and references. Each message from a person
//! starts a turn (a message from an agent is posted and starts none, see [`opens_turn`]), and a turn reads the thread through a [`Window`]: the last messages whole, the
//! older ones as their first line, all within one size bound. Nothing here touches the store, a
//! clock or HTTP.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::principal::PrincipalKind;

/// The most turns one thread takes; a thread that reached it is closed and a new one is opened.
pub const MAX_TURNS: u32 = 50;

/// A thread with no message for this many days closes.
pub const IDLE_DAYS: i64 = 7;

/// The longest title and message body, in characters.
pub const TITLE_CHARS: usize = 120;
pub const BODY_CHARS: usize = 4_000;

/// How many of the latest messages a turn reads whole.
pub const WHOLE_MESSAGES: usize = 6;

/// How many older messages a turn reads as their first line; the rest are counted.
pub const OLDER_LINES: usize = 30;

/// The longest first line of an older message, in characters.
pub const FIRST_LINE_CHARS: usize = 160;

/// The most characters of messages one turn reads: whole messages, first lines and the notes.
pub const WINDOW_CHARS: usize = 12_000;

/// The most references one message carries.
pub const MAX_REFS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ThreadStatus {
    Open,
    Closed,
}

impl ThreadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadStatus::Open => "open",
            ThreadStatus::Closed => "closed",
        }
    }
}

impl FromStr for ThreadStatus {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "open" => Ok(ThreadStatus::Open),
            "closed" => Ok(ThreadStatus::Closed),
            _ => Err(()),
        }
    }
}

/// Which projects a thread reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ThreadScope {
    /// One project: its digest, and nothing else.
    Project { project: Uuid },
    /// The projects a credential reaches. `home` is the one project its turns run in.
    Credential { credential: Uuid, home: Uuid },
}

impl ThreadScope {
    /// The project a turn of the thread runs in.
    pub fn home(&self) -> Uuid {
        match self {
            ThreadScope::Project { project } => *project,
            ThreadScope::Credential { home, .. } => *home,
        }
    }

    /// The credential whose projects the thread reaches, when it is not one project's.
    pub fn credential(&self) -> Option<Uuid> {
        match self {
            ThreadScope::Project { .. } => None,
            ThreadScope::Credential { credential, .. } => Some(*credential),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Thread {
    pub id: Uuid,
    pub scope: ThreadScope,
    /// The agent profile whose runs are the thread's turns.
    pub profile: String,
    pub title: String,
    pub status: ThreadStatus,
    /// Turns taken so far.
    pub turns: u32,
    /// Why a closed thread closed.
    pub closed_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    /// The time of the latest message, or of the thread's creation when it has none.
    pub last_message_at: DateTime<Utc>,
}

/// What a message points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum RefKind {
    Item,
    ChangeSet,
    Decision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Ref {
    pub kind: RefKind,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Message {
    pub id: Uuid,
    pub thread: Uuid,
    /// Who wrote it, as [`Principal::label`](crate::principal::Principal::label) says:
    /// `operator:local`, `agent:<credential>`, `thread:<id>` for a reply.
    pub principal: String,
    pub body: String,
    pub refs: Vec<Ref>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TurnState {
    /// The run has not ended, or its reply has not been posted.
    Pending,
    /// The reply was posted.
    Replied,
    /// The run ended without a reply; the thread says why.
    Failed,
}

impl TurnState {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnState::Pending => "pending",
            TurnState::Replied => "replied",
            TurnState::Failed => "failed",
        }
    }
}

impl FromStr for TurnState {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "pending" => Ok(TurnState::Pending),
            "replied" => Ok(TurnState::Replied),
            "failed" => Ok(TurnState::Failed),
            _ => Err(()),
        }
    }
}

/// One run of the thread's profile, started by one message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Turn {
    pub thread: Uuid,
    /// 1 for the first turn.
    pub position: u32,
    /// The message that started it.
    pub message: Uuid,
    /// The run (execution request) it is.
    pub request_id: String,
    pub state: TurnState,
    pub started_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

/// The principal label a thread's turns run under, and the origin reference of the change sets
/// they make.
pub fn turn_principal(thread: Uuid) -> String {
    format!("thread:{thread}")
}

/// The thread a label names, when it is a thread's.
pub fn thread_of_label(label: &str) -> Option<Uuid> {
    Uuid::parse_str(label.strip_prefix("thread:")?).ok()
}

/// Why a title or a body cannot be accepted, or `None` when it can.
pub fn title_problem(title: &str) -> Option<String> {
    let length = title.trim().chars().count();
    (length == 0 || length > TITLE_CHARS)
        .then(|| format!("the title must be 1 to {TITLE_CHARS} characters"))
}

pub fn body_problem(body: &str) -> Option<String> {
    let length = body.trim().chars().count();
    (length == 0 || length > BODY_CHARS)
        .then(|| format!("a message must be 1 to {BODY_CHARS} characters"))
}

/// Why a message cannot carry `refs`, or `None` when it can.
pub fn refs_problem(refs: &[Ref]) -> Option<String> {
    if refs.len() > MAX_REFS {
        return Some(format!("a message carries at most {MAX_REFS} references"));
    }
    refs.iter()
        .any(|reference| reference.id.trim().is_empty())
        .then(|| "a reference names an id".to_owned())
}

/// Why a thread takes no more turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStop {
    /// The thread is closed.
    Closed,
    /// The thread took [`MAX_TURNS`] turns.
    TurnCap,
    /// The thread had no message for [`IDLE_DAYS`] days; it closes now.
    Idle,
}

impl fmt::Display for TurnStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TurnStop::Closed => f.write_str("the thread is closed; open a new one"),
            TurnStop::TurnCap => write!(
                f,
                "the thread took its {MAX_TURNS} turns; open a new one to go on"
            ),
            TurnStop::Idle => write!(
                f,
                "the thread had no message for {IDLE_DAYS} days and is closed; open a new one"
            ),
        }
    }
}

impl TurnStop {
    /// The reason a thread stopped by this rule is closed with.
    pub fn closing_reason(self) -> Option<&'static str> {
        match self {
            TurnStop::Closed => None,
            TurnStop::TurnCap => Some("turn cap"),
            TurnStop::Idle => Some("idle"),
        }
    }
}

/// True when the thread has been silent long enough to close.
pub fn is_idle(thread: &Thread, now: DateTime<Utc>) -> bool {
    thread.status == ThreadStatus::Open && now - thread.last_message_at >= Duration::days(IDLE_DAYS)
}

/// Whether the thread may take another turn at `now`.
pub fn may_take_turn(thread: &Thread, now: DateTime<Utc>) -> Result<(), TurnStop> {
    if thread.status == ThreadStatus::Closed {
        return Err(TurnStop::Closed);
    }
    if is_idle(thread, now) {
        return Err(TurnStop::Idle);
    }
    if thread.turns >= MAX_TURNS {
        return Err(TurnStop::TurnCap);
    }
    Ok(())
}

/// Whether a message written by `author` starts a turn. Only the operator's does: the turn is
/// the answer to a person, and a run that an agent could start by writing would let a profile
/// that writes to its own thread start a loop. An agent's message (a report, an alert) is posted
/// and waits for the person to read it.
pub fn opens_turn(author: PrincipalKind) -> bool {
    matches!(author, PrincipalKind::Operator)
}

/// Whether a message that [`opens_turn`] or not may be posted in the thread at `now`. A message
/// that starts a turn needs the thread to [`may_take_turn`]; one that starts none needs only an
/// open thread that is not idle, so it is not held back by a turn still pending or by the turn
/// cap, which count turns.
pub fn may_post(thread: &Thread, now: DateTime<Utc>, opens_turn: bool) -> Result<(), TurnStop> {
    match may_take_turn(thread, now) {
        Err(TurnStop::TurnCap) if !opens_turn => Ok(()),
        other => other,
    }
}

/// What a turn reads of a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window<'a> {
    /// Older messages that did not fit, not read at all.
    pub left_out: usize,
    /// Older messages, each as its first line.
    pub older: Vec<(&'a Message, String)>,
    /// The latest messages, whole, oldest first.
    pub whole: Vec<&'a Message>,
}

/// The first line of a body, shortened to [`FIRST_LINE_CHARS`] with an ellipsis.
pub fn first_line(body: &str) -> String {
    let line = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.chars().count() <= FIRST_LINE_CHARS {
        return line.to_owned();
    }
    let cut: String = line.chars().take(FIRST_LINE_CHARS - 1).collect();
    format!("{cut}…")
}

fn weight(message: &Message, whole: bool) -> usize {
    let text = if whole {
        message.body.chars().count()
    } else {
        first_line(&message.body).chars().count()
    };
    text + message.principal.chars().count() + HEADING_CHARS
}

/// What a message's heading and separators cost beyond its text.
const HEADING_CHARS: usize = 16;

/// The window a turn reads of `messages` (oldest first): the last [`WHOLE_MESSAGES`] whole, up to
/// [`OLDER_LINES`] older ones as their first line, and the whole bounded by [`WINDOW_CHARS`]. When
/// the bound is hit the newest messages win: the oldest whole message becomes a first line, and
/// the oldest lines are left out and counted. The newest message is always whole.
pub fn window(messages: &[Message]) -> Window<'_> {
    let split = messages.len().saturating_sub(WHOLE_MESSAGES);
    let mut whole: Vec<&Message> = messages[split..].iter().collect();
    let mut older: Vec<&Message> = messages[..split].iter().collect();
    let mut left_out = 0;
    if older.len() > OLDER_LINES {
        left_out = older.len() - OLDER_LINES;
        older.drain(..left_out);
    }
    let size = |whole: &[&Message], older: &[&Message]| {
        whole.iter().map(|m| weight(m, true)).sum::<usize>()
            + older.iter().map(|m| weight(m, false)).sum::<usize>()
    };
    // Whole messages give way first, oldest first, but the newest stays.
    while size(&whole, &older) > WINDOW_CHARS && whole.len() > 1 {
        older.push(whole.remove(0));
    }
    while size(&whole, &older) > WINDOW_CHARS && !older.is_empty() {
        older.remove(0);
        left_out += 1;
    }
    Window {
        left_out,
        older: older
            .into_iter()
            .map(|message| (message, first_line(&message.body)))
            .collect(),
        whole,
    }
}

/// The window as the text a turn is given. The same messages make the same bytes.
pub fn render_window(window: &Window<'_>) -> String {
    let mut text = String::new();
    if window.left_out > 0 || !window.older.is_empty() {
        text.push_str("Earlier in this thread, first lines only:\n");
        if window.left_out > 0 {
            text.push_str(&format!(
                "- ({} older messages are not shown)\n",
                window.left_out
            ));
        }
        for (message, line) in &window.older {
            text.push_str(&format!("- {}: {line}\n", message.principal));
        }
        text.push('\n');
    }
    text.push_str("Latest messages:\n");
    for message in &window.whole {
        text.push_str(&format!(
            "\n[{}]\n{}\n",
            message.principal,
            message.body.trim()
        ));
        if !message.refs.is_empty() {
            let refs: Vec<String> = message
                .refs
                .iter()
                .map(|reference| {
                    let kind = match reference.kind {
                        RefKind::Item => "item",
                        RefKind::ChangeSet => "change set",
                        RefKind::Decision => "decision",
                    };
                    format!("{kind} {}", reference.id)
                })
                .collect();
            text.push_str(&format!("(refers to: {})\n", refs.join(", ")));
        }
    }
    text
}

/// What a turn is told: the conversation, then the board's digest, then how to answer.
/// `reply_file` is where an agent that works in a folder leaves its reply.
pub fn turn_input(window: &Window<'_>, digest_json: &str, reply_file: &str) -> String {
    format!(
        "You are taking part in a conversation with a person about their board. Answer the \
         latest message. The board's digest below is computed, not written by a model; ask for \
         detail through your tools rather than guessing. Anything you want changed on the board \
         goes through a change set, which a person applies.\n\n{}\nBoard digest:\n{digest_json}\n\n\
         Reply: when you finish, your final message is your reply. If you work in a folder, also \
         write it to `{reply_file}` at the root of your work tree.\n",
        render_window(window)
    )
}

/// The text of a reply cut to [`BODY_CHARS`], with a note when it was cut.
pub fn fit_reply(reply: &str) -> String {
    let reply = reply.trim();
    if reply.chars().count() <= BODY_CHARS {
        return reply.to_owned();
    }
    let note = "\n\n(cut: the reply was longer than a message may be)";
    let kept: String = reply
        .chars()
        .take(BODY_CHARS - note.chars().count())
        .collect();
    format!("{kept}{note}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-01T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + Duration::days(day)
    }

    fn thread() -> Thread {
        Thread {
            id: Uuid::nil(),
            scope: ThreadScope::Project {
                project: Uuid::nil(),
            },
            profile: "p".into(),
            title: "t".into(),
            status: ThreadStatus::Open,
            turns: 0,
            closed_reason: None,
            created_at: at(0),
            last_message_at: at(0),
        }
    }

    fn messages(count: usize, body: &str) -> Vec<Message> {
        (0..count)
            .map(|n| Message {
                id: Uuid::from_u128(n as u128),
                thread: Uuid::nil(),
                principal: if n % 2 == 0 {
                    "operator:local"
                } else {
                    "thread:x"
                }
                .into(),
                body: format!("{body} {n}"),
                refs: Vec::new(),
                created_at: at(0),
            })
            .collect()
    }

    #[test]
    fn a_short_thread_is_read_whole() {
        let all = messages(3, "hello");
        let window = window(&all);
        assert_eq!(window.whole.len(), 3);
        assert!(window.older.is_empty());
        assert_eq!(window.left_out, 0);
    }

    #[test]
    fn older_messages_are_read_as_their_first_line() {
        let all = messages(10, "first line\nsecond line");
        let window = window(&all);
        assert_eq!(window.whole.len(), WHOLE_MESSAGES);
        assert_eq!(window.older.len(), 4);
        assert_eq!(window.older[0].1, "first line");
        let text = render_window(&window);
        assert!(text.contains("- operator:local: first line\n"));
        assert!(!text.contains("second line\n- "), "{text}");
    }

    #[test]
    fn a_long_thread_counts_what_it_leaves_out() {
        let all = messages(100, "x");
        let window = window(&all);
        assert_eq!(window.older.len(), OLDER_LINES);
        assert_eq!(window.left_out, 100 - WHOLE_MESSAGES - OLDER_LINES);
        assert!(render_window(&window).contains("older messages are not shown"));
    }

    #[test]
    fn the_size_bound_holds_and_the_newest_message_stays_whole() {
        let big = "y".repeat(BODY_CHARS);
        let all = messages(20, &big);
        let window = window(&all);
        assert!(render_window(&window).chars().count() <= WINDOW_CHARS + 2_000);
        assert_eq!(window.whole.last().unwrap().id, all[19].id);
        assert!(window.whole.len() < WHOLE_MESSAGES);
    }

    #[test]
    fn the_same_messages_make_the_same_bytes() {
        let all = messages(12, "same");
        assert_eq!(render_window(&window(&all)), render_window(&window(&all)));
        let input = turn_input(&window(&all), "{}", "tack-reply.md");
        assert_eq!(input, turn_input(&window(&all), "{}", "tack-reply.md"));
        assert!(input.contains("`tack-reply.md`"));
    }

    #[test]
    fn a_thread_stops_by_its_caps() {
        let mut open = thread();
        assert_eq!(may_take_turn(&open, at(1)), Ok(()));
        open.turns = MAX_TURNS;
        assert_eq!(may_take_turn(&open, at(1)), Err(TurnStop::TurnCap));
        open.turns = 0;
        assert_eq!(may_take_turn(&open, at(IDLE_DAYS - 1)), Ok(()));
        assert_eq!(may_take_turn(&open, at(IDLE_DAYS)), Err(TurnStop::Idle));
        open.status = ThreadStatus::Closed;
        assert_eq!(may_take_turn(&open, at(1)), Err(TurnStop::Closed));
        assert!(!is_idle(&open, at(30)), "a closed thread is not idle");
    }

    #[test]
    fn only_the_operators_message_opens_a_turn() {
        assert!(opens_turn(PrincipalKind::Operator));
        for kind in [
            PrincipalKind::Agent,
            PrincipalKind::Runner,
            PrincipalKind::System,
        ] {
            assert!(!opens_turn(kind), "{kind:?}");
        }
    }

    #[test]
    fn a_message_that_opens_no_turn_ignores_the_turn_cap_but_not_a_closed_thread() {
        let mut full = thread();
        full.turns = MAX_TURNS;
        assert_eq!(may_post(&full, at(1), true), Err(TurnStop::TurnCap));
        assert_eq!(may_post(&full, at(1), false), Ok(()));
        assert_eq!(may_post(&full, at(IDLE_DAYS), false), Err(TurnStop::Idle));
        full.status = ThreadStatus::Closed;
        assert_eq!(may_post(&full, at(1), false), Err(TurnStop::Closed));
    }

    #[test]
    fn labels_name_the_thread_they_come_from() {
        let id = Uuid::new_v4();
        assert_eq!(thread_of_label(&turn_principal(id)), Some(id));
        assert_eq!(thread_of_label("agent:x"), None);
    }

    #[test]
    fn titles_bodies_and_references_are_bounded() {
        assert!(title_problem("  ").is_some());
        assert!(title_problem("What blocks the launch?").is_none());
        assert!(body_problem(&"a".repeat(BODY_CHARS + 1)).is_some());
        assert!(body_problem("ok").is_none());
        let reference = Ref {
            kind: RefKind::ChangeSet,
            id: "1".into(),
        };
        assert!(refs_problem(&vec![reference.clone(); MAX_REFS + 1]).is_some());
        assert!(refs_problem(&[reference]).is_none());
    }

    #[test]
    fn a_long_reply_is_cut_with_a_note() {
        let fitted = fit_reply(&"z".repeat(BODY_CHARS * 2));
        assert_eq!(fitted.chars().count(), BODY_CHARS);
        assert!(fitted.ends_with("may be)"));
        assert_eq!(fit_reply("  short  "), "short");
    }
}
