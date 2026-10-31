//! Protocol ids as `&'static` values, for tests that build store inputs which borrow their ids.
//!
//! A store input such as `Completion<'a>` holds `&'a AttemptId`, so a helper that returns one
//! cannot build the id inside itself. Each function here interns the id once per distinct
//! value and hands out a reference that lives for the whole test process; the handful of
//! distinct ids a test run uses makes the leak irrelevant.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tack_protocol::execution::{
    AttemptId, CancellationRequestId, Checkpoint, ClaimRequestId, CompletionId, ExecutionRequestId,
    RecoveryKey, RunnerId, WorkspaceId,
};

macro_rules! interned {
    ($name:ident, $ty:ident) => {
        #[doc = concat!("The `", stringify!($ty), "` with this value, interned for the test process.")]
        pub fn $name(value: &str) -> &'static $ty {
            static POOL: OnceLock<Mutex<HashMap<String, &'static $ty>>> = OnceLock::new();
            let mut pool = POOL
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(found) = pool.get(value) {
                return found;
            }
            let leaked: &'static $ty = Box::leak(Box::new($ty::new(value)));
            pool.insert(value.to_owned(), leaked);
            leaked
        }
    };
}

interned!(attempt, AttemptId);
interned!(runner, RunnerId);
interned!(workspace, WorkspaceId);
interned!(claim_request, ClaimRequestId);
interned!(completion, CompletionId);
interned!(cancellation_request, CancellationRequestId);
interned!(recovery_key, RecoveryKey);
interned!(checkpoint, Checkpoint);
interned!(execution_request, ExecutionRequestId);
