use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};

use crate::router::AppState;

fn is_public_route(path: &str) -> bool {
    matches!(
        path,
        "/api/health" | "/api/openapi.json" | "/api/alexa" | "/health" | "/openapi.json" | "/alexa"
    )
}

/// Browser WebSockets cannot attach `Authorization`. The board handler performs
/// the equivalent token check during the upgrade using a dedicated subprotocol;
/// this narrow route recognizer prevents a suffix lookalike from bypassing auth.
fn is_board_websocket_route(path: &str) -> bool {
    let Some(project) = path
        .strip_prefix("/api/projects/")
        .or_else(|| path.strip_prefix("/projects/"))
        .and_then(|rest| rest.strip_suffix("/boards/live"))
    else {
        return false;
    };
    !project.contains('/') && uuid::Uuid::parse_str(project).is_ok()
}

const AUTH_PROTOCOL_PREFIX: &str = "tack.auth.";

fn board_websocket_is_authorized(req: &Request, state: &AppState) -> bool {
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        if !state
            .config
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin)
        {
            return false;
        }
    }

    let Some(expected) = state.config.api_token.as_deref() else {
        return true;
    };
    let provided = req
        .headers()
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(',').map(str::trim))
        .find_map(|protocol| protocol.strip_prefix(AUTH_PROTOCOL_PREFIX))
        .and_then(decode_base64url);
    provided
        .as_deref()
        .is_some_and(|token| constant_time_eq(token, expected.as_bytes()))
}

/// Decode the unpadded base64url form permitted by a WebSocket subprotocol.
/// `=` padding is intentionally rejected because browser WebSocket APIs only
/// permit HTTP-token characters in a subprotocol name.
fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let six_bits = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(six_bits);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    if bits == 6 { None } else { Some(output) }
}

/// Middleware: if `TACK_API_TOKEN` is configured, require a matching
/// `Authorization: Bearer <token>` header on every request except `/api/health`.
///
/// Uses a constant-time comparison to prevent timing-oracle attacks.
pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    if is_public_route(path) {
        return Ok(next.run(req).await);
    }

    if is_board_websocket_route(path) {
        return if board_websocket_is_authorized(&req, &state) {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        };
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
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
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
    use super::{constant_time_eq, decode_base64url, is_board_websocket_route, is_public_route};

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

    #[test]
    fn only_exact_public_routes_bypass_bearer_auth() {
        assert!(is_public_route("/api/health"));
        assert!(!is_public_route("/api/projects/health"));
        assert!(!is_public_route("/api/projects/alexa"));
    }

    #[test]
    fn only_the_exact_board_websocket_shape_defers_to_upgrade_auth() {
        assert!(is_board_websocket_route(
            "/api/projects/00000000-0000-0000-0000-000000000000/boards/live"
        ));
        assert!(!is_board_websocket_route(
            "/api/projects/not-a-uuid/boards/live"
        ));
        assert!(!is_board_websocket_route(
            "/api/projects/health/boards/live"
        ));
        assert!(!is_board_websocket_route(
            "/api/projects/00000000-0000-0000-0000-000000000000/boards/live-evil"
        ));
    }

    #[test]
    fn websocket_subprotocol_credentials_decode_without_query_parameters() {
        assert_eq!(
            decode_base64url("b3BlcmF0b3ItdG9rZW4"),
            Some(b"operator-token".to_vec())
        );
        assert_eq!(decode_base64url("not=valid"), None);
    }
}
