use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

use crate::router::AppState;

/// Middleware: if `FLEXPM_API_TOKEN` is configured, require a matching
/// `Authorization: Bearer <token>` header on every request except `/api/health`.
///
/// Uses a constant-time comparison to prevent timing-oracle attacks.
pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Health check is always public regardless of token configuration.
    if req.uri().path().ends_with("/health") {
        return Ok(next.run(req).await);
    }

    let Some(ref expected) = state.config.api_token else {
        // No token configured: pure-local mode, allow everything.
        return Ok(next.run(req).await);
    };

    let provided = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(tok) if constant_time_eq(tok.as_bytes(), expected.as_bytes()) => {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Byte-wise constant-time equality. Processes all bytes without early exit
/// so the comparison time does not reveal how many bytes matched.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // std::hint::black_box prevents the compiler from optimising away the fold.
    std::hint::black_box(
        a.iter()
            .zip(b.iter())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0,
    )
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn equal_bytes_match() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn different_bytes_no_match() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn different_lengths_no_match() {
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn empty_bytes_match() {
        assert!(constant_time_eq(b"", b""));
    }
}
