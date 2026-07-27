#![cfg(feature = "docker")]

mod common;

use common::ensure_network;
use openworkspace_api::docker::{DockerClient, DockerService};

async fn setup() -> DockerClient {
    ensure_network().await;
    DockerClient::with_network("ow-test").await.expect("Docker not available")
}

#[tokio::test]
async fn test_list_containers_not_empty() {
    let client = setup().await;
    let containers = client.list_containers(true).await.unwrap();
    assert!(!containers.is_empty(), "expected at least one container (Docker daemon)");
}

#[tokio::test]
async fn test_create_start_stop_remove_lifecycle() {
    let client = setup().await;
    let name = format!("ow_test_docker_lifecycle_{}", std::process::id());

    let id = client.create_container(&name, "busybox:1").await.unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert!(state.is_some(), "container should have a state");

    let _ = client.stop_container_by_id(&id).await;
    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("exited"));
}

#[tokio::test]
async fn test_pause_unpause() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_pause_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();

    client.pause_container_by_id(&id).await.unwrap();
    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("paused"));

    client.unpause_container_by_id(&id).await.unwrap();
    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_get_container_ip() {
    let client = setup().await;
    let name = format!("ow_test_docker_ip_{}", std::process::id());

    let id = client.create_container(&name, "busybox:1").await.unwrap();

    match client.get_container_ip(&id, "ow-test").await {
        Ok(ip) => {
            assert!(!ip.is_empty());
            assert!(ip.starts_with("172.") || ip.starts_with("10.") || ip.starts_with("192.168."),
                "unexpected IP format: {}", ip);
        }
        Err(_) => {}
    }
}

#[tokio::test]
async fn test_create_container_from_config() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_config_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_config_with_env_and_dns() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_config_env_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({
            "environment": ["MY_VAR=hello", "OTHER=world"],
            "dns": ["8.8.8.8"],
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 2, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_config_with_volume() {
    use openworkspace_api::docker::ContainerConfig;
    use std::fs;

    let client = setup().await;
    let name = format!("ow_test_docker_config_vol_{}", std::process::id());
    let tmp_dir = std::env::temp_dir().join(format!("ow_test_vol_{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).unwrap();

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({
            "volume_mappings": { "/tmp/ow_test": "/container/data" },
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({ "/tmp/ow_test": "/container/data" }),
        persistent_volume: Some(tmp_dir.to_str().unwrap().to_string()),
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 3, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());

    fs::remove_dir_all(&tmp_dir).ok();
}

#[tokio::test]
async fn test_remove_nonexistent_container_returns_error() {
    let client = setup().await;
    let result = client.remove_container_by_id("nonexistent_container_id_12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stop_nonexistent_container_returns_error() {
    let client = setup().await;
    let result = client.stop_container_by_id("nonexistent_container_id_12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_container_from_config_with_exec() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_exec_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({
            "post_start": { "cmd": "echo hello" }
        }),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_config_with_hostname() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_hostname_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({
            "hostname": "my-test-host"
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn test_create_container_from_config_command_from_run_config() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_run_cmd_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({
            "command": ["sleep", "3600"]
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: None,
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_config_no_command() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_no_cmd_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: None,
    };

    let _result = client.create_container_from_config(&name, 1, &config, "test_password").await;
}

#[tokio::test]
async fn test_create_container_from_config_with_shm_size_and_network_mode() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_shm_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({
            "shm_size": 67108864,
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn test_get_container_ip_wrong_network() {
    let client = setup().await;
    let name = format!("ow_test_docker_ip_wrong_{}", std::process::id());

    let id = client.create_container(&name, "busybox:1").await.unwrap();

    let result = client.get_container_ip(&id, "nonexistent-network-12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_inspect_container_state_after_remove() {
    let client = setup().await;
    let name = format!("ow_test_docker_inspect_gone_{}", std::process::id());

    let id = client.create_container(&name, "busybox:1").await.unwrap();
    client.remove_container_by_id(&id).await.ok();

    let result = client.inspect_container_state(&id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_docker_client_new() {
    let client = DockerClient::new().await.expect("DockerClient::new() should connect");
    let containers = client.list_containers(true).await.unwrap();
    assert!(!containers.is_empty(), "expected at least one container");
}

#[tokio::test]
async fn test_create_container_from_config_with_gpu() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_gpu_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 1,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let result = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await;
    match result {
        Ok(id) => {
            assert!(!id.is_empty());
        }
        Err(e) => {
            assert!(e.contains("nvidia") || e.contains("device driver"),
                "unexpected error: {}", e);
        }
    }
}

#[tokio::test]
async fn test_create_container_from_config_image_already_cached() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;

    let name1 = format!("ow_test_docker_cached1_{}", std::process::id());
    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let _id1 = client
        .create_container_from_config(&name1, 1, &config, "test_password")
        .await
        .unwrap();

    let name2 = format!("ow_test_docker_cached2_{}", std::process::id());
    let id2 = client
        .create_container_from_config(&name2, 2, &config, "test_password")
        .await
        .unwrap();

    assert!(!id2.is_empty());
}

#[tokio::test]
async fn test_create_container_from_config_cores_and_memory() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_res_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 2,
        memory: 536870912,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn test_list_containers_all_false() {
    let client = setup().await;
    let _containers = client.list_containers(false).await.unwrap();
}

#[tokio::test]
async fn test_start_nonexistent_container_returns_error() {
    let client = setup().await;
    let result = client.start_container_by_id("nonexistent_container_id_12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pause_nonexistent_container_returns_error() {
    let client = setup().await;
    let result = client.pause_container_by_id("nonexistent_container_id_12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_unpause_nonexistent_container_returns_error() {
    let client = setup().await;
    let result = client.unpause_container_by_id("nonexistent_container_id_12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_inspect_container_state_running() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_inspect_running_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
    };

    let id = client
        .create_container_from_config(&name, 1, &config, "test_password")
        .await
        .unwrap();

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_get_container_ip_empty_container() {
    let client = setup().await;
    let result = client.get_container_ip("nonexistent_container_id_12345", "ow-test").await;
    assert!(result.is_err());
}
