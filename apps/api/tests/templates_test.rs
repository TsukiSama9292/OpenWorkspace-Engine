mod common;

use common::TestContext;

#[tokio::test]
async fn test_create_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "test-config",
        "image": "tsukisama9292/ow-kasmvnc-ubuntu:jammy",
        "cores": 2,
        "memory": 4294967296_i64
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["name"], "test-config");
    assert_eq!(body["template"]["cores"], 2);
    assert!(body["template"]["id"].is_string());
}

#[tokio::test]
async fn test_list_templates() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    ctx.post("/api/templates", &serde_json::json!({
        "name": "list-test-config",
        "image": "tsukisama9292/ow-kasmvnc-ubuntu:jammy"
    })).await;

    let resp = ctx.get("/api/templates").await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["templates"].is_array());
    let configs = body["templates"].as_array().unwrap();
    assert!(configs.iter().any(|c| c["name"] == "list-test-config"));
}

#[tokio::test]
async fn test_get_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "get-test-config",
        "image": "tsukisama9292/ow-kasmvnc-ubuntu:jammy"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["name"], "get-test-config");
    assert_eq!(body["template"]["instance_count"], 0);
}

#[tokio::test]
async fn test_update_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "update-test",
        "image": "tsukisama9292/ow-kasmvnc-ubuntu:jammy",
        "cores": 1
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "update-test-renamed",
        "image": "tsukisama9292/ow-kasmvnc-ubuntu:jammy",
        "cores": 4,
        "memory": 8589934592_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {}
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["name"], "update-test-renamed");
    assert_eq!(body["template"]["cores"], 4);
}

#[tokio::test]
async fn test_delete_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "delete-test",
        "image": "tsukisama9292/ow-kasmvnc-ubuntu:jammy"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.delete(&format!("/api/templates/{}", template_id)).await;
    assert_eq!(resp.status(), 204);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_get_nonexistent_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.get(&format!("/api/templates/{}", fake_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_update_nonexistent_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.put(&format!("/api/templates/{}", fake_id), &serde_json::json!({
        "name": "updated",
        "image": "test:latest",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {}
    })).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_nonexistent_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.delete(&format!("/api/templates/{}", fake_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_create_template_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "no-auth-config",
        "image": "test:latest"
    })).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_list_templates_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/templates").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_get_template_requires_auth() {
    let ctx = TestContext::new().await;
    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.get(&format!("/api/templates/{}", fake_id)).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_create_template_with_all_fields() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "full-config",
        "description": "A full config",
        "image": "kasmweb/desktop:1.19.0",
        "cores": 4,
        "memory": 8589934592_i64,
        "gpu_count": 1,
        "docker_registry": "https://myregistry.com",
        "persistent_storage_path": "/data/{template_name}/{user_id}",
        "run_config": { "ports": [8080] },
        "exec_config": { "command": ["/bin/sh"] },
        "volume_mappings": { "/host": "/container" }
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["name"], "full-config");
    assert_eq!(body["template"]["cores"], 4);
    assert_eq!(body["template"]["gpu_count"], 1);
}

#[tokio::test]
async fn test_create_template_with_network_bandwidth() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bw-config",
        "image": "busybox:1",
        "network_bandwidth_up_mbps": 100,
        "network_bandwidth_down_mbps": 50
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["network_bandwidth_up_mbps"], 100);
    assert_eq!(body["template"]["network_bandwidth_down_mbps"], 50);

    let template_id = body["template"]["id"].as_str().unwrap();
    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["network_bandwidth_up_mbps"], 100);
    assert_eq!(body["template"]["network_bandwidth_down_mbps"], 50);
}

#[tokio::test]
async fn test_create_template_default_network_bandwidth() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "no-bw-config",
        "image": "busybox:1"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["network_bandwidth_up_mbps"], 0);
    assert_eq!(body["template"]["network_bandwidth_down_mbps"], 0);
}

#[tokio::test]
async fn test_create_template_rejects_negative_network_bandwidth() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bad-up-config",
        "image": "busybox:1",
        "network_bandwidth_up_mbps": -1
    })).await;
    assert_eq!(resp.status(), 400);

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bad-down-config",
        "image": "busybox:1",
        "network_bandwidth_down_mbps": -5
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_update_template_network_bandwidth() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bw-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "bw-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "network_bandwidth_up_mbps": 250,
        "network_bandwidth_down_mbps": 120
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["network_bandwidth_up_mbps"], 250);
    assert_eq!(body["template"]["network_bandwidth_down_mbps"], 120);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["network_bandwidth_up_mbps"], 250);
    assert_eq!(body["template"]["network_bandwidth_down_mbps"], 120);
}

#[tokio::test]
async fn test_update_template_rejects_negative_network_bandwidth() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bw-bad-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "bw-bad-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "network_bandwidth_down_mbps": -10
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_update_template_bandwidth_resets_to_zero_when_omitted() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bw-reset",
        "image": "busybox:1",
        "network_bandwidth_up_mbps": 80,
        "network_bandwidth_down_mbps": 40
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "bw-reset",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {}
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["network_bandwidth_up_mbps"], 0);
    assert_eq!(body["template"]["network_bandwidth_down_mbps"], 0);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_create_template_with_defaults() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "defaults-config"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["image"], "tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy");
    assert_eq!(body["template"]["cores"], 2);
    assert_eq!(body["template"]["memory"], 4_294_967_296_i64);
}

#[tokio::test]
async fn test_list_templates_as_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    ctx.post("/api/templates", &serde_json::json!({
        "name": "admin-owned-config",
        "image": "busybox:1"
    })).await;

    ctx.post("/api/users", &serde_json::json!({
        "username": "cfglist_nonadmin",
        "password": "pass123"
    })).await;
    ctx.login_user("cfglist_nonadmin", "pass123").await;

    let resp = ctx.get("/api/templates").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let configs = body["templates"].as_array().unwrap();
    assert!(configs.is_empty(), "non-admin should not see admin's configs");
}

#[tokio::test]
async fn test_create_template_with_null_optional_fields() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "null-fields-config",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "run_config": null,
        "exec_config": null,
        "volume_mappings": null
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["run_config"], serde_json::json!({}));
    assert_eq!(body["template"]["exec_config"], serde_json::json!({}));
    assert_eq!(body["template"]["volume_mappings"], serde_json::json!({}));
}

#[tokio::test]
async fn test_template_to_json_fields_in_response() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "json-fields",
        "description": "testing json output",
        "image": "test:latest",
        "cores": 4,
        "memory": 8192,
        "gpu_count": 1,
        "docker_registry": "myreg.io",
        "run_config": {"key": "val"},
        "exec_config": {"cmd": true},
        "volume_mappings": {"/h": "/c"},
        "persistent_storage_path": "/data"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let cfg = &body["template"];

    assert!(cfg["id"].is_string());
    assert_eq!(cfg["name"], "json-fields");
    assert_eq!(cfg["description"], "testing json output");
    assert_eq!(cfg["image"], "test:latest");
    assert_eq!(cfg["cores"], 4);
    assert_eq!(cfg["memory"], 8192);
    assert_eq!(cfg["gpu_count"], 1);
    assert_eq!(cfg["docker_registry"], "myreg.io");
    assert_eq!(cfg["container_runtime"], "docker");
    assert_eq!(cfg["run_config"]["key"], "val");
    assert_eq!(cfg["exec_config"]["cmd"], true);
    assert_eq!(cfg["volume_mappings"]["/h"], "/c");
    assert_eq!(cfg["persistent_storage_path"], "/data");
    assert!(cfg["max_run_seconds"].is_null());
    assert_eq!(cfg["timeout_action"], "remove");
    assert_eq!(cfg["network_bandwidth_up_mbps"], 0);
    assert_eq!(cfg["network_bandwidth_down_mbps"], 0);
    assert_eq!(cfg["instance_count"], 0);
    assert!(cfg["owner_id"].is_string());
    assert!(cfg["created_at"].is_string());
    assert!(cfg["updated_at"].is_string());
}

#[tokio::test]
async fn test_get_template_includes_instance_count() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "count-config",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    ctx.post("/api/instances", &serde_json::json!({
        "template_id": template_id
    })).await;
    ctx.post("/api/instances", &serde_json::json!({
        "template_id": template_id
    })).await;

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["instance_count"], 2);
}

#[tokio::test]
async fn test_create_template_with_container_runtime() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "runtime-test",
        "image": "busybox:1",
        "container_runtime": "runsc"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["container_runtime"], "runsc");

    let template_id = body["template"]["id"].as_str().unwrap();
    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["container_runtime"], "runsc");
}

#[tokio::test]
async fn test_create_template_default_container_runtime() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "default-runtime",
        "image": "busybox:1"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["container_runtime"], "docker");
}

#[tokio::test]
async fn test_update_template_container_runtime() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "runtime-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "runtime-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "container_runtime": "runsc"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["container_runtime"], "runsc");

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["container_runtime"], "runsc");
}

#[tokio::test]
async fn test_template_response_includes_container_runtime() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "json-runtime",
        "image": "busybox:1",
        "container_runtime": "runsc"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["template"].get("container_runtime").is_some());
}

#[tokio::test]
async fn test_create_template_with_docker_in_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "dini-config",
        "image": "busybox:1",
        "container_runtime": "runsc",
        "docker_in_instance": true
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["docker_in_instance"], true);

    let template_id = body["template"]["id"].as_str().unwrap();
    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["docker_in_instance"], true);
}

#[tokio::test]
async fn test_create_template_default_docker_in_instance_false() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "dini-default",
        "image": "busybox:1"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["docker_in_instance"], false);

    let template_id = body["template"]["id"].as_str().unwrap();
    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["docker_in_instance"], false);
}

#[tokio::test]
async fn test_update_template_docker_in_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "dini-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();
    assert_eq!(body["template"]["docker_in_instance"], false);

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "dini-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "container_runtime": "runsc",
        "docker_in_instance": true
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["docker_in_instance"], true);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["docker_in_instance"], true);
}

#[tokio::test]
async fn test_template_response_includes_docker_in_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "json-dini",
        "image": "busybox:1",
        "docker_in_instance": true
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["template"].get("docker_in_instance").is_some());
}

#[tokio::test]
async fn test_create_template_with_auto_sleep() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "auto-sleep-config",
        "image": "busybox:1",
        "max_run_seconds": 3600,
        "timeout_action": "stop"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["max_run_seconds"], 3600);
    assert_eq!(body["template"]["timeout_action"], "stop");

    let template_id = body["template"]["id"].as_str().unwrap();
    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["max_run_seconds"], 3600);
    assert_eq!(body["template"]["timeout_action"], "stop");
}

#[tokio::test]
async fn test_create_template_default_auto_sleep_disabled() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "no-auto-sleep-config",
        "image": "busybox:1"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["template"]["max_run_seconds"].is_null());
    assert_eq!(body["template"]["timeout_action"], "remove");
}

#[tokio::test]
async fn test_create_template_rejects_max_run_seconds_below_minimum() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "too-short-config",
        "image": "busybox:1",
        "max_run_seconds": 59
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_template_rejects_invalid_timeout_action() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "bad-action-config",
        "image": "busybox:1",
        "max_run_seconds": 3600,
        "timeout_action": "explode"
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_update_template_auto_sleep() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "auto-sleep-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "auto-sleep-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "max_run_seconds": 7200,
        "timeout_action": "pause"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["max_run_seconds"], 7200);
    assert_eq!(body["template"]["timeout_action"], "pause");

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "auto-sleep-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "max_run_seconds": null,
        "timeout_action": "remove"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["template"]["max_run_seconds"].is_null());
    assert_eq!(body["template"]["timeout_action"], "remove");
}

#[tokio::test]
async fn test_update_template_rejects_invalid_auto_sleep() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "auto-sleep-bad-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "auto-sleep-bad-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "max_run_seconds": 30
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_template_with_keep_time() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "keep-time-config",
        "image": "busybox:1",
        "keep_time_seconds": 3600,
        "keep_time_action": "stop"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["keep_time_seconds"], 3600);
    assert_eq!(body["template"]["keep_time_action"], "stop");

    let template_id = body["template"]["id"].as_str().unwrap();
    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["keep_time_seconds"], 3600);
    assert_eq!(body["template"]["keep_time_action"], "stop");
}

#[tokio::test]
async fn test_create_template_default_keep_time_disabled() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "no-keep-time-config",
        "image": "busybox:1"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["template"]["keep_time_seconds"].is_null());
    assert_eq!(body["template"]["keep_time_action"], "pause");
}

#[tokio::test]
async fn test_update_template_keep_time() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "keep-time-update",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "keep-time-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "keep_time_seconds": 7200,
        "keep_time_action": "pause"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["keep_time_seconds"], 7200);
    assert_eq!(body["template"]["keep_time_action"], "pause");

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "keep-time-update",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "keep_time_seconds": null,
        "keep_time_action": "remove"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["template"]["keep_time_seconds"].is_null());
    assert_eq!(body["template"]["keep_time_action"], "remove");
}

#[tokio::test]
async fn test_create_template_rejects_keep_time_below_minimum() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "keep-time-short-config",
        "image": "busybox:1",
        "keep_time_seconds": 30
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_create_template_rejects_invalid_keep_time_action() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "keep-time-bad-action-config",
        "image": "busybox:1",
        "keep_time_seconds": 3600,
        "keep_time_action": "bogus"
    })).await;
    assert_eq!(resp.status(), 400);
}

// ── allocation_mode ──────────────────────────────────────────

async fn create_manager(ctx: &TestContext) -> String {
    ctx.post("/api/users", &serde_json::json!({
        "username": "alloc_manager",
        "password": "pass123",
        "role": "manager"
    })).await;
    ctx.login_user("alloc_manager", "pass123").await;
    "alloc_manager".to_string()
}

#[tokio::test]
async fn test_create_template_default_allocation_mode_shared() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "alloc-default",
        "image": "busybox:1"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "shared");
}

#[tokio::test]
async fn test_create_template_with_allocation_mode_dedicated_as_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "alloc-dedicated",
        "image": "busybox:1",
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "dedicated");
}

#[tokio::test]
async fn test_create_template_rejects_invalid_allocation_mode() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "alloc-bogus",
        "image": "busybox:1",
        "allocation_mode": "fancy"
    })).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_manager_create_shared_allowed_dedicated_forbidden() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    create_manager(&ctx).await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "mgr-shared",
        "image": "busybox:1",
        "allocation_mode": "shared"
    })).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "shared");

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "mgr-dedicated",
        "image": "busybox:1",
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_manager_update_dedicated_forbidden() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "mgr-upd",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    create_manager(&ctx).await;

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "mgr-upd",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 403);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "shared");
}

#[tokio::test]
async fn test_update_without_allocation_mode_preserves_existing_mode() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "preserve-mode",
        "image": "busybox:1",
        "allocation_mode": "dedicated"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    create_manager(&ctx).await;

    // Manager edits a dedicated template without sending `allocation_mode`:
    // the mode must be preserved, not silently reset to the shared default.
    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "preserve-mode-renamed",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {}
    })).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "dedicated");
    assert_eq!(body["template"]["name"], "preserve-mode-renamed");
}

async fn insert_instance_with_status(ctx: &TestContext, template_id: &str, status: &str) {
    use openworkspace_api::db::{WorkspaceInstanceRepository, UserRepository};
    use sea_orm::Database;

    let db = Database::connect(common::pg_url(&ctx.db_name))
        .await
        .expect("failed to connect to test db");

    let admin = UserRepository::new(&db)
        .find_by_username("admin")
        .await
        .expect("failed to find admin")
        .expect("admin missing");
    let admin_id = admin.id;

    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let instance = instance_repo
        .launch(
            uuid::Uuid::parse_str(template_id).unwrap(),
            admin_id,
            "alloc-instance",
            false,
            None,
        )
        .await
        .expect("failed to launch test instance");
    instance_repo
        .update_status(instance.id, status)
        .await
        .expect("failed to set instance status");
}

#[tokio::test]
async fn test_update_allocation_mode_blocked_by_active_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "guard-active",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    insert_instance_with_status(&ctx, &template_id, "running").await;

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "guard-active",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 409);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "shared");
}

#[tokio::test]
async fn test_update_allocation_mode_succeeds_once_no_active_instances() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "guard-clear",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    insert_instance_with_status(&ctx, &template_id, "running").await;

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "guard-clear",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 409);

    let db = sea_orm::Database::connect(common::pg_url(&ctx.db_name))
        .await
        .expect("failed to connect to test db");
    let instance_repo = openworkspace_api::db::WorkspaceInstanceRepository::new(&db);
    let instances = instance_repo
        .list_by_template(uuid::Uuid::parse_str(&template_id).unwrap())
        .await
        .expect("failed to list instances");
    assert_eq!(instances.len(), 1);
    instance_repo
        .update_status(instances[0].id, "stopped")
        .await
        .expect("failed to stop instance");

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "guard-clear",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "dedicated");
}

#[tokio::test]
async fn test_update_allocation_mode_allowed_when_no_active_instances() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "guard-none",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "guard-none",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "allocation_mode": "dedicated"
    })).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "dedicated");
}

#[tokio::test]
async fn test_update_allocation_mode_unchanged_allowed_with_active_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "guard-same",
        "image": "busybox:1"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    insert_instance_with_status(&ctx, &template_id, "paused").await;

    let resp = ctx.put(&format!("/api/templates/{}", template_id), &serde_json::json!({
        "name": "guard-same",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {},
        "allocation_mode": "shared"
    })).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_list_and_get_include_allocation_mode() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/templates", &serde_json::json!({
        "name": "alloc-list",
        "image": "busybox:1",
        "allocation_mode": "dedicated"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let template_id = body["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.get("/api/templates").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let configs = body["templates"].as_array().unwrap();
    let found = configs
        .iter()
        .find(|c| c["id"] == template_id)
        .expect("template should be in list");
    assert_eq!(found["allocation_mode"], "dedicated");

    let resp = ctx.get(&format!("/api/templates/{}", template_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"]["allocation_mode"], "dedicated");
}

