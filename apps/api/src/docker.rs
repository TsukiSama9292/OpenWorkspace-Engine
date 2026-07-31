use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, UploadToContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::ContainerSummary;
use bollard::Docker;
use futures_util::stream::TryStreamExt;
use std::default::Default;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteType {
    KasmVnc,
    Ttyd,
    Jupyter,
}

impl RemoteType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteType::KasmVnc => "kasmvnc",
            RemoteType::Ttyd => "ttyd",
            RemoteType::Jupyter => "jupyter",
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            RemoteType::KasmVnc => 6901,
            RemoteType::Ttyd => 7681,
            RemoteType::Jupyter => 8888,
        }
    }
}

impl FromStr for RemoteType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "kasmvnc" => Ok(RemoteType::KasmVnc),
            "ttyd" => Ok(RemoteType::Ttyd),
            "jupyter" => Ok(RemoteType::Jupyter),
            _ => Err(format!("unknown remote_type: {}", s)),
        }
    }
}

const KASMVNC_YAML: &str = r#"network:
  ssl:
    pem_certificate: ${HOME}/.vnc/self.pem
    pem_key: ${HOME}/.vnc/self.pem
    require_ssl: false
  udp:
    public_ip: 127.0.0.1
runtime_configuration:
  allow_override_standard_vnc_server_settings: true
  allow_override_list:
    - pointer.enabled
server:
  allow_environment_variables_to_override_config_settings: true
"#;

/// Full configuration for creating a container from a workspace template.
pub struct ContainerConfig {
    pub image: String,
    pub cores: i32,
    pub memory: i64,
    pub gpu_count: i32,
    pub remote_type: RemoteType,
    pub run_config: serde_json::Value,
    pub exec_config: serde_json::Value,
    pub volume_mappings: serde_json::Value,
    pub persistent_volume: Option<String>,
    pub command: Option<Vec<String>>,
    pub runtime: Option<String>,
}

pub fn runtime_to_host_config(value: &str) -> Option<String> {
    match value {
        "" | "docker" => None,
        other => Some(other.to_string()),
    }
}

/// Trait for Docker operations, allowing mock implementations in tests.
#[async_trait::async_trait]
#[mockall::automock]
pub trait DockerService: Send + Sync {
    fn network_name(&self) -> &str;

    async fn list_containers(
        &self,
        all: bool,
    ) -> Result<Vec<ContainerSummary>, bollard::errors::Error>;

    async fn create_container(
        &self,
        name: &str,
        image: &str,
    ) -> Result<String, String>;

    async fn create_container_from_template(
        &self,
        container_name: &str,
        instance_number: i32,
        config: &ContainerConfig,
        password: &str,
        access_token: &str,
    ) -> Result<String, String>;

    async fn start_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error>;

    async fn stop_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error>;

    async fn remove_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error>;

    async fn inspect_container_state(
        &self,
        container_id: &str,
    ) -> Result<Option<String>, bollard::errors::Error>;

    async fn pause_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error>;

    async fn unpause_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error>;

    async fn get_container_ip(
        &self,
        container_id: &str,
        network_name: &str,
    ) -> Result<String, String>;
}

pub fn is_container_not_found(err: &bollard::errors::Error) -> bool {
    matches!(
        err,
        bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

pub async fn stop_and_remove_container(
    docker: &dyn DockerService,
    container_id: &str,
    instance_name: &str,
) {
    match docker.stop_container_by_id(container_id).await {
        Ok(()) => {}
        Err(ref e) if is_container_not_found(e) => {}
        Err(e) => tracing::warn!(
            "Failed to stop container for '{}': {} (proceeding with removal)",
            instance_name,
            e
        ),
    }

    match docker.remove_container_by_id(container_id).await {
        Ok(()) => tracing::info!("Container removed for instance '{}'", instance_name),
        Err(ref e) if is_container_not_found(e) => {
            tracing::info!("Container for '{}' already removed", instance_name);
        }
        Err(e) => tracing::warn!("Failed to remove container for '{}': {}", instance_name, e),
    }
}

pub struct DockerClient {
    docker: Docker,
    network_name: String,
}

impl DockerClient {
    pub async fn new() -> Result<Self, String> {
        Self::with_network("ow-network").await
    }

    pub async fn with_network(network_name: &str) -> Result<Self, String> {
        let docker = Docker::connect_with_local_defaults().map_err(|e| e.to_string())?;
        Ok(Self { docker, network_name: network_name.to_string() })
    }
}

#[async_trait::async_trait]
impl DockerService for DockerClient {
    fn network_name(&self) -> &str {
        &self.network_name
    }

    async fn list_containers(
        &self,
        all: bool,
    ) -> Result<Vec<ContainerSummary>, bollard::errors::Error> {
        let options = Some(ListContainersOptions::<String> {
            all,
            ..Default::default()
        });
        self.docker.list_containers(options).await
    }

    async fn create_container(
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

    /// Create a container from a full workspace template, applying all Docker settings.
    /// Returns container_id.
    async fn create_container_from_template(
        &self,
        container_name: &str,
        instance_number: i32,
        config: &ContainerConfig,
        password: &str,
        access_token: &str,
    ) -> Result<String, String> {
        let image = &config.image;

        if self.docker.inspect_image(image).await.is_err() {
            tracing::info!(
                "Pulling image '{}' for instance '{}' (#{})...",
                image,
                container_name,
                instance_number
            );

            self.docker
                .create_image(
                    Some(CreateImageOptions {
                        from_image: image.as_str(),
                        ..Default::default()
                    }),
                    None,
                    None,
                )
                .try_collect::<Vec<_>>()
                .await
                .map_err(|e| e.to_string())?;
        } else {
            tracing::debug!(
                "Image '{}' already cached, skipping pull for instance '{}' (#{})",
                image,
                container_name,
                instance_number
            );
        }

        // ── Build environment variables per remote_type ──
        let mut env: Vec<&str> = Vec::new();
        let mut owned_env: Vec<String> = Vec::new();

        match config.remote_type {
            RemoteType::KasmVnc => {
                env.push("KASM_VNC_PORT=6901");
                env.push("DISPLAY=:1");
                let pw_env = format!("VNC_PW={}", password);
                owned_env.push(pw_env);
            }
            RemoteType::Ttyd => {
                owned_env.push(format!("TTYD_USERNAME=ow_user"));
                owned_env.push(format!("TTYD_PASSWORD={}", password));
            }
            RemoteType::Jupyter => {
                owned_env.push(format!("JUPYTER_TOKEN={}", password));
                owned_env.push(format!("JUPYTER_BASE_URL=/jupyter/{}", access_token));
            }
        }

        // Push owned env strings as borrowed str
        for s in &owned_env {
            env.push(s);
        }

        if let Some(user_env) = config.run_config.get("environment").and_then(|v| v.as_array()) {
            for item in user_env {
                if let Some(s) = item.as_str() {
                    env.push(s);
                }
            }
        }

        // ── Exposed ports per remote_type ──
        let mut exposed_ports = std::collections::HashMap::new();
        match config.remote_type {
            RemoteType::KasmVnc => {
                exposed_ports.insert("6901/tcp", std::collections::HashMap::<(), ()>::new());
            }
            RemoteType::Ttyd => {
                exposed_ports.insert("7681/tcp", std::collections::HashMap::<(), ()>::new());
            }
            RemoteType::Jupyter => {
                exposed_ports.insert("8888/tcp", std::collections::HashMap::<(), ()>::new());
            }
        }

        // ── Build volume binds ──
        let mut binds = Vec::new();

        // Config volume_mappings
        if let Some(mappings) = config.volume_mappings.as_object() {
            for (host_path, container_path) in mappings {
                if let Some(container_path_str) = container_path.as_str() {
                    binds.push(format!("{}:{}:rw", host_path, container_path_str));
                }
            }
        }

        // Persistent storage volume
        if let Some(ref persistent_path) = config.persistent_volume {
            binds.push(format!("{}:/home/kasm_user/persistent:rw", persistent_path));
        }

        // ── DNS ──
        let dns: Option<Vec<String>> = config
            .run_config
            .get("dns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });

        // ── SHM size ──
        let shm_size: Option<i64> = config
            .run_config
            .get("shm_size")
            .and_then(|v| v.as_i64());

        // ── Hostname ──
        let hostname: Option<&str> = config
            .run_config
            .get("hostname")
            .and_then(|v| v.as_str());

        let host_config = bollard::models::HostConfig {
            privileged: Some(false),
            cap_drop: Some(vec!["NET_RAW".to_string(), "NET_ADMIN".to_string()]),
            nano_cpus: if config.cores > 0 {
                Some((config.cores as i64) * 1_000_000_000)
            } else {
                None
            },
            memory: if config.memory > 0 {
                Some(config.memory)
            } else {
                None
            },
            dns,
            shm_size,
            network_mode: Some(self.network_name().to_string()),
            binds: if binds.is_empty() { None } else { Some(binds) },
            device_requests: if config.gpu_count > 0 {
                Some(vec![bollard::models::DeviceRequest {
                    driver: Some("nvidia".to_string()),
                    count: Some(config.gpu_count as i64),
                    capabilities: Some(vec![vec!["gpu".to_string()]]),
                    ..Default::default()
                }])
            } else {
                None
            },
            runtime: config.runtime.as_deref().and_then(runtime_to_host_config),
            ..Default::default()
        };

        let cmd: Option<Vec<&str>> = config.command.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect())
            .or_else(|| {
                config
                    .run_config
                    .get("command")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect()
                    })
            });

        let container_config = Config {
            image: Some(image.as_str()),
            cmd,
            env: Some(env),
            exposed_ports: Some(exposed_ports),
            hostname,
            host_config: Some(host_config),
            ..Default::default()
        };

        let options = Some(CreateContainerOptions {
            name: container_name.to_string(),
            ..Default::default()
        });

        let container = self
            .docker
            .create_container(options, container_config)
            .await
            .map_err(|e| e.to_string())?;

        // ── Inject kasmvnc.yaml only for KasmVNC ──
        if config.remote_type == RemoteType::KasmVnc {
            tracing::info!("Injecting kasmvnc.yaml into container '{}'...", container_name);

            let mut header = tar::Header::new_gnu();
            header.set_size(KASMVNC_YAML.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            let mut tar_buf: Vec<u8> = Vec::new();
            {
                let mut ar = tar::Builder::new(&mut tar_buf);
                ar.append_data(
                    &mut header,
                    "etc/kasmvnc/kasmvnc.yaml",
                    KASMVNC_YAML.as_bytes(),
                )
                .map_err(|e| e.to_string())?;
                ar.finish().map_err(|e| e.to_string())?;
            }

            self.docker
                .upload_to_container(
                    &container.id,
                    Some(UploadToContainerOptions {
                        path: "/",
                        ..Default::default()
                    }),
                    tar_buf.into(),
                )
                .await
                .map_err(|e| format!("upload_to_container failed: {}", e))?;
        }

        tracing::info!("Starting container '{}'...", container_name);

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| e.to_string())?;

        // ── Exec post-start commands ──
        if let Some(commands) = config.exec_config.as_object() {
            for (name, cmd_obj) in commands {
                if let Some(cmd_str) = cmd_obj.get("cmd").and_then(|v| v.as_str()) {
                    tracing::info!(
                        "Executing post-start command '{}' in container '{}'...",
                        name,
                        container_name
                    );
                    let exec_options = bollard::exec::CreateExecOptions {
                        cmd: Some(vec!["bash", "-c", cmd_str]),
                        ..Default::default()
                    };
                    let exec = self
                        .docker
                        .create_exec(&container.id, exec_options)
                        .await;
                    match exec {
                        Ok(exec_output) => {
                            let start_result = self
                                .docker
                                .start_exec(
                                    &exec_output.id,
                                    None::<bollard::exec::StartExecOptions>,
                                )
                                .await;
                            if let Err(e) = start_result {
                                tracing::warn!(
                                    "Failed to execute command '{}' in container '{}': {}",
                                    name,
                                    container_name,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to create exec for command '{}' in container '{}': {}",
                                name,
                                container_name,
                                e
                            );
                        }
                    }
                }
            }
        }

        tracing::info!(
            "Container '{}' (#{}) started successfully (id: {})",
            container_name,
            instance_number,
            &container.id[..12]
        );

        Ok(container.id)
    }

    async fn start_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker
            .start_container(container_id, None::<StartContainerOptions<String>>)
            .await
    }

    async fn stop_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker
            .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
            .await
    }

    async fn remove_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    v: false,
                    force: true,
                    link: false,
                }),
            )
            .await
    }

    async fn inspect_container_state(
        &self,
        container_id: &str,
    ) -> Result<Option<String>, bollard::errors::Error> {
        match self.docker.inspect_container(container_id, None).await {
            Ok(info) => Ok(info.state.and_then(|s| s.status).map(|s| format!("{:?}", s).trim_matches('"').to_lowercase())),
            Err(e) => Err(e),
        }
    }

    async fn pause_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker.pause_container(container_id).await
    }

    async fn unpause_container_by_id(
        &self,
        container_id: &str,
    ) -> Result<(), bollard::errors::Error> {
        self.docker.unpause_container(container_id).await
    }

    async fn get_container_ip(
        &self,
        container_id: &str,
        network_name: &str,
    ) -> Result<String, String> {
        let info = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|e| format!("inspect failed: {}", e))?;

        info.network_settings
            .and_then(|ns| ns.networks)
            .and_then(|networks| networks.get(network_name).cloned())
            .and_then(|net| net.ip_address)
            .filter(|ip| !ip.is_empty())
            .ok_or_else(|| format!("no IP on network '{}' for container {}", network_name, &container_id[..12]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_container_not_found_matches_404() {
        let not_found = bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            message: "No such container".to_string(),
        };
        assert!(is_container_not_found(&not_found));
    }

    #[test]
    fn test_is_container_not_found_rejects_other_errors() {
        let conflict = bollard::errors::Error::DockerResponseServerError {
            status_code: 409,
            message: "container is running".to_string(),
        };
        assert!(!is_container_not_found(&conflict));

        let io = std::io::Error::new(std::io::ErrorKind::Other, "network down");
        let io_err: bollard::errors::Error = io.into();
        assert!(!is_container_not_found(&io_err));
    }

    #[test]
    fn test_runtime_to_host_config_docker_returns_none() {
        assert_eq!(runtime_to_host_config("docker"), None);
    }

    #[test]
    fn test_runtime_to_host_config_empty_returns_none() {
        assert_eq!(runtime_to_host_config(""), None);
    }

    #[test]
    fn test_runtime_to_host_config_runsc_returns_some() {
        assert_eq!(runtime_to_host_config("runsc"), Some("runsc".to_string()));
    }

    #[test]
    fn test_runtime_to_host_config_other_returns_some() {
        assert_eq!(runtime_to_host_config("kata"), Some("kata".to_string()));
    }

    #[test]
    fn test_runtime_to_host_config_nvidia_returns_some() {
        assert_eq!(runtime_to_host_config("nvidia"), Some("nvidia".to_string()));
    }
}
