//! Security and adversarial coverage for `tack-api`: the operator
//! bearer-token boundary against path-lookalike and WebSocket-handshake
//! bypass attempts, CORS preflight behavior, the chaos/fencing/recovery
//! adversarial suite driven against the real production router, and the two
//! WIP-limit races — an ordinary board-drag PATCH and the sprint-dispatch
//! path — proven under genuine concurrent load.

mod common;

#[path = "security/board_drag_wip_race.rs"]
mod board_drag_wip_race;
#[path = "security/chaos_recovery.rs"]
mod chaos_recovery;
#[path = "security/cors.rs"]
mod cors;
#[path = "security/trust_boundary.rs"]
mod trust_boundary;
#[path = "security/wip_limit_race.rs"]
mod wip_limit_race;
