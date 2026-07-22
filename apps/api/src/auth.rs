use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts, StatusCode};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");

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

pub fn create_token(user_id: &Uuid, role: &str) -> Result<String, StatusCode> {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");

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
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
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
