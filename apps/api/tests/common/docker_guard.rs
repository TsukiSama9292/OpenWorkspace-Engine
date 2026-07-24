use bollard::container::{ListContainersOptions, RemoveContainerOptions, StopContainerOptions};
use bollard::Docker;

const TEST_NETWORK: &str = "ow-test";

#[allow(dead_code)]
pub struct DockerContainerGuard {
    container_id: String,
}

#[allow(dead_code)]
impl DockerContainerGuard {
    pub fn new(container_id: impl Into<String>) -> Self {
        Self {
            container_id: container_id.into(),
        }
    }

    pub fn id(&self) -> &str {
        &self.container_id
    }
}

impl Drop for DockerContainerGuard {
    fn drop(&mut self) {
        let id = self.container_id.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                cleanup_container_async(&id).await;
            });
        });
    }
}

#[allow(dead_code)]
pub async fn cleanup_container_async(container_id: &str) {
    if let Ok(docker) = Docker::connect_with_local_defaults() {
        let _ = docker
            .stop_container(container_id, None::<StopContainerOptions>)
            .await;
        let _ = docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
    }
}

/// Remove all containers (including stopped) connected to the test network via Docker API.
#[allow(dead_code)]
pub async fn cleanup_test_containers() {
    let Ok(docker) = Docker::connect_with_local_defaults() else {
        return;
    };

    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };

    let Ok(containers) = docker.list_containers(Some(options)).await else {
        return;
    };

    for container in &containers {
        let Some(id) = &container.id else {
            continue;
        };

        let Ok(info) = docker.inspect_container(id.as_str(), None).await else {
            continue;
        };

        let on_test_network = info
            .network_settings
            .as_ref()
            .and_then(|ns| ns.networks.as_ref())
            .map(|nets| nets.contains_key(TEST_NETWORK))
            .unwrap_or(false);

        if on_test_network {
            let _ = docker
                .stop_container(id.as_str(), None::<StopContainerOptions>)
                .await;
            let _ = docker
                .remove_container(
                    id.as_str(),
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }
    }
}
