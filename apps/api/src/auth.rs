use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts, StatusCode};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routes::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Manager,
    User,
}

impl Role {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "manager" => Some(Role::Manager),
            "user" => Some(Role::User),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Manager => "manager",
            Role::User => "user",
        }
    }

    pub fn can_manage_users(&self) -> bool {
        matches!(self, Role::Admin | Role::Manager)
    }

    pub fn can_create_role(&self, target: &Role) -> bool {
        match self {
            Role::Admin => !matches!(target, Role::Admin),
            Role::Manager => matches!(target, Role::User),
            Role::User => false,
        }
    }

    pub fn can_manage_templates(&self) -> bool {
        matches!(self, Role::Admin | Role::Manager)
    }

    pub fn can_view_all_instances(&self) -> bool {
        matches!(self, Role::Admin | Role::Manager)
    }

    pub fn can_manage_instance(&self, owner_role: &Role) -> bool {
        match self {
            Role::Admin => true,
            Role::Manager => matches!(owner_role, Role::User),
            Role::User => false,
        }
    }

    pub fn can_manage_all_instances(&self) -> bool {
        matches!(self, Role::Admin)
    }

    pub fn can_manage_docker(&self) -> bool {
        matches!(self, Role::Admin | Role::Manager)
    }

    pub fn can_manage_registry(&self) -> bool {
        matches!(self, Role::Admin | Role::Manager)
    }
}

#[derive(Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: Role,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    pub fn can_manage_users(&self) -> bool {
        self.role.can_manage_users()
    }

    pub fn can_create_role(&self, target: &Role) -> bool {
        self.role.can_create_role(target)
    }
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

        let role = Role::from_str(&token_data.claims.role)
            .ok_or(StatusCode::UNAUTHORIZED)?;

        Ok(AuthUser {
            user_id,
            role,
        })
    }
}

pub fn create_token(user_id: &Uuid, role: &Role, jwt_secret: &str) -> Result<String, StatusCode> {
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.as_str().to_string(),
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

    const TEST_SECRET: &str = "test-jwt-secret";

    #[test]
    fn test_role_from_str() {
        assert_eq!(Role::from_str("admin"), Some(Role::Admin));
        assert_eq!(Role::from_str("manager"), Some(Role::Manager));
        assert_eq!(Role::from_str("user"), Some(Role::User));
        assert_eq!(Role::from_str("invalid"), None);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::Admin.as_str(), "admin");
        assert_eq!(Role::Manager.as_str(), "manager");
        assert_eq!(Role::User.as_str(), "user");
    }

    #[test]
    fn test_role_can_manage_users() {
        assert!(Role::Admin.can_manage_users());
        assert!(Role::Manager.can_manage_users());
        assert!(!Role::User.can_manage_users());
    }

    #[test]
    fn test_role_can_create_role() {
        assert!(Role::Admin.can_create_role(&Role::Manager));
        assert!(Role::Admin.can_create_role(&Role::User));
        assert!(!Role::Admin.can_create_role(&Role::Admin));

        assert!(Role::Manager.can_create_role(&Role::User));
        assert!(!Role::Manager.can_create_role(&Role::Manager));
        assert!(!Role::Manager.can_create_role(&Role::Admin));

        assert!(!Role::User.can_create_role(&Role::User));
        assert!(!Role::User.can_create_role(&Role::Manager));
        assert!(!Role::User.can_create_role(&Role::Admin));
    }

    #[test]
    fn test_role_can_manage_templates() {
        assert!(Role::Admin.can_manage_templates());
        assert!(Role::Manager.can_manage_templates());
        assert!(!Role::User.can_manage_templates());
    }

    #[test]
    fn test_role_can_view_all_instances() {
        assert!(Role::Admin.can_view_all_instances());
        assert!(Role::Manager.can_view_all_instances());
        assert!(!Role::User.can_view_all_instances());
    }

    #[test]
    fn test_role_can_manage_all_instances() {
        assert!(Role::Admin.can_manage_all_instances());
        assert!(!Role::Manager.can_manage_all_instances());
        assert!(!Role::User.can_manage_all_instances());
    }

    #[test]
    fn test_role_can_manage_instance() {
        assert!(Role::Admin.can_manage_instance(&Role::Admin));
        assert!(Role::Admin.can_manage_instance(&Role::Manager));
        assert!(Role::Admin.can_manage_instance(&Role::User));

        assert!(!Role::Manager.can_manage_instance(&Role::Admin));
        assert!(!Role::Manager.can_manage_instance(&Role::Manager));
        assert!(Role::Manager.can_manage_instance(&Role::User));

        assert!(!Role::User.can_manage_instance(&Role::Admin));
        assert!(!Role::User.can_manage_instance(&Role::Manager));
        assert!(!Role::User.can_manage_instance(&Role::User));
    }

    #[test]
    fn test_role_can_manage_docker() {
        assert!(Role::Admin.can_manage_docker());
        assert!(Role::Manager.can_manage_docker());
        assert!(!Role::User.can_manage_docker());
    }

    #[test]
    fn test_role_can_manage_registry() {
        assert!(Role::Admin.can_manage_registry());
        assert!(Role::Manager.can_manage_registry());
        assert!(!Role::User.can_manage_registry());
    }

    #[test]
    fn test_create_token_roundtrip() {
        let user_id = Uuid::new_v4();
        let role = Role::Admin;

        let token = create_token(&user_id, &role, TEST_SECRET).unwrap();

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
        let token = create_token(&user_id, &Role::User, TEST_SECRET).unwrap();

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
        let token = create_token(&user_id, &Role::User, TEST_SECRET).unwrap();

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
        assert!(cookie_val.contains("Secure"));
        assert!(cookie_val.contains("SameSite=Lax"));
        assert!(cookie_val.contains("Max-Age=604800"));
    }

    #[test]
    fn test_create_token_expires_in_seven_days() {
        let user_id = Uuid::new_v4();
        let role = Role::Admin;

        let token = create_token(&user_id, &role, TEST_SECRET).unwrap();
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
    fn test_clear_cookie() {
        let mut headers = axum::http::HeaderMap::new();
        clear_cookie(&mut headers);

        let cookie_val = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(cookie_val.contains("Max-Age=0"));
    }
}
