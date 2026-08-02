#![cfg(feature = "docker")]

mod common;

use common::ensure_network;
use futures_util::stream::TryStreamExt;
use openworkspace_api::docker::{DockerClient, DockerService, RemoteType};

async fn setup() -> DockerClient {
    ensure_network().await;
    DockerClient::with_network("ow-test").await.expect("Docker not available")
}

/// Detect once whether the Docker daemon can start resource-constrained
/// (cores/memory) containers. In some containerized CI environments the daemon
/// has no usable cgroup v2 controller access and such containers fail at start
/// with a cgroup error; tests that need resource limits skip in that case.
async fn cgroup_supported() -> bool {
    static SUPPORTED: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();
    *SUPPORTED
        .get_or_init(|| async {
            let docker = match bollard::Docker::connect_with_local_defaults() {
                Ok(d) => d,
                Err(_) => return false,
            };
            let name = format!("ow_test_cgroup_probe_{}", std::process::id());
            let config = bollard::container::Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["true"]),
                host_config: Some(bollard::models::HostConfig {
                    memory: Some(64 * 1024 * 1024),
                    ..Default::default()
                }),
                ..Default::default()
            };
            match docker
                .create_container(
                    Some(bollard::container::CreateContainerOptions {
                        name: name.clone(),
                        ..Default::default()
                    }),
                    config,
                )
                .await
            {
                Ok(container) => {
                    let started = docker
                        .start_container(
                            &container.id,
                            None::<bollard::container::StartContainerOptions<String>>,
                        )
                        .await;
                    let _ = docker
                        .remove_container(
                            &container.id,
                            Some(bollard::container::RemoveContainerOptions {
                                v: true,
                                force: true,
                                link: false,
                            }),
                        )
                        .await;
                    match started {
                        Ok(()) => true,
                        Err(e) => !e.to_string().to_lowercase().contains("cgroup"),
                    }
                }
                Err(_) => false,
            }
        })
        .await
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
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
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
async fn test_create_container_publishes_host_port() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;

    // Grab a currently-free host port (the bind IP 127.0.0.1 keeps the test
    // independent of the host's gateway IP).
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host_port = probe.local_addr().unwrap().port();
    drop(probe);

    let name = format!("ow_test_docker_hostport_{}_{}", std::process::id(), host_port);

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: Some(host_port),
        host_gateway_ip: Some("127.0.0.1".to_string()),
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();

    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let inspect = docker.inspect_container(&id, None).await.unwrap();
    let _ = client.remove_container_by_id(&id).await;

    let ports = inspect
        .network_settings
        .and_then(|ns| ns.ports)
        .unwrap_or_default();
    let entry = ports
        .get("6901/tcp")
        .expect("expected a published 6901/tcp binding");
    let binding = entry
        .iter()
        .flatten()
        .next()
        .expect("expected a host-side binding");
    assert_eq!(binding.host_ip.as_deref(), Some("127.0.0.1"));
    assert_eq!(binding.host_port.as_deref(), Some(host_port.to_string().as_str()));
}

#[tokio::test]
async fn test_create_container_from_template() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_template_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_template_with_env_and_dns() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_template_env_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({
            "environment": ["MY_VAR=hello", "OTHER=world"],
            "dns": ["8.8.8.8"],
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 2, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_template_with_volume() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_template_vol_{}", std::process::id());
    let volume_name = format!("ow_test_vol_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({
            "volume_mappings": { "/tmp/ow_test": "/container/data" },
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({ "/tmp/ow_test": "/container/data" }),
        persistent_volume_name: Some(volume_name.clone()),
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 3, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));

    let _ = client.remove_container_by_id(&id).await;
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let _ = docker.remove_volume(&volume_name, None::<bollard::volume::RemoveVolumeOptions>).await;
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
async fn test_create_container_from_template_with_exec() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_exec_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({
            "post_start": { "cmd": "echo hello" }
        }),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_template_with_hostname() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_hostname_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({
            "hostname": "my-test-host"
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn test_create_container_from_template_command_from_run_config() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_run_cmd_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({
            "command": ["sleep", "3600"]
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: None,
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_create_container_from_template_no_command() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_no_cmd_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: None,
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let _result = client.create_container_from_template(&name, 1, &config, "test_password", "").await;
}

#[tokio::test]
async fn test_create_container_from_template_with_shm_size_and_network_mode() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_shm_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({
            "shm_size": 67108864,
        }),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();
    assert!(!id.is_empty());
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
async fn test_create_container_from_template_with_gpu() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_gpu_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 1,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let result = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
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
async fn test_create_container_from_template_image_already_cached() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;

    let name1 = format!("ow_test_docker_cached1_{}", std::process::id());
    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let _id1 = client
        .create_container_from_template(&name1, 1, &config, "test_password", "")
        .await
        .unwrap();

    let name2 = format!("ow_test_docker_cached2_{}", std::process::id());
    let id2 = client
        .create_container_from_template(&name2, 2, &config, "test_password", "")
        .await
        .unwrap();

    assert!(!id2.is_empty());
}

#[tokio::test]
async fn test_create_container_from_template_cores_and_memory() {
    use openworkspace_api::docker::ContainerConfig;

    if !cgroup_supported().await {
        eprintln!(
            "SKIP: Docker daemon lacks cgroup permission — resource-constrained container not tested"
        );
        return;
    }

    let client = setup().await;
    let name = format!("ow_test_docker_res_{}", std::process::id());

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 2,
        memory: 536870912,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
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
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();

    let state = client.inspect_container_state(&id).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_persistent_volume_lifecycle_via_client() {
    use openworkspace_api::docker::ContainerConfig;
    use std::fs;

    let client = setup().await;
    let host_dir = std::env::temp_dir().join(format!("ow_test_pv_lc_{}", std::process::id()));
    let host_path = host_dir.to_str().unwrap().to_string();
    let volume_name = format!("ow-test-lifecycle-{}", std::process::id());

    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();
    assert!(host_dir.exists(), "prepare must create the host data dir");

    let config = ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: Some(volume_name.clone()),
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
    };

    let name1 = format!("ow_test_pv_lc1_{}", std::process::id());
    let id1 = client
        .create_container_from_template(&name1, 1, &config, "test_password", "")
        .await
        .unwrap();
    let state = client.inspect_container_state(&id1).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));

    // Simulate the user writing data into their home (the volume's backing dir).
    fs::write(host_dir.join("user_data.txt"), "hello").unwrap();

    let _ = client.remove_container_by_id(&id1).await;
    let state = client.inspect_container_state(&id1).await;
    assert!(state.is_err(), "container should be gone");

    // Recreate with the same volume: the written data must persist.
    let name2 = format!("ow_test_pv_lc2_{}", std::process::id());
    let id2 = client
        .create_container_from_template(&name2, 1, &config, "test_password", "")
        .await
        .unwrap();
    let state = client.inspect_container_state(&id2).await.unwrap();
    assert_eq!(state.as_deref(), Some("running"));
    assert!(
        host_dir.join("user_data.txt").exists(),
        "user data must survive container recreation"
    );

    let _ = client.remove_container_by_id(&id2).await;

    client.remove_persistent_volume(&host_path, &volume_name).await.unwrap();
    assert_eq!(
        fs::read_dir(&host_dir).unwrap().count(),
        0,
        "host data dir should be emptied on remove"
    );
    fs::remove_dir_all(&host_dir).ok();
}

#[tokio::test]
async fn test_prepare_persistent_volume_reuses_existing_volume() {
    use std::fs;

    let client = setup().await;
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let host_dir = std::env::temp_dir().join(format!("ow_test_pv_reuse_{}", std::process::id()));
    let host_path = host_dir.to_str().unwrap().to_string();
    let volume_name = format!("ow-test-reuse-{}", std::process::id());

    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();
    fs::write(host_dir.join("user_data.txt"), "hello").unwrap();

    // A second prepare (e.g. re-launch after the instance was deleted and its
    // data kept) must succeed and reuse the existing Volume without touching
    // the data.
    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();
    assert!(
        docker.inspect_volume(&volume_name).await.is_ok(),
        "prepare must leave the existing volume declaration intact"
    );
    assert_eq!(
        fs::read_to_string(host_dir.join("user_data.txt")).unwrap(),
        "hello",
        "prepare must not wipe preserved data when the volume already exists"
    );

    client.remove_persistent_volume(&host_path, &volume_name).await.unwrap();
    fs::remove_dir_all(&host_dir).ok();
}

#[tokio::test]
async fn test_ensure_persistent_volume_redeclares_lost_volume() {
    use bollard::volume::RemoveVolumeOptions;
    use std::fs;

    let client = setup().await;
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let host_dir = std::env::temp_dir().join(format!("ow_test_pv_ensure_{}", std::process::id()));
    let host_path = host_dir.to_str().unwrap().to_string();
    let volume_name = format!("ow-test-ensure-{}", std::process::id());

    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();
    fs::write(host_dir.join("user_data.txt"), "hello").unwrap();

    // Simulate the Volume declaration being lost (e.g. `docker volume prune`).
    docker
        .remove_volume(&volume_name, None::<RemoveVolumeOptions>)
        .await
        .unwrap();
    assert!(
        docker.inspect_volume(&volume_name).await.is_err(),
        "volume declaration must be gone"
    );

    // Restart path: ensure re-declares the local-bind Volume without touching data.
    client.ensure_persistent_volume(&host_path, &volume_name).await.unwrap();
    assert!(
        docker.inspect_volume(&volume_name).await.is_ok(),
        "ensure must re-create the volume declaration"
    );
    assert!(
        host_dir.join("user_data.txt").exists(),
        "ensure must not touch the host data"
    );

    client.remove_persistent_volume(&host_path, &volume_name).await.unwrap();
    fs::remove_dir_all(&host_dir).ok();
}

#[tokio::test]
async fn test_local_bind_named_volume_copy_up_populates_image_files() {
    use bollard::container::{Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions};
    use std::fs;

    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let host_dir = std::env::temp_dir().join(format!("ow_test_pv_cu_{}", std::process::id()));
    let host_path = host_dir.to_str().unwrap().to_string();
    let volume_name = format!("ow-test-copyup-{}", std::process::id());

    let client = setup().await;
    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();

    // First mount at a directory the image ships content for (/etc): Docker's
    // copy-up must populate the empty local-bind volume with the image files.
    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("ow_test_pv_cu1_{}", std::process::id()),
                ..Default::default()
            }),
            Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["sleep", "3600"]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{}:/etc", volume_name)]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    docker
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();

    assert!(
        host_dir.join("passwd").exists(),
        "copy-up should populate /etc/passwd from the image into the volume"
    );

    docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptions { v: true, force: true, link: false }),
        )
        .await
        .unwrap();

    // A user file must survive container recreation, and the volume must not
    // be re-populated (no clobber of existing content). Docker's copy-up chowns
    // the volume root to the image dir's owner (root for /etc), so write the
    // marker via a one-shot root container rather than the host test process.
    let writer = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("ow_test_pv_cu_write_{}", std::process::id()),
                ..Default::default()
            }),
            Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["sh", "-c", "echo persisted > /etc/user_marker.txt"]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{}:/etc", volume_name)]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    docker
        .start_container(&writer.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
    docker
        .wait_container(&writer.id, None::<bollard::container::WaitContainerOptions<String>>)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    docker
        .remove_container(
            &writer.id,
            Some(RemoveContainerOptions { v: true, force: true, link: false }),
        )
        .await
        .unwrap();

    let container2 = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("ow_test_pv_cu2_{}", std::process::id()),
                ..Default::default()
            }),
            Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["sleep", "3600"]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{}:/etc", volume_name)]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    docker
        .start_container(&container2.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();

    assert!(
        host_dir.join("passwd").exists(),
        "existing volume content must not be replaced on re-mount"
    );
    assert!(
        host_dir.join("user_marker.txt").exists(),
        "user data must survive container recreation"
    );
    docker
        .remove_container(
            &container2.id,
            Some(RemoveContainerOptions { v: true, force: true, link: false }),
        )
        .await
        .unwrap();

    client.remove_persistent_volume(&host_path, &volume_name).await.unwrap();
    assert_eq!(fs::read_dir(&host_dir).unwrap().count(), 0);
    let gone = docker.inspect_volume(&volume_name).await;
    assert!(gone.is_err(), "volume declaration should be removed");

    fs::remove_dir_all(&host_dir).ok();
}

#[tokio::test]
async fn test_reset_repopulates_image_files_on_next_mount() {
    use bollard::container::{Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions};
    use std::fs;

    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let host_dir = std::env::temp_dir().join(format!("ow_test_pv_rs_{}", std::process::id()));
    let host_path = host_dir.to_str().unwrap().to_string();
    let volume_name = format!("ow-test-reset-{}", std::process::id());

    let client = setup().await;
    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();

    // First mount populates the empty volume from the image's built-in files.
    let first = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("ow_test_pv_rs1_{}", std::process::id()),
                ..Default::default()
            }),
            Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["sleep", "3600"]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{}:/etc", volume_name)]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    docker
        .start_container(&first.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
    assert!(
        host_dir.join("passwd").exists(),
        "first mount should copy the image's /etc/passwd into the volume"
    );
    // Simulate user data written into the home. Copy-up chowns the volume root
    // to root (the image dir's owner), so write via a one-shot root container
    // rather than the host test process.
    let writer = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("ow_test_pv_rs_write_{}", std::process::id()),
                ..Default::default()
            }),
            Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["sh", "-c", "echo hello > /etc/user_data.txt"]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{}:/etc", volume_name)]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    docker
        .start_container(&writer.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
    docker
        .wait_container(&writer.id, None::<bollard::container::WaitContainerOptions<String>>)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    docker
        .remove_container(
            &writer.id,
            Some(RemoveContainerOptions { v: true, force: true, link: false }),
        )
        .await
        .unwrap();
    assert!(
        host_dir.join("user_data.txt").exists(),
        "user data must be written into the home"
    );
    docker
        .remove_container(
            &first.id,
            Some(RemoveContainerOptions { v: true, force: true, link: false }),
        )
        .await
        .unwrap();

    // Reset: empty the host dir and remove the volume declaration.
    client.remove_persistent_volume(&host_path, &volume_name).await.unwrap();
    assert_eq!(
        fs::read_dir(&host_dir).unwrap().count(),
        0,
        "reset must wipe the built-in files and the user data"
    );

    // Re-prepare, then mount again: the fresh empty volume must re-populate
    // the image's built-in files (and the stale user data must not return).
    client.prepare_persistent_volume(&host_path, &volume_name).await.unwrap();
    let second = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("ow_test_pv_rs2_{}", std::process::id()),
                ..Default::default()
            }),
            Config {
                image: Some("busybox:1"),
                cmd: Some(vec!["sleep", "3600"]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{}:/etc", volume_name)]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    docker
        .start_container(&second.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
    assert!(
        host_dir.join("passwd").exists(),
        "a fresh launch after reset must re-populate the image's built-in files"
    );
    assert!(
        !host_dir.join("user_data.txt").exists(),
        "reset must give a clean home with no stale user data"
    );

    docker
        .remove_container(
            &second.id,
            Some(RemoveContainerOptions { v: true, force: true, link: false }),
        )
        .await
        .unwrap();
    client.remove_persistent_volume(&host_path, &volume_name).await.unwrap();
    fs::remove_dir_all(&host_dir).ok();
}
