//! docket's sign-in is read by Tack from docket itself, not asked of it here.

use std::{future::Future, pin::Pin};

use tack_orch::execution::{ModelListSource, ModelsReport, SignInReport, SignInState};

use crate::harness::local_process::{Readiness, ReadinessShell};

pub(super) fn check<'a>(
    _shell: &'a ReadinessShell,
) -> Pin<Box<dyn Future<Output = Readiness> + Send + 'a>> {
    Box::pin(async {
        Readiness {
            sign_in: SignInReport {
                state: SignInState::Unknown,
                detail: "Tack reads docket's sign-in from docket 0.2.0-beta.5".to_owned(),
                command: None,
            },
            models: ModelsReport {
                source: ModelListSource::None,
                ids: Vec::new(),
                reason: Some("docket lists no models".to_owned()),
            },
            default_model: None,
            requires_model: Some(true),
        }
    })
}
