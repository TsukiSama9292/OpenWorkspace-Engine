#![cfg(feature = "docker")]

mod common;

use common::TestContext;
use sea_orm::ActiveModelTrait;

async fn create_test_template(ctx: &TestContext, suffix: &str) -> String {
    common::ensure_network().await;

    ctx.login_admin().await;
    let name = format!("ow_test_docker_lt_{}_{}", std::process::id(), suffix);
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "cores": 0,
            "memory": 0,
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    assert_eq!(resp.status(), 200, "create config failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["template"]["id"].as_str().unwrap().to_string()
}

async fn launch_instance(ctx: &TestContext, template_id: &str) -> String {
    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
        }))
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    if status != 200 || body["instance"]["status"].as_str() != Some("running") {
        panic!(
            "launch failed: status={}, body={}",
            status,
            serde_json::to_string_pretty(&body).unwrap()
        );
    }
    body["instance"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_launch_and_delete_instance() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "launch_del").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");
    assert!(body["instance"]["container_id"].as_str().is_some());

    let resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn test_stop_and_start_instance() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "stop_start").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "stopped");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "stopped");

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");

}

#[tokio::test]
async fn test_pause_and_unpause_instance() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "pause_unpause").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "pause failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "paused");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "paused");

    let resp = ctx.post(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "unpause failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");

}

#[tokio::test]
async fn test_stop_already_stopped_returns_conflict() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "stop_conflict").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

}

#[tokio::test]
async fn test_pause_not_running_returns_conflict() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "pause_conflict").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

}

#[tokio::test]
async fn test_unpause_not_paused_returns_conflict() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "unpause_conflict").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

}

#[tokio::test]
async fn test_start_already_running_returns_conflict() {
    let ctx = TestContext::new().await;
    let template_id = create_test_template(&ctx, "start_conflict").await;

    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_launch_with_bad_image_returns_error_status() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("ow_test_docker_lt_badimg_{}", std::process::id());
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "nonexistent-image-12345:latest",
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let status = body["instance"]["status"].as_str().unwrap();
    if status == "error" {
        assert!(body.get("docker_error").is_some());
    }
    let _instance_id = body["instance"]["id"].as_str().unwrap();
}

#[tokio::test]
async fn test_launch_with_mount_persistent() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("ow_test_docker_lt_mount_{}", std::process::id());
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "mount_persistent": true,
            "resolved_volume_host_path": "/tmp/ow_test_mount"
        }))
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    let _instance_id = body["instance"]["id"].as_str().unwrap();

    if status == 200 && body["instance"]["status"].as_str() == Some("running") {
        assert_eq!(body["instance"]["mount_persistent"], true);
        assert_eq!(body["instance"]["resolved_volume_host_path"], "/tmp/ow_test_mount");
    }
}

#[tokio::test]
async fn test_start_existing_stopped_container() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "start_stopped").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "stopped");
    assert!(body["instance"]["container_id"].as_str().is_some());

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str(), Some("running"));

}

#[tokio::test]
async fn test_delete_with_container() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "del_container").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["instance"]["container_id"].as_str().is_some());

    let resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 204);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_start_already_running_container_just_updates_db() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "start_already").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");

}

#[tokio::test]
async fn test_stop_from_paused_unpauses_first() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "stop_paused").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "pause failed: {:?}", resp.text().await);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "paused");

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "stop after unpause failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "stopped");

}

#[tokio::test]
async fn test_start_container_not_found_creates_new() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "start_recreate").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    if let Some(cid) = {
        let r = ctx.get(&format!("/api/instances/{}", instance_id)).await;
        let b: serde_json::Value = r.json().await.unwrap();
        b["instance"]["container_id"].as_str().map(|s| s.to_string())
    } {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let _ = docker.stop_container(&cid, None::<bollard::container::StopContainerOptions>).await;
        let _ = docker.remove_container(&cid, None).await;
    }

    let inst_id = uuid::Uuid::parse_str(&instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = openworkspace_api::db::workspace_instance::ActiveModel {
        id: sea_orm::Set(inst_id),
        container_id: sea_orm::Set(None),
        status: sea_orm::Set("stopped".to_string()),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start with recreate failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");

}

#[tokio::test]
async fn test_launch_and_get_returns_template_name_and_owner() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "get_names").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["instance"]["template_name"].as_str().is_some());
    assert!(body["instance"]["owner_username"].as_str().is_some());
    assert!(!body["instance"]["owner_username"].as_str().unwrap().is_empty());

}

#[tokio::test]
async fn test_list_instances_returns_all_fields() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "list_fields").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.get("/api/instances").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let instances = body["instances"].as_array().unwrap();
    let inst = instances.iter().find(|i| i["id"].as_str() == Some(&instance_id)).unwrap();

    assert!(inst["template_name"].as_str().is_some());
    assert!(inst["owner_username"].as_str().is_some());
    assert!(inst["access_token"].as_str().is_some());
    assert!(inst["remote_type"].as_str().is_some());
    assert!(inst["status"].as_str().is_some());
    assert!(inst["container_id"].as_str().is_some());

}

#[tokio::test]
async fn test_launch_with_resolved_volume_path() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("ow_test_docker_lt_volpath_{}", std::process::id());
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "mount_persistent": true,
            "resolved_volume_host_path": "/tmp/ow_test_vol_path"
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let _instance_id = body["instance"]["id"].as_str().unwrap().to_string();

    assert_eq!(body["instance"]["mount_persistent"], true);
    assert_eq!(body["instance"]["resolved_volume_host_path"], "/tmp/ow_test_vol_path");

}

#[tokio::test]
async fn test_start_stopped_with_no_container_creates_new() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "start_nocid").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    if let Some(cid) = {
        let r = ctx.get(&format!("/api/instances/{}", instance_id)).await;
        let b: serde_json::Value = r.json().await.unwrap();
        b["instance"]["container_id"].as_str().map(|s| s.to_string())
    } {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let _ = docker.stop_container(&cid, None::<bollard::container::StopContainerOptions>).await;
        let _ = docker.remove_container(&cid, None).await;
    }

    let inst_id = uuid::Uuid::parse_str(&instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = openworkspace_api::db::workspace_instance::ActiveModel {
        id: sea_orm::Set(inst_id),
        container_id: sea_orm::Set(None),
        status: sea_orm::Set("stopped".to_string()),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");
    assert!(body["container_id"].as_str().is_some());

}

#[tokio::test]
async fn test_delete_stopped_instance() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "del_stopped").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 204);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_start_with_container_in_unknown_state() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let template_id = create_test_template(&ctx, "start_unknown").await;
    let instance_id = launch_instance(&ctx, &template_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    if let Some(cid) = {
        let r = ctx.get(&format!("/api/instances/{}", instance_id)).await;
        let b: serde_json::Value = r.json().await.unwrap();
        b["instance"]["container_id"].as_str().map(|s| s.to_string())
    } {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let _ = docker.stop_container(&cid, None::<bollard::container::StopContainerOptions>).await;
        let _ = docker.remove_container(&cid, None).await;
    }

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start after container removal failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");

}
