use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts, StatusCode};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::{PolicyRepository, UserRepository};
use crate::effective_context::EffectiveContext;
use crate::routes::AppState;

/// The authenticated user with the effective context resolved from the
/// database on this very request. All permission decisions go through the
/// context's flags / `is_admin` (root → Admin-group membership; manager → the
/// seeded Manager group, which carries all five flags).
#[derive(Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    /// The context computed by the policy module from the DB. Permission
    /// changes take effect on the next request without re-login.
    pub context: EffectiveContext,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.context.is_admin
    }

    pub fn can_manage_users(&self) -> bool {
        self.is_admin() || self.context.can_manage_users
    }

    pub fn can_manage_docker(&self) -> bool {
        self.is_admin() || self.context.can_manage_docker
    }

    pub fn can_manage_registry(&self) -> bool {
        self.is_admin() || self.context.can_manage_registry
    }
}

/// Identity-only JWT claims: the user id and the expiry. The token never
/// carries a role, so a stale permission claim can never outlive a change.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
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

        let user = UserRepository::new(&state.db)
            .find_by_id(user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let context = PolicyRepository::new(&state.db)
            .load_effective_context(user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        Ok(AuthUser {
            user_id,
            username: user.username,
            context,
        })
    }
}

pub fn create_token(user_id: &Uuid, jwt_secret: &str) -> Result<String, StatusCode> {
    let claims = Claims {
        sub: user_id.to_string(),
        exp: chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(7))
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

const SESSION_COOKIE_MAX_AGE: i64 = 7 * 24 * 60 * 60;

pub fn set_cookie(headers: &mut axum::http::HeaderMap, token: &str) {
    let cookie = format!(
        "ow_token={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
        token, SESSION_COOKIE_MAX_AGE
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
    use crate::effective_context::EffectiveContext;

    const TEST_SECRET: &str = "test-jwt-secret";

    fn auth_user(
        is_admin: bool,
        can_create_template: bool,
        can_manage_users: bool,
        can_manage_group_instances: bool,
        can_manage_docker: bool,
        can_manage_registry: bool,
    ) -> AuthUser {
        AuthUser {
            user_id: Uuid::new_v4(),
            username: "tester".to_string(),
            context: EffectiveContext {
                user_id: Uuid::new_v4(),
                username: "tester".to_string(),
                is_admin,
                tier: if is_admin { 2 } else { 0 },
                can_create_template,
                can_manage_users,
                can_manage_group_instances,
                can_manage_docker,
                can_manage_registry,
                effective_max_instances: 2,
                allowed_template_ids: vec![],
                group_ids: vec![],
                direct_max_instances: None,
            },
        }
    }

    #[test]
    fn test_is_admin_backed_by_context_flag() {
        assert!(auth_user(true, false, false, false, false, false).is_admin());
        assert!(!auth_user(false, false, false, false, false, false).is_admin());
    }

    #[test]
    fn test_compat_gates_reflect_context_flags() {
        let manager = auth_user(false, true, true, true, true, true);
        assert!(manager.can_manage_users());
        assert!(manager.can_manage_docker());
        assert!(manager.can_manage_registry());

        let plain = auth_user(false, false, false, false, false, false);
        assert!(!plain.can_manage_users());
        assert!(!plain.can_manage_docker());
        assert!(!plain.can_manage_registry());
    }

    #[test]
    fn test_admin_bypasses_every_gate() {
        let admin = auth_user(true, false, false, false, false, false);
        assert!(admin.can_manage_users());
        assert!(admin.can_manage_docker());
        assert!(admin.can_manage_registry());
    }

    #[test]
    fn test_create_token_carries_identity_only() {
        let user_id = Uuid::new_v4();

        let token = create_token(&user_id, TEST_SECRET).unwrap();

        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        )
        .unwrap();

        assert_eq!(token_data.claims.sub, user_id.to_string());
        assert!(token_data.claims.exp > chrono::Utc::now().timestamp() as usize);

        let payload: serde_json::Value = decode::<serde_json::Value>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        )
        .unwrap()
        .claims;
        assert_eq!(payload["sub"], user_id.to_string());
        assert!(payload.get("role").is_none(), "JWT must not carry a role claim");
    }

    #[test]
    fn test_create_token_wrong_secret() {
        let user_id = Uuid::new_v4();
        let token = create_token(&user_id, TEST_SECRET).unwrap();

        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret("wrong-secret".as_bytes()),
            &Validation::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_create_token_expires_in_seven_days() {
        let user_id = Uuid::new_v4();

        let token = create_token(&user_id, TEST_SECRET).unwrap();
        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(TEST_SECRET.as_bytes()),
            &Validation::default(),
        )
        .unwrap();

        let now = chrono::Utc::now().timestamp() as usize;
        let seven_days = 7 * 24 * 60 * 60;
        let remaining = token_data.claims.exp.saturating_sub(now);
        assert!(remaining > seven_days - 60, "token should last ~7 days, got {remaining}s");
        assert!(remaining <= seven_days, "token should not exceed 7 days, got {remaining}s");
    }

    #[test]
    fn test_set_cookie() {
        let mut headers = axum::http::HeaderMap::new();
        set_cookie(&mut headers, "test-token");

        let cookie_val = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(cookie_val.contains("ow_token=test-token"));
        assert!(cookie_val.contains("HttpOnly"));
        assert!(cookie_val.contains("Secure"));
        assert!(cookie_val.contains("SameSite=Lax"));
        assert!(cookie_val.contains("Max-Age=604800"));
    }

    #[test]
    fn test_clear_cookie() {
        let mut headers = axum::http::HeaderMap::new();
        clear_cookie(&mut headers);

        let cookie_val = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(cookie_val.contains("Max-Age=0"));
    }
}
