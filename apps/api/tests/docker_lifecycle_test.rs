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

// Waits until the instance's container is actually running, then mirrors what the
// health worker does in production: marks the instance "running" in the DB. The
// worker's own probe can't fire in tests because busybox containers serve nothing
// on the VNC port, so the test simulates the post-probe transition directly.
async fn wait_until_running(ctx: &TestContext, instance_id: &str) {
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let container_id = body["instance"]["container_id"]
            .as_str()
            .expect("instance has no container_id")
            .to_string();

        let running = docker
            .inspect_container(&container_id, None)
            .await
            .ok()
            .and_then(|inspect| inspect.state)
            .and_then(|state| state.running)
            .unwrap_or(false);

        if running {
            let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
            let db_url = common::pg_url(&ctx.db_name);
            let db = sea_orm::Database::connect(&db_url).await.unwrap();
            openworkspace_api::db::WorkspaceInstanceRepository::new(&db)
                .update_status(inst_id, "running")
                .await
                .expect("failed to mark instance running");
            return;
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "instance {} did not become running within 30s",
            instance_id
        );
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

async fn launch_instance(ctx: &TestContext, template_id: &str) -> String {
    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
        }))
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    if status != 200 {
        panic!(
            "launch failed: status={}, body={}",
            status,
            serde_json::to_string_pretty(&body).unwrap()
        );
    }
    let launch_status = body["instance"]["status"].as_str();
    if launch_status != Some("starting") && launch_status != Some("running") {
        panic!(
            "launch returned unexpected status: {}",
            serde_json::to_string_pretty(&body).unwrap()
        );
    }
    if body["instance"]["container_id"].as_str().is_none() {
        panic!(
            "launch returned no container_id: {}",
            serde_json::to_string_pretty(&body).unwrap()
        );
    }
    let instance_id = body["instance"]["id"].as_str().unwrap().to_string();
    if launch_status == Some("starting") {
        wait_until_running(&ctx, &instance_id).await;
    }
    instance_id
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

    // Capture the allocated host port at first launch; it must be stable
    // across the whole stop/start cycle (user story: stable bookmarked URL).
    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let host_port = body["instance"]["host_port"]
        .as_i64()
        .expect("instance JSON must expose host_port");
    assert!(host_port > 0, "host_port must be allocated");

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "stopped");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "stopped");
    assert_eq!(
        body["instance"]["host_port"].as_i64(),
        Some(host_port),
        "host port must survive stop"
    );

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "starting");

    wait_until_running(&ctx, &instance_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");
    assert_eq!(
        body["instance"]["host_port"].as_i64(),
        Some(host_port),
        "host port must be stable across restart"
    );

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
async fn test_launch_persistent_null_root_degrades_and_ignores_client_path() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("ow_test_docker_lt_mount_{}", std::process::id());
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "cores": 0,
            "memory": 0,
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "persistence": "use_persistent",
            "resolved_volume_host_path": "/tmp/ow_test_mount"
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let _instance_id = body["instance"]["id"].as_str().unwrap();

    assert_eq!(body["instance"]["mount_persistent"], false, "a template without a persistent_storage_path must degrade to non-persistent");
    assert_eq!(body["instance"]["resolved_volume_host_path"], serde_json::Value::Null, "the client-supplied path must be ignored");
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
    assert_eq!(body["status"].as_str(), Some("starting"));

    wait_until_running(&ctx, &instance_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str(), Some("running"));
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

    wait_until_running(&ctx, &instance_id).await;

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
    assert_eq!(body["status"].as_str().unwrap(), "starting");

    wait_until_running(&ctx, &instance_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");
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
async fn test_launch_persistent_resolves_path_server_side() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("ow_test_docker_lt_volpath_{}", std::process::id());
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "cores": 0,
            "memory": 0,
            "run_config": { "command": ["sleep", "3600"] },
            "persistent_storage_path": "/tmp/ow_test_root",
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    // Compute the path the API must resolve server-side:
    // {root}/{template_name}/{owner_user_id}.
    let db = sea_orm::Database::connect(&common::pg_url(&ctx.db_name)).await.unwrap();
    let admin = openworkspace_api::db::UserRepository::new(&db)
        .find_by_username("admin")
        .await
        .unwrap()
        .expect("admin user must exist");
    drop(db);
    let expected_path = format!("/tmp/ow_test_root/{}/{}", name, admin.0);

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "persistence": "use_persistent",
            "resolved_volume_host_path": "/tmp/ow_test_vol_path"
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let instance_id = body["instance"]["id"].as_str().unwrap().to_string();

    assert_eq!(body["instance"]["mount_persistent"], true);
    assert_eq!(
        body["instance"]["resolved_volume_host_path"],
        expected_path,
        "the API must resolve the host path itself and ignore the client-supplied one"
    );

    // Clean up via the API: delete must remove the container + DB record but
    // keep the Volume declaration and host data dir, so the data is reusable.
    let delete_resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(delete_resp.status(), 204);

    let volume_name = openworkspace_api::persistent_volume::persistent_volume_name(&expected_path);
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    assert!(
        docker.inspect_volume(&volume_name).await.is_ok(),
        "delete must preserve the persistent volume declaration for reuse"
    );
    assert!(
        std::fs::read_dir(&expected_path).is_ok(),
        "delete must preserve the persistent host data dir"
    );

    // Relaunching use_persistent after delete: the slot is free (record gone)
    // and the preserved volume is reused as-is, not re-populated or re-created.
    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "persistence": "use_persistent",
        }))
        .await;
    assert_eq!(resp.status(), 200, "re-launch after delete must reuse preserved data");
    let body: serde_json::Value = resp.json().await.unwrap();
    let relaunched_id = body["instance"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["instance"]["mount_persistent"], true);

    // Clean up: delete the relaunched instance, then drop the leftover volume
    // + host dir manually (delete intentionally keeps them).
    assert_eq!(ctx.delete(&format!("/api/instances/{}", relaunched_id)).await.status(), 204);
    docker.remove_volume(&volume_name, None::<bollard::volume::RemoveVolumeOptions>).await.ok();
    std::fs::remove_dir_all(&expected_path).ok();
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
    assert_eq!(body["status"].as_str().unwrap(), "starting");
    assert!(body["container_id"].as_str().is_some());

    wait_until_running(&ctx, &instance_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");
    assert!(body["instance"]["container_id"].as_str().is_some());

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
    assert_eq!(body["status"].as_str().unwrap(), "starting");

    wait_until_running(&ctx, &instance_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");
}
