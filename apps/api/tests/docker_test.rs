#![cfg(feature = "docker")]

mod common;

use common::ensure_network;
use futures_util::stream::TryStreamExt;
use openworkspace_api::docker::{ContainerConfig, DockerClient, DockerService, RemoteType};

async fn setup() -> DockerClient {
    ensure_network().await;
    DockerClient::new().await.expect("Docker not available")
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
async fn test_network_create_list_remove_idempotent() {
    let client = setup().await;

    // Unique name + subnet per run so concurrent test binaries on the same host
    // can never collide. Subnet is a /30 (network .0, gateway .1, host .2).
    let suffix = uuid::Uuid::new_v4();
    let name = format!("ow-test-net-{}", &suffix.simple().to_string()[..12]);
    let a = suffix.as_bytes()[0];
    let third = (suffix.as_bytes()[1] % 64) * 4;
    let subnet = format!("10.200.{}.{}/30", a, third);
    let gateway = format!("10.200.{}.{}", a, third + 1);

    // Tolerate a leftover network from a crashed earlier run.
    let _ = client.remove_network(&name).await;

    let scenario = async {
        client.create_network(&name, &subnet, &gateway).await?;

        // Re-creating an existing network is success, not an error.
        client.create_network(&name, &subnet, &gateway).await?;

        let nets = client.list_networks().await?;
        let mine = nets
            .iter()
            .find(|n| n.name == name)
            .ok_or("created network missing from list_networks")?;
        if mine.subnet.as_deref() != Some(subnet.as_str()) {
            return Err(format!(
                "expected subnet {} for {}, got {:?}",
                subnet, name, mine.subnet
            ));
        }

        client.remove_network(&name).await?;

        // Removing an already-gone network is success, not an error.
        client.remove_network(&name).await?;

        let nets = client.list_networks().await?;
        if nets.iter().any(|n| n.name == name) {
            return Err(format!("network {} still listed after removal", name));
        }
        Ok(())
    }
    .await;

    // Always clean up, even when the scenario above failed.
    if let Err(e) = client.remove_network(&name).await {
        eprintln!("warning: failed to clean up test network {}: {}", name, e);
    }

    scenario.expect("network create/list/remove scenario should succeed");
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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

/// Detect whether the host Docker daemon has the `runsc` (gVisor) runtime
/// registered. Tests that verify the runtime pass-through skip when it is not.
async fn runsc_supported() -> bool {
    static SUPPORTED: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();
    *SUPPORTED
        .get_or_init(|| async {
            let docker = match bollard::Docker::connect_with_local_defaults() {
                Ok(d) => d,
                Err(_) => return false,
            };
            match docker.info().await {
                Ok(info) => info.runtimes.is_some_and(|r| r.contains_key("runsc")),
                Err(_) => false,
            }
        })
        .await
}

#[tokio::test]
async fn test_create_container_dini_off_keeps_hardened_defaults() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_dini_off_{}", std::process::id());

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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();

    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let inspect = docker.inspect_container(&id, None).await.unwrap();
    let state = client.inspect_container_state(&id).await.unwrap();
    let _ = client.remove_container_by_id(&id).await;

    assert_eq!(state.as_deref(), Some("running"));

    let host_config = inspect.host_config.as_ref().expect("expected host config");
    assert_eq!(host_config.privileged, Some(false));
    assert_eq!(
        host_config.cap_drop,
        Some(vec!["NET_RAW".to_string(), "NET_ADMIN".to_string()])
    );
    assert!(host_config.tmpfs.is_none());

    let env = inspect
        .config
        .as_ref()
        .and_then(|c| c.env.as_ref())
        .expect("expected env");
    assert!(!env.iter().any(|e| e.starts_with("OW_DOCKER_IN_INSTANCE")));
}

#[tokio::test]
async fn test_create_container_dini_on_applies_privileged_tmpfs_and_env() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;
    let name = format!("ow_test_docker_dini_on_{}", std::process::id());

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
        docker_in_instance: true,
        network_name: None,
        instance_dns: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();

    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let inspect = docker.inspect_container(&id, None).await.unwrap();
    let state = client.inspect_container_state(&id).await.unwrap();
    let _ = client.remove_container_by_id(&id).await;

    assert_eq!(state.as_deref(), Some("running"));

    let host_config = inspect.host_config.as_ref().expect("expected host config");
    assert_eq!(host_config.privileged, Some(true));
    assert!(host_config.cap_drop.is_none());
    let tmpfs = host_config.tmpfs.as_ref().expect("expected tmpfs");
    assert_eq!(tmpfs.get("/var/lib/docker"), Some(&"exec,mode=755".to_string()));

    let env = inspect
        .config
        .as_ref()
        .and_then(|c| c.env.as_ref())
        .expect("expected env");
    assert!(env.iter().any(|e| e == "OW_DOCKER_IN_INSTANCE=true"));
}

#[tokio::test]
async fn test_container_attaches_to_instance_network_with_ow_dns() {
    use openworkspace_api::docker::ContainerConfig;

    let client = setup().await;

    // Unique name + subnet per run so concurrent test binaries on the same host
    // can never collide. Subnet is a /30 (network .0, gateway .1, container .2).
    let suffix = uuid::Uuid::new_v4();
    let name = format!("ow-test-net-{}", &suffix.simple().to_string()[..12]);
    let container_name = format!(
        "ow_test_net_attach_{}",
        &suffix.simple().to_string()[..12]
    );
    let a = suffix.as_bytes()[0];
    let third = (suffix.as_bytes()[1] % 64) * 4;
    let subnet = format!("10.200.{}.{}/30", a, third);
    let gateway = format!("10.200.{}.{}", a, third + 1);
    let container_ip = format!("10.200.{}.{}", a, third + 2);

    // Tolerate a leftover network from a crashed earlier run.
    let _ = client.remove_network(&name).await;

    let mut created_id: Option<String> = None;
    let scenario = async {
        client.create_network(&name, &subnet, &gateway).await?;

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
            docker_in_instance: false,
            network_name: Some(name.clone()),
            instance_dns: Some("8.8.8.8,1.1.1.1".to_string()),
        };

        let id = client
            .create_container_from_template(&container_name, 1, &config, "test_password", "")
            .await?;
        created_id = Some(id.clone());

        let docker = bollard::Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;
        let inspect = docker
            .inspect_container(&id, None)
            .await
            .map_err(|e| e.to_string())?;

        // The container must be born on the instance network, not the default bridge.
        let host_config = inspect.host_config.as_ref().ok_or("expected host config")?;
        if host_config.network_mode.as_deref() != Some(name.as_str()) {
            return Err(format!(
                "expected network_mode {}, got {:?}",
                name, host_config.network_mode
            ));
        }

        // Docker assigns the /30's single usable IP (gateway .1, container .2).
        let networks = inspect
            .network_settings
            .as_ref()
            .and_then(|ns| ns.networks.as_ref())
            .ok_or("expected network_settings.networks")?;
        let net = networks
            .get(&name)
            .ok_or_else(|| format!("container not attached to network {}", name))?;
        if net.ip_address.as_deref() != Some(container_ip.as_str()) {
            return Err(format!(
                "expected container IP {}, got {:?}",
                container_ip, net.ip_address
            ));
        }

        // The instance's DNS resolvers land as OW_DNS for the entrypoint rewrite.
        let env = inspect
            .config
            .as_ref()
            .and_then(|c| c.env.as_ref())
            .ok_or("expected env")?;
        if !env.iter().any(|e| e == "OW_DNS=8.8.8.8,1.1.1.1") {
            return Err("expected OW_DNS=8.8.8.8,1.1.1.1 in the container env".to_string());
        }

        Ok(())
    }
    .await;

    // Always clean up, even when the scenario above failed: drop the container
    // first (a network with active endpoints cannot be removed), then the network.
    if let Some(id) = created_id {
        let _ = client.remove_container_by_id(&id).await;
    }
    if let Err(e) = client.remove_network(&name).await {
        eprintln!("warning: failed to clean up test network {}: {}", name, e);
    }

    scenario.expect("container should attach to the /30 instance network with OW_DNS");
}

#[tokio::test]
async fn test_create_container_runsc_runtime_passthrough() {
    use openworkspace_api::docker::ContainerConfig;

    if !runsc_supported().await {
        eprintln!("skipping: runsc runtime not registered on this host");
        return;
    }

    let client = setup().await;
    let name = format!("ow_test_docker_runsc_{}", std::process::id());

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
        runtime: Some("runsc".to_string()),
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
    };

    let id = client
        .create_container_from_template(&name, 1, &config, "test_password", "")
        .await
        .unwrap();

    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let inspect = docker.inspect_container(&id, None).await.unwrap();
    let state = client.inspect_container_state(&id).await.unwrap();
    let _ = client.remove_container_by_id(&id).await;

    assert_eq!(state.as_deref(), Some("running"));

    let host_config = inspect.host_config.as_ref().expect("expected host config");
    assert_eq!(host_config.runtime, Some("runsc".to_string()));
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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
        docker_in_instance: false,
        network_name: None,
        instance_dns: None,
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

// ─────────────────────────────────────────────────────────────────────────────
// Ticket 06 — Real-Docker isolation proof
//
// These tests prove, against the real Docker daemon, that the per-instance /30
// topology actually isolates tenants: a /30 bridge holds exactly one usable IP
// (network+2, gateway network+1), two separate /30 bridges are mutually
// unreachable while each keeps internet + its own gateway, and the OW_DNS
// resolv.conf rewrite restores resolution under runsc.
// ─────────────────────────────────────────────────────────────────────────────

/// Derive a fresh /30 instance subnet (and its gateway + single container IP)
/// from a random UUID: network `.0`, gateway `.1`, container `.2`. Uses the
/// `10.201.0.0/16` range — a different base from the `10.200.0.0/16` the other
/// tests in this file use — so concurrent test binaries cannot collide on a
/// subnet while they run in parallel.
fn random_30_subnet(seed: &uuid::Uuid) -> (String, String, String) {
    let bytes = seed.as_bytes();
    let a = bytes[0];
    let third = (bytes[1] % 64) * 4;
    (
        format!("10.201.{}.{}/30", a, third),
        format!("10.201.{}.{}", a, third + 1),
        format!("10.201.{}.{}", a, third + 2),
    )
}

/// A plain busybox container config for the isolation tests. With
/// `run_listener` the container's PID 1 runs a busybox `nc` listener on port
/// 5555 (so cross-network probes are symmetric), otherwise it just sleeps. Uses
/// the OW instance security profile (DinI off): NET_RAW/NET_ADMIN dropped, so
/// the probes must be TCP-based — exactly the real threat model.
fn isolation_container_config(network_name: &str, run_listener: bool) -> ContainerConfig {
    ContainerConfig {
        image: "busybox:1".to_string(),
        cores: 0,
        memory: 0,
        gpu_count: 0,
        remote_type: RemoteType::KasmVnc,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_volume_name: None,
        command: if run_listener {
            Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                "nc -l -p 5555 -s 0.0.0.0 & sleep 3600".to_string(),
            ])
        } else {
            Some(vec!["sleep".to_string(), "3600".to_string()])
        },
        runtime: None,
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        host_port: None,
        host_gateway_ip: None,
        docker_in_instance: false,
        network_name: Some(network_name.to_string()),
        instance_dns: None,
    }
}

/// Run `cmd` inside a running container via `docker exec` and return
/// `(exit_code, combined stdout+stderr)`.
async fn exec_cmd(
    docker: &bollard::Docker,
    container_id: &str,
    cmd: &[&str],
) -> Result<(i64, String), String> {
    use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
    use futures_util::StreamExt;

    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                cmd: Some(cmd.to_vec()),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("create_exec in {} failed: {}", container_id, e))?;

    let start = docker
        .start_exec(&exec.id, None::<StartExecOptions>)
        .await
        .map_err(|e| format!("start_exec failed: {}", e))?;

    let mut output = String::new();
    if let StartExecResults::Attached { output: stream, .. } = start {
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let log = item.map_err(|e| format!("exec stream error: {}", e))?;
            output.push_str(&String::from_utf8_lossy(&log.into_bytes()));
        }
    }

    // The daemon records the exec's exit code once the process has finished.
    let mut exit_code: Option<i64> = None;
    for _ in 0..10 {
        if let Ok(info) = docker.inspect_exec(&exec.id).await
            && let Some(code) = info.exit_code {
                exit_code = Some(code);
                break;
            }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    Ok((exit_code.unwrap_or(-1), output))
}

#[tokio::test]
async fn test_network_single_usable_ip() {
    let client = setup().await;

    let suffix = uuid::Uuid::new_v4();
    let name = format!("ow-iso-net-{}", &suffix.simple().to_string()[..12]);
    let container_name = format!("ow-iso-net-c-{}", &suffix.simple().to_string()[..12]);
    let (subnet, gateway, container_ip) = random_30_subnet(&suffix);

    // Tolerate a leftover network from a crashed earlier run.
    let _ = client.remove_network(&name).await;

    let mut created_id: Option<String> = None;
    let scenario = async {
        client.create_network(&name, &subnet, &gateway).await?;

        let config = isolation_container_config(&name, false);
        let id = client
            .create_container_from_template(&container_name, 1, &config, "test_password", "")
            .await?;
        created_id = Some(id.clone());

        let docker = bollard::Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;

        // The container holds exactly the /30's single usable IP (network+2).
        let inspect = docker
            .inspect_container(&id, None)
            .await
            .map_err(|e| e.to_string())?;
        let networks = inspect
            .network_settings
            .as_ref()
            .and_then(|ns| ns.networks.as_ref())
            .ok_or("expected network_settings.networks")?;
        let net = networks
            .get(&name)
            .ok_or_else(|| format!("container not attached to network {}", name))?;
        if net.ip_address.as_deref() != Some(container_ip.as_str()) {
            return Err(format!(
                "expected container IP {}, got {:?}",
                container_ip, net.ip_address
            ));
        }

        // The network carries exactly one container endpoint — the instance
        // itself — so no address in the block is a live peer of the instance.
        let net_inspect = docker
            .inspect_network(&name, None::<bollard::network::InspectNetworkOptions<String>>)
            .await
            .map_err(|e| e.to_string())?;
        let endpoints = net_inspect
            .containers
            .as_ref()
            .map(|c| c.len())
            .unwrap_or(0);
        if endpoints != 1 {
            return Err(format!(
                "expected exactly one container endpoint on network {}, got {}",
                name, endpoints
            ));
        }
        let endpoint = net_inspect
            .containers
            .as_ref()
            .and_then(|c| c.values().next())
            .ok_or("network reported no endpoints")?;
        let expected_endpoint_ip = format!("{}/30", container_ip);
        if endpoint.ipv4_address.as_deref() != Some(expected_endpoint_ip.as_str()) {
            return Err(format!(
                "expected the endpoint's IP to be {}, got {:?}",
                expected_endpoint_ip, endpoint.ipv4_address
            ));
        }

        // The gateway (network+1) is the only other endpoint in the /30 block.
        let gw = net_inspect
            .ipam
            .as_ref()
            .and_then(|ipam| ipam.config.as_ref())
            .and_then(|c| c.first())
            .and_then(|cfg| cfg.gateway.clone());
        if gw.as_deref() != Some(gateway.as_str()) {
            return Err(format!("expected network gateway {}, got {:?}", gateway, gw));
        }

        Ok(())
    }
    .await;

    // Always clean up, even when the scenario failed: drop the container first
    // (a network with active endpoints cannot be removed), then the network.
    if let Some(id) = created_id {
        let _ = client.remove_container_by_id(&id).await;
    }
    if let Err(e) = client.remove_network(&name).await {
        eprintln!("warning: failed to clean up test network {}: {}", name, e);
    }

    scenario.expect("a /30 instance network should carry exactly one usable IP");
}

#[tokio::test]
async fn test_two_networks_mutually_isolated() {
    let client = setup().await;

    let suffix_a = uuid::Uuid::new_v4();
    let suffix_b = uuid::Uuid::new_v4();
    let (subnet_a, gw_a, ip_a) = random_30_subnet(&suffix_a);
    let mut pair_b = random_30_subnet(&suffix_b);
    while pair_b.0 == subnet_a {
        pair_b = random_30_subnet(&uuid::Uuid::new_v4());
    }
    let (subnet_b, gw_b, ip_b) = pair_b;
    let name_a = format!("ow-iso-a-{}", &suffix_a.simple().to_string()[..12]);
    let name_b = format!("ow-iso-b-{}", &suffix_b.simple().to_string()[..12]);
    let cname_a = format!("ow-iso-a-c-{}", &suffix_a.simple().to_string()[..12]);
    let cname_b = format!("ow-iso-b-c-{}", &suffix_b.simple().to_string()[..12]);

    // Tolerate leftover networks from a crashed earlier run.
    let _ = client.remove_network(&name_a).await;
    let _ = client.remove_network(&name_b).await;

    let mut created_a: Option<String> = None;
    let mut created_b: Option<String> = None;
    let scenario = async {
        client.create_network(&name_a, &subnet_a, &gw_a).await?;
        client.create_network(&name_b, &subnet_b, &gw_b).await?;

        // Each container runs its own listener as PID 1 so the cross-network
        // probe is symmetric and unambiguous: reaching the listener yields exit
        // 0; an isolated /30 drops the SYN and nc times out (exit 1).
        let config_a = isolation_container_config(&name_a, true);
        let config_b = isolation_container_config(&name_b, true);
        let id_a = client
            .create_container_from_template(&cname_a, 1, &config_a, "test_password", "")
            .await?;
        created_a = Some(id_a.clone());
        let id_b = client
            .create_container_from_template(&cname_b, 2, &config_b, "test_password", "")
            .await?;
        created_b = Some(id_b.clone());

        let docker = bollard::Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;
        // Give both listeners a moment to bind before probing.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Positive control: each container can reach its OWN listener, proving
        // both the probe mechanism and the listeners are live (a dead listener
        // would make the cross-network failure checks vacuous).
        let probe_a_self = format!("nc -w 3 {} 5555", ip_a);
        let (code, out) =
            exec_cmd(&docker, &id_a, &["sh", "-c", probe_a_self.as_str()]).await?;
        if code != 0 {
            return Err(format!(
                "positive control failed: container A cannot reach its own listener: (exit {}) {}",
                code, out
            ));
        }
        let probe_b_self = format!("nc -w 3 {} 5555", ip_b);
        let (code, out) =
            exec_cmd(&docker, &id_b, &["sh", "-c", probe_b_self.as_str()]).await?;
        if code != 0 {
            return Err(format!(
                "positive control failed: container B cannot reach its own listener: (exit {}) {}",
                code, out
            ));
        }

        // Each container's default route runs through its own /30 gateway.
        let (code, out) = exec_cmd(&docker, &id_a, &["ip", "route"]).await?;
        if code != 0 || !out.contains(&format!("default via {}", gw_a)) {
            return Err(format!(
                "container A is not routed via its gateway {}: (exit {}) {}",
                gw_a, code, out
            ));
        }
        let (code, out) = exec_cmd(&docker, &id_b, &["ip", "route"]).await?;
        if code != 0 || !out.contains(&format!("default via {}", gw_b)) {
            return Err(format!(
                "container B is not routed via its gateway {}: (exit {}) {}",
                gw_b, code, out
            ));
        }

        // Each container reaches the internet through its own gateway (NAT).
        let (code, out) = exec_cmd(
            &docker,
            &id_a,
            &["sh", "-c", "wget -q -T 8 -O - http://example.com"],
        )
        .await?;
        if code != 0 || out.trim().is_empty() {
            return Err(format!(
                "container A cannot reach the internet: (exit {}) {}",
                code, out
            ));
        }
        let (code, out) = exec_cmd(
            &docker,
            &id_b,
            &["sh", "-c", "wget -q -T 8 -O - http://example.com"],
        )
        .await?;
        if code != 0 || out.trim().is_empty() {
            return Err(format!(
                "container B cannot reach the internet: (exit {}) {}",
                code, out
            ));
        }

        // Mutual isolation: neither container can reach the other's listener.
        let probe_ab = format!("nc -w 3 {} 5555", ip_a);
        let (code, out) = exec_cmd(&docker, &id_b, &["sh", "-c", probe_ab.as_str()]).await?;
        if code == 0 {
            return Err(format!(
                "isolation broken: container B reached container A's listener: {}",
                out
            ));
        }
        let probe_ba = format!("nc -w 3 {} 5555", ip_b);
        let (code, out) = exec_cmd(&docker, &id_a, &["sh", "-c", probe_ba.as_str()]).await?;
        if code == 0 {
            return Err(format!(
                "isolation broken: container A reached container B's listener: {}",
                out
            ));
        }

        Ok(())
    }
    .await;

    // Always clean up, even when the scenario failed: drop the containers
    // first (a network with active endpoints cannot be removed), then the
    // networks.
    if let Some(id) = created_a {
        let _ = client.remove_container_by_id(&id).await;
    }
    if let Some(id) = created_b {
        let _ = client.remove_container_by_id(&id).await;
    }
    for name in [&name_a, &name_b] {
        if let Err(e) = client.remove_network(name).await {
            eprintln!("warning: failed to clean up test network {}: {}", name, e);
        }
    }

    scenario.expect("two /30 networks with one container each must be mutually unreachable");
}

#[tokio::test]
async fn test_runsc_dns_rewrite_in_instance() {
    if !runsc_supported().await {
        eprintln!("skipping: runsc runtime not registered on this host");
        return;
    }

    let client = setup().await;
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();

    // The OW image owns the resolv.conf rewrite contract. If it is neither
    // present nor pullable, skip — the host smoke test (ticket 08) covers this
    // path end to end, and this suite must stay green on hosts without it.
    let image = "tsukisama9292/ow-kasmvnc-ubuntu:jammy";
    if docker.inspect_image(image).await.is_err() {
        match docker
            .create_image(
                Some(bollard::image::CreateImageOptions {
                    from_image: image,
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await
        {
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "skipping: OW image '{}' is not available locally and could not be pulled ({}); the smoke test covers this path",
                    image, e
                );
                return;
            }
        }
    }

    let suffix = uuid::Uuid::new_v4();
    let name = format!("ow-iso-runsc-{}", &suffix.simple().to_string()[..12]);
    let (subnet, gateway, _ip) = random_30_subnet(&suffix);

    // Tolerate a leftover network from a crashed earlier run.
    let _ = client.remove_network(&name).await;

    let mut created_id: Option<String> = None;
    let scenario = async {
        client.create_network(&name, &subnet, &gateway).await?;

        // Run the image's own apply-ow-dns.sh as PID 1 under runsc on the /30
        // bridge, then keep the container alive for the exec checks. A custom
        // entrypoint is deliberate: it proves the resolv.conf rewrite contract
        // without booting the full Kasm desktop, which the smoke test covers.
        let container = docker
            .create_container(
                Some(bollard::container::CreateContainerOptions {
                    name: format!("ow-iso-runsc-c-{}", &suffix.simple().to_string()[..12]),
                    ..Default::default()
                }),
                bollard::container::Config {
                    image: Some(image),
                    entrypoint: Some(vec![
                        "/bin/bash",
                        "-c",
                        "/usr/local/bin/apply-ow-dns.sh && exec sleep 3600",
                    ]),
                    env: Some(vec!["OW_DNS=8.8.8.8,1.1.1.1"]),
                    host_config: Some(bollard::models::HostConfig {
                        network_mode: Some(name.clone()),
                        runtime: Some("runsc".to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        created_id = Some(container.id.clone());
        docker
            .start_container(
                &container.id,
                None::<bollard::container::StartContainerOptions<String>>,
            )
            .await
            .map_err(|e| e.to_string())?;

        // The locally cached OW image may predate the DNS contract: build.sh
        // COPYs the script into the image, so a stale image lacks it. Skip
        // (rather than fail) in that case — the rewrite runs once the image is
        // rebuilt, and the live-host smoke script covers this path meanwhile.
        match exec_cmd(
            &docker,
            &container.id,
            &["test", "-x", "/usr/local/bin/apply-ow-dns.sh"],
        )
        .await
        {
            Ok((0, _)) => {}
            other => {
                eprintln!(
                    "skipping: OW image '{}' lacks /usr/local/bin/apply-ow-dns.sh (image predates the DNS contract; rebuild with docker/template_images/build.sh). exec returned: {:?}",
                    image, other
                );
                return Ok(());
            }
        }

        // The entrypoint rewrites /etc/resolv.conf before anything else; poll
        // until the rewritten resolvers are visible (tolerates slow startup).
        let mut resolv = String::new();
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Ok((0, out)) = exec_cmd(&docker, &container.id, &["cat", "/etc/resolv.conf"]).await
                && out.contains("8.8.8.8") && out.contains("1.1.1.1") {
                    resolv = out;
                    break;
                }
        }
        if resolv.is_empty() {
            return Err(
                "resolv.conf was not rewritten to the OW_DNS resolvers under runsc".to_string(),
            );
        }

        // With the rewritten resolvers in place, in-instance resolution works
        // (the embedded 127.0.0.11 resolver does not bind under runsc).
        let mut resolved = String::new();
        for _ in 0..10 {
            match exec_cmd(&docker, &container.id, &["getent", "hosts", "example.com"]).await {
                Ok((0, out)) if out.contains("example.com") => {
                    resolved = out;
                    break;
                }
                _ => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            }
        }
        if resolved.is_empty() {
            return Err(format!(
                "in-instance name resolution failed under runsc (resolv.conf: {}); getent hosts example.com never resolved",
                resolv
            ));
        }

        Ok(())
    }
    .await;

    // Always clean up, even when the scenario failed: drop the container first
    // (a network with active endpoints cannot be removed), then the network.
    if let Some(id) = created_id {
        let _ = client.remove_container_by_id(&id).await;
    }
    if let Err(e) = client.remove_network(&name).await {
        eprintln!("warning: failed to clean up test network {}: {}", name, e);
    }

    scenario.expect("the OW_DNS resolv.conf rewrite should restore resolution under runsc");
}
