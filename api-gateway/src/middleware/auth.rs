// ==============================================================================
// middleware/auth.rs - Authentik Authentication Middleware
// ==============================================================================
// Description: Extract and validate Authentik forward auth headers
// Author: Matt Barham
// Created: 2026-01-11
// Modified: 2026-01-11
// Version: 1.0.0
// ==============================================================================
//
// Security: This middleware enforces authentication by extracting the username
// from the X-authentik-username header set by Traefik's Authentik forward auth.
// If the header is missing, the request is rejected with 401 Unauthorized.
//
// The extracted username is used to set PostgreSQL RLS variable for row-level
// security enforcement.
//
// ==============================================================================

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode, HeaderMap},
    response::{IntoResponse, Response},
};

/// Authenticated user extracted from Authentik headers
///
/// This extractor reads the X-authentik-username header set by Traefik's
/// Authentik forward auth middleware. If the header is missing or invalid,
/// the request is rejected with 401 Unauthorized.
///
/// # Example
/// ```rust
/// async fn my_handler(AuthUser(username): AuthUser) -> impl IntoResponse {
///     format!("Hello, {}!", username)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUser(pub String);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract X-authentik-username header
        let username = parts
            .headers
            .get("X-authentik-username")
            .and_then(|value| value.to_str().ok())
            .filter(|s| !s.is_empty());

        match username {
            Some(username) => Ok(AuthUser(username.to_string())),
            None => {
                // Return 401 Unauthorized if header is missing
                Err((
                    StatusCode::UNAUTHORIZED,
                    "Missing or invalid X-authentik-username header",
                )
                    .into_response())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[tokio::test]
    async fn test_auth_user_extraction() {
        let mut req = Request::builder()
            .header("X-authentik-username", "testuser")
            .body(())
            .unwrap();

        let (mut parts, _) = req.into_parts();
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, "testuser");
    }

    #[tokio::test]
    async fn test_auth_user_missing_header() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_auth_user_empty_header() {
        let mut req = Request::builder()
            .header("X-authentik-username", "")
            .body(())
            .unwrap();

        let (mut parts, _) = req.into_parts();
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
    }
}
