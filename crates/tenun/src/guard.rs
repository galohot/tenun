//! Protections for a server that can read the user's files.
//!
//! - Host check: a page on another site that re-points its DNS name at
//!   127.0.0.1 (DNS rebinding) sends its own name as Host, so it is refused.
//! - Origin check: a browser always sends Origin on cross-site requests and
//!   WebSocket handshakes. If it is present it must be this server.
//! - `X-Tenun` header on every change: a cross-site form cannot set it.
//! - LAN mode (`--host` beyond loopback): every request needs the token, given
//!   once in the URL and then kept in a cookie.

use crate::AppState;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

const COOKIE: &str = "tenun_token";

pub async fn guard(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();

    let mut set_cookie = None;
    if let Some(token) = &state.token {
        let from_cookie = headers
            .get(header::COOKIE)
            .and_then(|c| c.to_str().ok())
            .is_some_and(|c| {
                c.split(';')
                    .any(|kv| kv.trim() == format!("{COOKIE}={token}"))
            });
        let from_query = req
            .uri()
            .query()
            .is_some_and(|q| q.split('&').any(|kv| kv == format!("token={token}")));
        if !from_cookie && !from_query {
            return deny("Open the link that `tenun serve` printed (it has the access token).");
        }
        if from_query {
            set_cookie = Some(format!(
                "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict"
            ));
        }
    } else if !is_loopback_host(&host) {
        return deny("Tenun only answers on this computer.");
    }

    if let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok())
        && origin != "null"
        && origin.trim_end_matches('/') != format!("http://{host}")
        && !origin.starts_with("tauri://")
    {
        return deny("Cross-site request refused.");
    }

    let changes = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if changes && req.uri().path().starts_with("/api/") && !headers.contains_key("x-tenun") {
        return deny("Missing X-Tenun header.");
    }

    let mut res = next.run(req).await;
    if let Some(c) = set_cookie
        && let Ok(v) = HeaderValue::from_str(&c)
    {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
    res
}

fn deny(msg: &str) -> Response {
    (StatusCode::FORBIDDEN, Body::from(msg.to_string())).into_response()
}

pub fn is_loopback_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(h, _)| h)
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts() {
        assert!(is_loopback_host("127.0.0.1:4310"));
        assert!(is_loopback_host("localhost:80"));
        assert!(is_loopback_host("[::1]:4310"));
        assert!(!is_loopback_host("evil.example:4310"));
        assert!(!is_loopback_host("127.0.0.1.evil.example"));
    }
}
