use bollard::container::{RemoveContainerOptions, StopContainerOptions};
use bollard::Docker;

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


