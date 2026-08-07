//! OpenAPI generation for the 17 safe endpoints plus the admin-gated Monitor
//! snapshot (`.scratch/security-fuzzing/`, `.scratch/monitor-dashboard/`). The
//! spec is export-only: `ApiDoc::openapi()`
//! is built from the handler annotations and serialized by the
//! `export_openapi` binary into the committed `security/openapi.json`. Nothing
//! here is ever served at runtime — there is no `/api/openapi.json` route.
//!
//! Response schemas are dual-track (spec Decision "Response schemas"):
//!   - real serializable structs (`EffectiveContext`, `SystemSettings`,
//!     `LoginRequest`) derive `ToSchema` at their definition site;
//!   - `json!`-assembled hot paths (`instance_to_json`, `template_to_json`,
//!     …) keep their handlers returning `Json<serde_json::Value>` untouched;
//!     the envelope structs below exist only as documentation for the spec.

use chrono::{DateTime, Utc};
use utoipa::OpenApi;
use uuid::Uuid;

use crate::effective_context::EffectiveContext;
use crate::system_settings::SystemSettings;

/// The complete exportable spec: all 17 safe operations. Component schemas are
/// collected automatically from the operation annotations, so nothing is
/// listed here that the paths do not already reference. Handlers are named by
/// full module path so `path!` resolves each generated `__path_<fn>_impl`.
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::health,
        crate::routes::auth::login::login,
        crate::routes::auth::session::me,
        crate::routes::auth::session::validate,
        crate::routes::workspace::templates::list_templates,
        crate::routes::workspace::templates::get_template,
        crate::routes::workspace::instances::list_instances,
        crate::routes::workspace::instances::get_instance,
        crate::routes::proxy::vnc::vnc_verify,
        crate::routes::users::list_users,
        crate::routes::users::get_user,
        crate::routes::groups::list_groups,
        crate::routes::workspace::registry::get_registry,
        crate::routes::workspace::registry::get_registry_url,
        crate::routes::workspace::docker_raw::list_docker_containers,
        crate::routes::workspace::persistent_volumes::list_persistent_volumes,
        crate::routes::admin_settings::get_settings,
        crate::routes::monitor::snapshot,
    ),
    info(
        title = "OpenWorkspace API — security fuzz surface",
        description = "OpenAPI spec for the 17 security-fuzzable endpoints plus the admin-gated Monitor snapshot (see .scratch/security-fuzzing, .scratch/monitor-dashboard). Admin-gated operations are tagged `admin-gated` for the low-privilege RBAC-boundary pass. The spec is export-only and never served.",
        version = "1.0.0",
    )
)]
pub struct ApiDoc;

/// The regenerated spec as a stable `serde_json::Value`. Used by the export
/// binary and the drift-check unit test; key ordering is deterministic because
/// `serde_json::Value` sorts object keys.
pub fn export_json() -> serde_json::Value {
    serde_json::to_value(ApiDoc::openapi()).expect("OpenApi document must serialize")
}

// ── Declaration-only response schemas ─────────────────────────
// Each mirrors the `json!` shape a handler serializes. They are documentation
// only: Schemathesis validates the live body against them (lenient — extra
// fields tolerated), so a drifted serializer surfaces as a fuzz failure.

#[derive(utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(utoipa::ToSchema)]
pub struct ContextEnvelope {
    pub context: EffectiveContext,
}

#[derive(utoipa::ToSchema)]
pub struct SettingsEnvelope {
    pub settings: SystemSettings,
}

#[derive(utoipa::ToSchema)]
pub struct ValidateEnvelope {
    pub user_id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub tier: i32,
}

#[derive(utoipa::ToSchema)]
pub struct TemplateSchema {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub image: String,
    pub cores: i32,
    pub memory: i64,
    pub gpu_count: i32,
    pub docker_registry: Option<String>,
    pub remote_type: String,
    pub container_runtime: String,
    pub run_config: serde_json::Value,
    pub exec_config: serde_json::Value,
    pub volume_mappings: serde_json::Value,
    pub persistent_storage_path: Option<String>,
    pub max_run_seconds: Option<i64>,
    pub timeout_action: String,
    pub keep_time_seconds: Option<i64>,
    pub keep_time_action: String,
    pub network_bandwidth_up_mbps: i32,
    pub network_bandwidth_down_mbps: i32,
    pub docker_in_instance: bool,
    pub visibility: String,
    pub instance_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(utoipa::ToSchema)]
pub struct TemplateListEnvelope {
    pub templates: Vec<TemplateSchema>,
}

#[derive(utoipa::ToSchema)]
pub struct TemplateEnvelope {
    pub template: TemplateSchema,
}

#[derive(utoipa::ToSchema)]
pub struct InstanceSchema {
    pub id: Uuid,
    pub template_id: Uuid,
    pub name: String,
    pub instance_number: i32,
    pub owner_id: Uuid,
    pub owner_username: String,
    pub owner_group_ids: Vec<Uuid>,
    pub owner_tier: i32,
    pub container_id: Option<String>,
    pub host_port: Option<i32>,
    pub network_name: String,
    pub status: String,
    pub access_token: String,
    pub access_password: String,
    pub mount_persistent: bool,
    pub resolved_volume_host_path: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub template_name: Option<String>,
    pub remote_type: Option<String>,
    pub auto_sleeps_at: Option<DateTime<Utc>>,
    pub timeout_action: Option<String>,
    pub keep_time_deadline: Option<DateTime<Utc>>,
    pub keep_time_seconds: Option<i64>,
    pub keep_time_action: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(utoipa::ToSchema)]
pub struct InstanceListEnvelope {
    pub instances: Vec<InstanceSchema>,
}

#[derive(utoipa::ToSchema)]
pub struct InstanceEnvelope {
    pub instance: InstanceSchema,
}

#[derive(utoipa::ToSchema)]
pub struct UserSchema {
    pub id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
    pub direct_max_instances: Option<i32>,
    pub group_ids: Vec<Uuid>,
    pub is_admin: bool,
    pub tier: i32,
}

#[derive(utoipa::ToSchema)]
pub struct UserListEnvelope {
    pub users: Vec<UserSchema>,
}

#[derive(utoipa::ToSchema)]
pub struct UserEnvelope {
    pub user: UserSchema,
}

#[derive(utoipa::ToSchema)]
pub struct GroupSchema {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub kind: Option<String>,
    pub can_create_template: bool,
    pub can_manage_users: bool,
    pub can_manage_group_instances: bool,
    pub can_manage_docker: bool,
    pub can_manage_registry: bool,
    pub can_view_monitoring: bool,
    pub max_instances: Option<i32>,
    pub template_ids: Vec<Uuid>,
}

#[derive(utoipa::ToSchema)]
pub struct GroupListEnvelope {
    pub groups: Vec<GroupSchema>,
}

#[derive(utoipa::ToSchema)]
pub struct RegistryUrlEnvelope {
    pub url: Option<String>,
}

#[derive(utoipa::ToSchema)]
pub struct DockerContainerSchema {
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    pub status: String,
    pub state: String,
}

#[derive(utoipa::ToSchema)]
pub struct DockerContainersEnvelope {
    pub containers: Vec<DockerContainerSchema>,
}

#[derive(utoipa::ToSchema)]
pub struct VolumeSchema {
    pub id: Uuid,
    pub host_path: String,
    pub owner_id: Option<Uuid>,
    pub owner_username: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(utoipa::ToSchema)]
pub struct VolumesEnvelope {
    pub volumes: Vec<VolumeSchema>,
}

/// Documentation-only envelope for the Monitor snapshot (the handler returns
/// `Json<serde_json::Value>`; this schema describes its JSON shape).
#[derive(utoipa::ToSchema)]
pub struct MonitorSnapshotEnvelope {
    pub host: serde_json::Value,
    pub instances: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed `security/openapi.json` artifact must never silently
    /// diverge from the utoipa annotations: rebuilding `ApiDoc` and comparing
    /// against the committed file is the drift guard (spec Testing Decision 2).
    /// Regenerate with: `cargo run --bin export_openapi` from `apps/api`.
    #[test]
    fn committed_spec_is_in_sync() {
        let regenerated = export_json();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("security/openapi.json");
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .expect("security/openapi.json must exist — run the export binary"),
        )
        .expect("committed spec must be valid JSON");

        assert_eq!(
            regenerated,
            committed,
            "security/openapi.json is out of sync with the utoipa annotations; \
             regenerate it with: cargo run --bin export_openapi"
        );
    }

    /// Every annotated operation is present in the exported spec, and the 9
    /// admin-gated operations carry the `admin-gated` tag the Pass-2 custom
    /// check relies on. Guards against a handler being annotated but not
    /// registered in `ApiDoc` (or losing its tag).
    #[test]
    fn export_covers_all_safe_endpoints() {
        let doc = ApiDoc::openapi();
        let paths = doc.paths.paths;
        let assert_path = |path: &str| {
            assert!(paths.contains_key(path), "missing operation for {path}");
        };
        assert_path("/health");
        assert_path("/api/auth/login");
        assert_path("/api/auth/me");
        assert_path("/api/auth/validate");
        assert_path("/api/templates");
        assert_path("/api/templates/{id}");
        assert_path("/api/instances");
        assert_path("/api/instances/{id}");
        assert_path("/api/vnc/verify");
        assert_path("/api/users");
        assert_path("/api/users/{id}");
        assert_path("/api/groups");
        assert_path("/api/registry");
        assert_path("/api/registry/url");
        assert_path("/api/docker/containers");
        assert_path("/api/persistent-volumes");
        assert_path("/api/admin/settings");
        assert_path("/api/monitor/snapshot");

        for admin_path in [
            "/api/users",
            "/api/users/{id}",
            "/api/groups",
            "/api/registry",
            "/api/registry/url",
            "/api/docker/containers",
            "/api/persistent-volumes",
            "/api/admin/settings",
            "/api/monitor/snapshot",
        ] {
            let op = paths
                .get(admin_path)
                .and_then(|item| item.get.as_ref())
                .unwrap_or_else(|| panic!("GET operation missing for {admin_path}"));
            assert!(
                op.tags
                    .as_ref()
                    .is_some_and(|tags| tags.iter().any(|t| t == "admin-gated")),
                "expected admin-gated tag on {admin_path}"
            );
        }
    }
}
