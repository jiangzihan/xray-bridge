// Bearer token authentication extractor.
// Uses constant-time comparison to prevent timing attacks.
// Mirrors Python's bearer_auth dependency behavior exactly.

use async_trait::async_trait;
use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{error::AppError, nodes::AppState};

/// Axum extractor that validates `Authorization: Bearer <token>` header.
/// Returns `AppError::Unauthenticated` for missing/wrong token.
#[derive(Debug)]
pub struct BearerAuth;

#[async_trait]
impl FromRequestParts<AppState> for BearerAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());

        let Some(auth_str) = auth_header else {
            return Err(AppError::Unauthenticated(
                "缺少 Authorization header".to_owned(),
            ));
        };

        let Some(token) = auth_str.strip_prefix("Bearer ").or_else(|| {
            // Case-insensitive prefix check
            let lower = auth_str.to_lowercase();
            if lower.starts_with("bearer ") {
                Some(&auth_str[7..])
            } else {
                None
            }
        }) else {
            return Err(AppError::Unauthenticated(
                "Authorization header 格式应为: Bearer <token>".to_owned(),
            ));
        };

        if token.is_empty() {
            return Err(AppError::Unauthenticated(
                "Authorization header 格式应为: Bearer <token>".to_owned(),
            ));
        }

        // Constant-time comparison to prevent timing attacks
        if !constant_time_eq(token.as_bytes(), state.settings.bridge_token.as_bytes()) {
            return Err(AppError::Unauthenticated("Token 无效".to_owned()));
        }

        Ok(BearerAuth)
    }
}

/// Constant-time byte slice comparison.
/// Returns true iff a == b in all bytes and same length.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::{config::Settings, nodes::XrayClientCache};

    fn make_state(token: &str) -> AppState {
        let settings = Arc::new(Settings {
            bridge_token: token.to_owned(),
            port: 8080,
        });
        let cache = Arc::new(XrayClientCache::new(256));
        AppState { settings, cache }
    }

    fn make_request(auth: Option<&str>) -> Parts {
        let mut builder = axum::http::Request::builder().method("GET").uri("/test");
        if let Some(auth_val) = auth {
            builder = builder.header("authorization", auth_val);
        }
        let req = builder.body(()).unwrap();
        req.into_parts().0
    }

    #[tokio::test]
    async fn missing_auth_header_returns_401() {
        let state = make_state("secret");
        let mut parts = make_request(None);
        let result = BearerAuth::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AppError::Unauthenticated(_)));
    }

    #[tokio::test]
    async fn wrong_scheme_returns_401() {
        let state = make_state("secret");
        let mut parts = make_request(Some("Basic secret"));
        let result = BearerAuth::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AppError::Unauthenticated(_)));
    }

    #[tokio::test]
    async fn wrong_token_returns_401() {
        let state = make_state("secret");
        let mut parts = make_request(Some("Bearer wrongtoken"));
        let result = BearerAuth::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AppError::Unauthenticated(_)));
    }

    #[tokio::test]
    async fn correct_token_succeeds() {
        let state = make_state("secret");
        let mut parts = make_request(Some("Bearer secret"));
        let result = BearerAuth::from_request_parts(&mut parts, &state).await;
        assert!(result.is_ok());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"xyz"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }
}
