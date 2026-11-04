use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use tack_core::principal::Principal;

use crate::error::ApiError;
use crate::router::AppState;

fn is_public_route(path: &str) -> bool {
    matches!(
        path,
        "/api/health" | "/api/openapi.json" | "/health" | "/openapi.json"
    )
}

/// Browser WebSockets cannot attach `Authorization`. The token middleware
/// checks the dedicated subprotocol during the upgrade; this narrow route
/// recognizer prevents a suffix lookalike from bypassing auth.
fn is_live_websocket_route(path: &str) -> bool {
    if matches!(path, "/api/notifications/live" | "/notifications/live") {
        return true;
    }
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

/// True when the browser `Origin` header names a loopback host. A remote
/// page cannot forge this: a browser sets `Origin` from the script's own
/// origin, never from where a URL later resolves, so this only ever matches
/// a page that is itself running on `localhost`/`127.0.0.1`.
fn origin_is_loopback(origin: &str) -> bool {
    reqwest::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| crate::config::is_loopback_host(&host))
}

/// Checks the browser `Origin` header against `TACK_ALLOWED_ORIGINS`. When
/// the server itself is bound to loopback, an `Origin` that also names a
/// loopback host is accepted even if it isn't in the configured list — on a
/// single machine, whoever can open this connection already holds the same
/// access as the bind itself (the posture ADR 0004 states for the rest of
/// this API). This is what lets any local dev port (Vite's default `5173`
/// included) reach the socket with no configuration; it does not change
/// what a non-loopback bind accepts, since that check still requires an
/// exact match against the configured list.
fn live_websocket_is_authorized(req: &Request, state: &AppState) -> bool {
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        let listed = state
            .config
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin);
        let same_machine = state.config.binds_loopback() && origin_is_loopback(origin);
        if !listed && !same_machine {
            return false;
        }
    }

    let Some(expected) = state.config.api_token.as_deref() else {
        return true;
    };
    if session_cookie_matches(req, expected) {
        return true;
    }
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

/// Name of the cookie the page response sets so a browser reaches `/api`
/// without holding the token in script.
pub const SESSION_COOKIE: &str = "tack_session";

/// True when a `Cookie` header carries `tack_session=<expected>`.
fn session_cookie_matches(req: &Request, expected: &str) -> bool {
    req.headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix("tack_session="))
        .any(|value| constant_time_eq(value.as_bytes(), expected.as_bytes()))
}

/// Middleware for everything outside `/api`: an HTML page answered to a
/// `GET` that names this server in `Host` also sets the session cookie.
pub async fn set_session_cookie(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let wanted = req.method() == axum::http::Method::GET
        && !req.uri().path().starts_with("/api")
        && req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|host| host_is_accepted(&bare_host(host), &state));
    let mut response = next.run(req).await;
    let is_html = response.status().is_success()
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"));
    if let (true, true, Some(token)) = (wanted, is_html, state.config.api_token.as_deref())
        && let Ok(value) = HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}={token}; Path=/api; HttpOnly; SameSite=Strict"
        ))
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
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

/// The host part of a `Host` header value or an origin's authority: port
/// stripped, brackets kept off an IPv6 literal, lowercased.
fn bare_host(authority: &str) -> String {
    let host = match authority.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest),
        None => match authority.rsplit_once(':') {
            Some((host, port))
                if !host.contains(':') && port.bytes().all(|b| b.is_ascii_digit()) =>
            {
                host
            }
            _ => authority,
        },
    };
    host.to_ascii_lowercase()
}

/// True when `host` (already bare) is a name this server answers to: loopback,
/// the configured bind host, or the host of a configured allowed origin.
fn host_is_accepted(host: &str, state: &AppState) -> bool {
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    if bare_host(&state.config.host) == host {
        return true;
    }
    state.config.allowed_origins.iter().any(|origin| {
        reqwest::Url::parse(origin)
            .ok()
            .and_then(|url| url.host_str().map(bare_host))
            .is_some_and(|allowed| allowed == host)
    })
}

/// Middleware: answer `421` unless the request's `Host` (or `:authority`)
/// names this server. A page reaching the loopback socket through a name an
/// attacker controls (DNS rebinding) carries that name here.
pub async fn require_host(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let authority = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| req.uri().authority().map(|a| a.to_string()));
    match authority {
        Some(authority) if host_is_accepted(&bare_host(&authority), &state) => {
            Ok(next.run(req).await)
        }
        _ => Err(ApiError::MisdirectedHost(
            "this server does not answer to that host name".into(),
        )),
    }
}

/// Middleware: refuse a state-changing request (any method but `GET`, `HEAD`,
/// `OPTIONS`) that a page on another site sent: `Sec-Fetch-Site: cross-site`,
/// or an `Origin` that is neither one of this server's own origins nor a
/// configured allowed origin. `OPTIONS` is left to the CORS layer.
pub async fn refuse_cross_site(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) {
        return Ok(next.run(req).await);
    }
    let cross_site = req
        .headers()
        .get("sec-fetch-site")
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"cross-site"));
    let foreign_origin = req.headers().get(header::ORIGIN).is_some_and(|origin| {
        let Ok(origin) = origin.to_str() else {
            return true;
        };
        if state.config.allowed_origins.iter().any(|a| a == origin) {
            return false;
        }
        match reqwest::Url::parse(origin) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => !url
                .host_str()
                .is_some_and(|host| host_is_accepted(&bare_host(host), &state)),
            _ => true,
        }
    });
    if cross_site || foreign_origin {
        return Err(ApiError::CrossSiteRequest(
            "a page on another site may not change anything here".into(),
        ));
    }
    Ok(next.run(req).await)
}

/// Middleware: puts the caller's [`Principal`] in the request's extensions on every route, and
/// refuses a caller that is none.
///
/// Three callers exist. A route that needs no one (`/api/health`, the schema) is Tack itself with
/// no scope. A `Bearer tack_agent_...` credential that is active is an agent with the scopes it
/// was issued, and may reach only the routes `agent_access` lists. Anything else is the operator:
/// when a token is configured it must match the `Authorization: Bearer <token>` header or the
/// `tack_session` cookie, compared in constant time; when none is configured (loopback only, or
/// the explicit opt-out of ADR 0004) every caller is the operator. The operator holds every
/// scope and is never asked about one, so Tack's own token behaves as it always did.
pub async fn require_token(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if is_public_route(&path) {
        req.extensions_mut().insert(Principal::system());
        return next.run(req).await;
    }

    if is_live_websocket_route(&path) {
        return if live_websocket_is_authorized(&req, &state) {
            req.extensions_mut()
                .insert(operator_principal(&state.config));
            next.run(req).await
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        };
    }

    let bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);

    // An agent credential, whether or not a token is configured.
    if let Some(secret) = bearer
        .as_deref()
        .filter(|secret| secret.starts_with(tack_app::runs::agent_credential::SECRET_PREFIX))
    {
        let credential = match tack_app::runs::agent_credential::authenticate(
            &state.repo,
            chrono::Utc::now(),
            secret,
        )
        .await
        {
            Ok(Some(credential)) => credential,
            Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
            Err(error) => return ApiError::from(error).into_response(),
        };
        let principal = credential.principal();
        if let Err(refusal) =
            crate::agent_access::authorize(&state, &principal, req.method(), &path).await
        {
            return refusal.into_response();
        }
        req.extensions_mut().insert(principal);
        return next.run(req).await;
    }

    let allowed = match state.config.api_token {
        // No token configured. A loopback bind always has one
        // (`AppConfig::ensure_api_token` creates it), and a non-loopback bind
        // without one is refused at startup by `AppConfig::validate_security`,
        // so this is reached only when the operator bound a non-loopback address
        // with TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1 and accepted an open API.
        None => true,
        Some(ref expected) => {
            bearer
                .as_deref()
                .is_some_and(|tok| constant_time_eq(tok.as_bytes(), expected.as_bytes()))
                || session_cookie_matches(&req, expected)
        }
    };
    if !allowed {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    req.extensions_mut()
        .insert(operator_principal(&state.config));
    next.run(req).await
}

/// Header the operator execution/fleet handlers
/// (`crate::handlers::executions`, `crate::handlers::runner_admin`) read to
/// scope idempotency and audit `actor` fields. Those handlers trust it
/// completely, so it must never be attacker-controlled — see the strip-and-
/// replace below, which is what makes that true.
pub const OPERATOR_PRINCIPAL_HEADER: &str = "x-tack-principal";

/// The operator, derived from the configured secret rather than anything the client sent: a
/// stable, non-secret label of the token (or `local` when none is configured and every caller is
/// equally trusted). Tack's operator-auth model
/// (`docs/contracts/runner-v1/protocol.json`'s `operator_session_or_api_token`) is one shared bearer
/// token, so every request that clears [`require_token`] with it is the same principal
/// (ADR 0004); an agent credential is the other kind of caller (ADR 0034).
pub fn operator_principal(config: &crate::config::AppConfig) -> Principal {
    match config.api_token.as_deref() {
        Some(token) if !token.is_empty() => {
            let digest = Sha256::digest(token.as_bytes());
            Principal::operator(format!("token:{}", hex::encode(&digest[..8])))
        }
        _ => Principal::operator("local"),
    }
}

/// Strips any client-supplied [`OPERATOR_PRINCIPAL_HEADER`] and replaces it with the label of the
/// principal [`require_token`] resolved. Layered directly on the operator execution/fleet
/// sub-router in `router.rs`, *inside* the `require_token` gate, so it only ever runs for a
/// request that already cleared authentication (an agent only reaches routes `agent_access`
/// lists, none of which is here). A request arriving with its own `x-tack-principal` set is not
/// an error — it is simply overwritten, so a client cannot read or collide with another
/// principal's idempotency scope by guessing or copying a header value.
pub async fn inject_operator_principal(mut req: Request, next: Next) -> Response {
    let label = req
        .extensions()
        .get::<Principal>()
        .map(Principal::label)
        .unwrap_or_else(|| "system:tack".to_string());
    let value = HeaderValue::from_str(&label)
        .expect("a principal label is ASCII and always a valid header value");
    req.headers_mut()
        .insert(HeaderName::from_static(OPERATOR_PRINCIPAL_HEADER), value);
    next.run(req).await
}

/// Header `tack mcp` sets on every call it makes; the value `mcp` marks the call
/// as an agent's, not a person's.
pub const CLIENT_HEADER: &str = "x-tack-client";

/// Whether the request came through `tack mcp` (`X-Tack-Client: mcp`), put in
/// the request's extensions for the handlers that treat an agent's move differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FromMcp(pub bool);

/// Reads [`CLIENT_HEADER`] into the request's extensions as [`FromMcp`].
pub async fn read_client(mut req: Request, next: Next) -> Response {
    let from_mcp = req
        .headers()
        .get(CLIENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("mcp"));
    req.extensions_mut().insert(FromMcp(from_mcp));
    next.run(req).await
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
    use super::{
        constant_time_eq, decode_base64url, is_live_websocket_route, is_public_route,
        operator_principal,
    };
    use crate::config::AppConfig;

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
    }

    #[test]
    fn only_the_exact_board_websocket_shape_defers_to_upgrade_auth() {
        assert!(is_live_websocket_route(
            "/api/projects/00000000-0000-0000-0000-000000000000/boards/live"
        ));
        assert!(!is_live_websocket_route(
            "/api/projects/not-a-uuid/boards/live"
        ));
        assert!(!is_live_websocket_route("/api/projects/health/boards/live"));
        assert!(!is_live_websocket_route(
            "/api/projects/00000000-0000-0000-0000-000000000000/boards/live-evil"
        ));
    }

    #[test]
    fn only_the_exact_notifications_websocket_shape_defers_to_upgrade_auth() {
        assert!(is_live_websocket_route("/api/notifications/live"));
        assert!(!is_live_websocket_route("/api/notifications/live-evil"));
        assert!(!is_live_websocket_route("/api/notifications/live/more"));
        assert!(!is_live_websocket_route("/api/notifications"));
    }

    #[test]
    fn websocket_subprotocol_creds_decode_without_query_params() {
        assert_eq!(
            decode_base64url("b3BlcmF0b3ItdG9rZW4"),
            Some(b"operator-token".to_vec())
        );
        assert_eq!(decode_base64url("not=valid"), None);
    }

    /// The runner-v1 router and the operator execution/fleet
    /// router each authenticate with their own distinct
    /// credential type and must never be listed here — `require_token`'s
    /// exemption check must stay exact-match, and neither router is mounted
    /// inside the `require_token`-gated sub-app in `router.rs` in the first
    /// place (see `build_router`'s `runner_protocol_routes` nest, which sits outside
    /// the `api` router's `require_token` layer entirely). This test pins
    /// the narrower, directly-checkable half of that guarantee: even if a
    /// future edit accidentally added one of these paths to this list, a
    /// suffix/prefix lookalike must still not slip through.
    #[test]
    fn no_runner_or_execution_path_is_publicly_exempt() {
        for path in [
            "/api/runner/v1/enroll",
            "/api/runner/v1/refresh",
            "/api/runner/v1/claim",
            "/api/runner/v1/heartbeat",
            "/api/runner/v1/attempts/att_1/events",
            "/api/runner/v1/attempts/att_1/recovery-observation",
            "/runner/v1/enroll",
            "/api/executions",
            "/api/executions/exec_1",
            "/api/executions/exec_1/cancel",
            "/api/executions/exec_1/requeue",
            "/api/runner-fleets",
            "/api/runners/enrollment",
            "/api/runners/runr_1/revoke",
            "/api/agent-profiles",
            "/executions",
            "/runner-fleets",
        ] {
            assert!(!is_public_route(path), "unexpectedly exempt: {path}");
        }
        // Suffix/prefix lookalikes of the routes that genuinely *are* public
        // must still be rejected — the existing behavior this test also
        // guards against regressing while adding the rows above.
        for lookalike in [
            "/api/healthy",
            "/api/health/",
            "/api/openapi.json.evil",
            "/api/health2",
        ] {
            assert!(
                !is_public_route(lookalike),
                "unexpectedly exempt: {lookalike}"
            );
        }
    }

    #[test]
    fn operator_principal_is_stable_and_never_the_raw_token() {
        let no_token = AppConfig::default();
        let first = operator_principal(&no_token).label();
        let second = operator_principal(&no_token).label();
        assert_eq!(first, second);
        assert_eq!(first, "operator:local");

        let with_token = AppConfig {
            api_token: Some("super-secret-value".into()),
            ..AppConfig::default()
        };
        let derived = operator_principal(&with_token).label();
        assert_ne!(derived, "operator:local");
        assert!(derived.starts_with("operator:token:"));
        assert!(!derived.contains("super-secret-value"));
        assert_eq!(derived, operator_principal(&with_token).label());

        let different_token = AppConfig {
            api_token: Some("another-secret".into()),
            ..AppConfig::default()
        };
        assert_ne!(derived, operator_principal(&different_token).label());
    }

    #[test]
    fn the_operator_holds_every_scope() {
        let operator = operator_principal(&AppConfig::default());
        assert!(operator.is_operator());
        assert!(operator.may_everywhere(tack_core::principal::Capability::Act));
    }
}
