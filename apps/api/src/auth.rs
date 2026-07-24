use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts, StatusCode};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routes::AppState;

#[derive(Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub exp: usize,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let secret = &state.settings.jwt_secret;

        let cookie = parts
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let token = cookie
            .split(';')
            .find(|c| c.trim().starts_with("ow_token="))
            .and_then(|c| c.trim().strip_prefix("ow_token="))
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let user_id = token_data
            .claims
            .sub
            .parse::<Uuid>()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        Ok(AuthUser {
            user_id,
            role: token_data.claims.role,
        })
    }
}

pub fn create_token(user_id: &Uuid, role: &str, jwt_secret: &str) -> Result<String, StatusCode> {
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        exp: chrono::Utc::now()
            .checked_add_signed(chrono::Duration::hours(24))
            .unwrap()
            .timestamp() as usize,
    };

    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub fn set_cookie(headers: &mut axum::http::HeaderMap, token: &str) {
    let cookie = format!(
        "ow_token={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400",
        token
    );
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());
}

pub fn clear_cookie(headers: &mut axum::http::HeaderMap) {
    let cookie = "ow_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-jwt-secret";

    #[test]
    fn test_create_token_roundtrip() {
        let user_id = Uuid::new_v4();
        let role = "admin";

        let token = create_token(&user_id, role, TEST_SECRET).unwrap();

        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        )
        .unwrap();

        assert_eq!(token_data.claims.sub, user_id.to_string());
        assert_eq!(token_data.claims.role, "admin");
        assert!(token_data.claims.exp > chrono::Utc::now().timestamp() as usize);
    }

    #[test]
    fn test_create_token_wrong_secret() {
        let user_id = Uuid::new_v4();
        let token = create_token(&user_id, "user", TEST_SECRET).unwrap();

        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret("wrong-secret".as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_create_token_user_role() {
        let user_id = Uuid::new_v4();
        let token = create_token(&user_id, "user", TEST_SECRET).unwrap();

        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        )
        .unwrap();

        assert_eq!(token_data.claims.role, "user");
    }

    #[test]
    fn test_set_cookie() {
        let mut headers = axum::http::HeaderMap::new();
        set_cookie(&mut headers, "test-token");

        let cookie_val = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(cookie_val.contains("ow_token=test-token"));
        assert!(cookie_val.contains("HttpOnly"));
        assert!(cookie_val.contains("SameSite=Lax"));
        assert!(cookie_val.contains("Max-Age=86400"));
    }

    #[test]
    fn test_clear_cookie() {
        let mut headers = axum::http::HeaderMap::new();
        clear_cookie(&mut headers);

        let cookie_val = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(cookie_val.contains("Max-Age=0"));
    }
}
