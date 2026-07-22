use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::ContainerSummary;
use bollard::network::ConnectNetworkOptions;
use bollard::Docker;
use futures_util::stream::TryStreamExt;
use std::default::Default;

pub struct DockerClient {
    docker: Docker,
}

impl DockerClient {
    pub async fn new() -> Result<Self, String> {
        let docker = Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;
        Ok(Self { docker })
    }

    pub async fn list_containers(
        &self,
        all: bool,
    ) -> Result<Vec<ContainerSummary>, bollard::errors::Error> {
        let options = Some(ListContainersOptions::<String> {
            all,
            ..Default::default()
        });
        self.docker.list_containers(options).await
    }

    pub async fn create_container(
        &self,
        name: &str,
        image: &str,
    ) -> Result<String, String> {
        self.docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: image,
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| e.to_string())?;

        let config = Config {
            image: Some(image),
            ..Default::default()
        };

        let options = Some(CreateContainerOptions {
            name: name.to_string(),
            ..Default::default()
        });

        let container = self.docker.create_container(options, config).await.map_err(|e| e.to_string())?;

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| e.to_string())?;

        Ok(container.id)
    }

    pub async fn create_kasm_container(
        &self,
        name: &str,
        instance_number: i32,
    ) -> Result<String, String> {
        let image = "kasmweb/desktop:1.19.0-rolling-daily";
        let container_name = format!("ow-kasm-{}", instance_number);

        tracing::info!(
            "Pulling KasmVNC image '{}' for instance '{}' (#{})...",
            image,
            name,
            instance_number
        );

        self.docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: image,
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| e.to_string())?;

        let mut exposed_ports = std::collections::HashMap::new();
        exposed_ports.insert("6901/tcp", std::collections::HashMap::<(), ()>::new());

        let config = Config {
            image: Some(image),
            env: Some(vec![
                "VNCOPTIONS=-disableBasicAuth",
                "KASM_VNC_PORT=6901",
                "DISPLAY=:1",
            ]),
            exposed_ports: Some(exposed_ports),
            host_config: Some(bollard::models::HostConfig {
                privileged: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };

        let options = Some(CreateContainerOptions {
            name: container_name,
            ..Default::default()
        });

        let container = self.docker.create_container(options, config).await.map_err(|e| e.to_string())?;

        tracing::info!(
            "Connecting container '{}' (id: {}) to network 'openworkspace-engin'...",
            name,
            &container.id[..12]
        );

        self.docker
            .connect_network(
                "openworkspace-engin",
                ConnectNetworkOptions {
                    container: container.id.clone(),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        tracing::info!("Starting container '{}'...", name);

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| e.to_string())?;

        tracing::info!(
            "KasmVNC container '{}' (#{}) started successfully (id: {})",
            name,
            instance_number,
            &container.id[..12]
        );

        Ok(container.id)
    }

    pub async fn start_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker
            .start_container(container_id, None::<StartContainerOptions<String>>)
            .await
    }

    pub async fn stop_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker
            .stop_container(container_id, None::<StopContainerOptions>)
            .await
    }

    pub async fn remove_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker.remove_container(container_id, None).await
    }

    pub async fn inspect_container_state(
        &self,
        container_id: &str,
    ) -> Result<Option<String>, bollard::errors::Error> {
        match self.docker.inspect_container(container_id, None).await {
            Ok(info) => Ok(info.state.and_then(|s| s.status).map(|s| format!("{:?}", s))),
            Err(e) => Err(e),
        }
    }
}
