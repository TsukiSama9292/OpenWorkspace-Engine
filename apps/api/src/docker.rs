use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, UploadToContainerOptions, WaitContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::ContainerSummary;
use bollard::volume::{CreateVolumeOptions, RemoveVolumeOptions};
use bollard::Docker;
use futures_util::stream::TryStreamExt;
use std::collections::HashMap;
use std::default::Default;
use std::str::FromStr;
use uuid::Uuid;

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
    pub persistent_volume_name: Option<String>,
    pub command: Option<Vec<String>>,
    pub runtime: Option<String>,
    pub network_bandwidth_up_mbps: i32,
    pub network_bandwidth_down_mbps: i32,
    /// Allocated host port this instance's single service port is published to.
    /// `None` keeps the container on `ow-network` with no host binding.
    pub host_port: Option<u16>,
    /// Host Docker-bridge gateway IP to bind the published port to. Only used
    /// when `host_port` is set; callers normally pass the `OW_HOST_GATEWAY_IP`
    /// setting.
    pub host_gateway_ip: Option<String>,
    /// When set, the instance runs a Docker daemon inside it (DinI): the
    /// container is created `--privileged` with no capability drops, a `tmpfs`
    /// at `/var/lib/docker`, and `OW_DOCKER_IN_INSTANCE=true` in its
    /// environment. Under `runsc` this stays sandbox-confined; under `runc` it
    /// grants full host access (warned in the UI).
    pub docker_in_instance: bool,
}

/// Security posture derived from the DinI switch and container runtime. Rows:
///
/// | `docker_in_instance` | runtime | `privileged` | `cap_drop` | `/var/lib/docker` tmpfs | `OW_DOCKER_IN_INSTANCE` |
/// |---|---|---|---|---|---|
/// | off | any | false | `NET_RAW`, `NET_ADMIN` | none | absent |
/// | on  | `runsc` | true | none | `exec,mode=755` | `true` |
/// | on  | `runc`  | true | none | `exec,mode=755` | `true` |
///
/// The two "on" rows send the same Docker configuration; `runtime` is part of
/// the mapping because it determines *why* the elevated profile is acceptable
/// (`runsc` sandbox) or a UI-warned risk (`runc`).
#[derive(Debug, Clone, PartialEq)]
pub struct DiniSecurityProfile {
    pub privileged: bool,
    pub cap_drop: Option<Vec<String>>,
    pub tmpfs: Option<std::collections::HashMap<String, String>>,
    pub dind_env: Option<String>,
}

pub fn dini_security_profile(docker_in_instance: bool, _runtime: &str) -> DiniSecurityProfile {
    if !docker_in_instance {
        return DiniSecurityProfile {
            privileged: false,
            cap_drop: Some(vec!["NET_RAW".to_string(), "NET_ADMIN".to_string()]),
            tmpfs: None,
            dind_env: None,
        };
    }
    DiniSecurityProfile {
        privileged: true,
        cap_drop: None,
        tmpfs: Some(std::collections::HashMap::from([(
            "/var/lib/docker".to_string(),
            "exec,mode=755".to_string(),
        )])),
        dind_env: Some("OW_DOCKER_IN_INSTANCE=true".to_string()),
    }
}

pub fn runtime_to_host_config(value: &str) -> Option<String> {
    match value {
        "" | "docker" => None,
        other => Some(other.to_string()),
    }
}

/// Port binding map for publishing an instance's single service port to a
/// given host gateway IP / host port. One binding per `remote_type` container
/// port (KasmVNC `6901`, ttyd `7681`, Jupyter `8888`).
pub fn port_bindings_for(
    remote_type: &RemoteType,
    host_ip: &str,
    host_port: u16,
) -> HashMap<String, Vec<bollard::models::PortBinding>> {
    let mut bindings = HashMap::new();
    bindings.insert(
        format!("{}/tcp", remote_type.port()),
        vec![bollard::models::PortBinding {
            host_ip: Some(host_ip.to_string()),
            host_port: Some(host_port.to_string()),
        }],
    );
    bindings
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

    /// Apply per-instance network bandwidth limits (Mbps, `0` = unlimited).
    /// Egress on the container's `eth0` is shaped for upload; egress on the
    /// host-side veth (the container's ingress) is shaped for download.
    /// A fully unlimited request is a no-op.
    async fn apply_bandwidth_limit(
        &self,
        container_id: &str,
        up_mbps: i32,
        down_mbps: i32,
    ) -> Result<(), String>;

    /// Whether a client is currently connected to the instance's session port
    /// (e.g. the user's browser has the remote desktop / terminal / notebook
    /// open). Used by keep-time to avoid reclaiming an in-use session.
    async fn has_session_connection(
        &self,
        container_id: &str,
        port: u16,
    ) -> Result<bool, String>;

    /// Create a clean, empty host data directory for an Instance and declare a
    /// Local Bind-mounted Named Volume over it. The API runs in a container and
    /// cannot touch host files, so a short-lived `alpine` helper container
    /// `mkdir -p`s the host dir and `chown`s it to UID/GID 1000 (the uid of
    /// both `kasm-user` and `ow_user`); the volume is then declared with
    /// `driver=local`, `type=none` / `device=<host_path>` / `o=bind`. The first
    /// container to mount the empty volume gets Docker's copy-up of the image's
    /// built-in home files.
    async fn prepare_persistent_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String>;

    /// Tear down an Instance's persistent data: empty the host data directory
    /// (via an `alpine` helper container) and remove the Volume declaration, so
    /// no orphaned data or stale volume blocks a later re-populate. Used by
    /// `reset_persistent` (and wiping a broken `error` record); `delete` keeps
    /// the data for reuse.
    async fn remove_persistent_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String>;

    /// Recreate a lost Volume declaration for a persistent Instance on restart.
    /// The host data dir and volume name come from the stored
    /// `resolved_volume_host_path`, so only the local-bind Volume mapping is
    /// (re)declared if Docker no longer knows it — the data itself is already on
    /// the host and must not be re-populated.
    async fn ensure_persistent_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String>;
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

pub fn is_volume_not_found(err: &bollard::errors::Error) -> bool {
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

        let dini = dini_security_profile(
            config.docker_in_instance,
            config.runtime.as_deref().unwrap_or(""),
        );

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
        if let Some(dind_env) = dini.dind_env {
            owned_env.push(dind_env);
        }
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

        // Persistent storage volume: mount the named local-bind volume at the
        // per-remote-type home dir (whole home). Docker populates the volume
        // from the image's built-in home files on first (empty) mount.
        if let Some(ref volume_name) = config.persistent_volume_name {
            if let Some(target) = crate::persistent_volume::persistent_container_target(config.remote_type.as_str()) {
                binds.push(format!("{}:{}:rw", volume_name, target));
            }
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
        // KasmVNC/Chrome exhausts Docker's default 64MB /dev/shm, which crashes
        // the desktop browser with "No space left on device". Default to 512MB
        // (Kasm's documented minimum) when the template does not override it.
        let shm_size: Option<i64> = Some(
            config
                .run_config
                .get("shm_size")
                .and_then(|v| v.as_i64())
                .unwrap_or(512 * 1024 * 1024),
        );

        // ── Hostname ──
        let hostname: Option<&str> = config
            .run_config
            .get("hostname")
            .and_then(|v| v.as_str());

        let host_config = bollard::models::HostConfig {
            privileged: Some(dini.privileged),
            cap_drop: dini.cap_drop,
            tmpfs: dini.tmpfs,
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
            port_bindings: config.host_port.map(|host_port| {
                let host_ip = config.host_gateway_ip.as_deref().unwrap_or("172.17.0.1");
                let bindings = port_bindings_for(&config.remote_type, host_ip, host_port);
                bindings.into_iter().map(|(k, v)| (k, Some(v))).collect()
            }),
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

        // A created-but-not-started container is a half-open resource: Docker
        // programs the host port binding at *start* time, so a concurrent launch
        // can steal our port between create and start (the DB UNIQUE index on
        // `host_port` arbitrates allocation, but the bind itself is the race).
        // If any post-create step fails, remove the container so the launch
        // route's port-conflict retry can re-create it under the same name with
        // a fresh port — otherwise the orphaned name would 409 on retry.
        let post_create = async {
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

            Ok::<(), String>(())
        }
        .await;

        if let Err(e) = post_create {
            tracing::warn!(
                "Container '{}' failed after creation ({}); removing it so a launch retry can rebind a fresh port",
                container_name,
                e
            );
            if let Err(remove_err) = self
                .docker
                .remove_container(
                    &container.id,
                    None::<bollard::container::RemoveContainerOptions>,
                )
                .await
            {
                tracing::warn!(
                    "Failed to remove container '{}' after failed start: {}",
                    container_name,
                    remove_err
                );
            }
            return Err(e);
        }

        // ── Apply per-instance bandwidth limit (fail-open) ──
        if config.network_bandwidth_up_mbps > 0 || config.network_bandwidth_down_mbps > 0 {
            match self
                .apply_bandwidth_limit(
                    &container.id,
                    config.network_bandwidth_up_mbps,
                    config.network_bandwidth_down_mbps,
                )
                .await
            {
                Ok(()) => tracing::info!(
                    "Applied bandwidth limit for '{}' (up={} Mbps, down={} Mbps)",
                    container_name,
                    config.network_bandwidth_up_mbps,
                    config.network_bandwidth_down_mbps
                ),
                Err(e) => tracing::error!(
                    "Failed to apply bandwidth limit for '{}': {} (container keeps running without limit) — TODO: notify admin",
                    container_name, e
                ),
            }
        }

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

    async fn apply_bandwidth_limit(
        &self,
        container_id: &str,
        up_mbps: i32,
        down_mbps: i32,
    ) -> Result<(), String> {
        if up_mbps <= 0 && down_mbps <= 0 {
            return Ok(());
        }

        let info = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|e| format!("inspect failed: {}", e))?;
        let pid = info.state.and_then(|s| s.pid).unwrap_or(0) as u32;
        if pid == 0 {
            let short_id: String = container_id.chars().take(12).collect();
            return Err(format!("container {} is not running (pid 0)", short_id));
        }

        // Upload: shape egress on eth0 inside the container's netns.
        if up_mbps > 0 {
            crate::network_qos::apply_htb(&run_nsenter_ns, pid, "eth0", up_mbps as u64)?;
        }

        // Download: shape egress on the host-side veth (container ingress).
        if down_mbps > 0 {
            let eth0_output = run_nsenter_ns(
                pid,
                &["/usr/sbin/ip", "-o", "link", "show", "eth0"]
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>(),
            )?;
            // From inside the container netns, `eth0@ifN` names the peer veth's
            // ifindex in the *host* netns (unique). The container-side ifindex
            // (before `@`) is the same in every container netns and cannot be
            // used to find the right host-side veth.
            let host_ifindex = crate::network_qos::parse_peer_ifindex(&eth0_output)
                .ok_or_else(|| format!("could not parse eth0 peer ifindex from '{}'", eth0_output.trim()))?;

            let host_links = run_nsenter_ns(
                1,
                &["/usr/sbin/ip", "-o", "link", "show", "type", "veth"]
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>(),
            )?;
            let veths = crate::network_qos::parse_host_veths(&host_links);
            let host_veth = crate::network_qos::find_host_veth(&veths, host_ifindex)
                .ok_or_else(|| {
                    let short_id: String = container_id.chars().take(12).collect();
                    format!(
                        "no host-side veth matches eth0 peer ifindex {} for container {}",
                        host_ifindex, short_id
                    )
                })?;

            crate::network_qos::apply_htb(&run_nsenter_ns, 1, &host_veth, down_mbps as u64)?;
        }

        Ok(())
    }

    async fn has_session_connection(
        &self,
        container_id: &str,
        port: u16,
    ) -> Result<bool, String> {
        let info = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|e| format!("inspect failed: {}", e))?;
        let pid = info.state.and_then(|s| s.pid).unwrap_or(0) as u32;
        if pid == 0 {
            let short_id: String = container_id.chars().take(12).collect();
            return Err(format!("container {} is not running (pid 0)", short_id));
        }

        let output = run_nsenter_ns(
            pid,
            &[ss_binary_path(), "-t", "-n", "state", "established"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
        )?;

        Ok(ss_output_has_connection(&output, port))
    }

    /// Create the (empty, 1000:1000-owned) host data directory via an alpine
    /// helper container, then declare the Local Bind-mounted Named Volume over
    /// it. If the Volume declaration already exists — e.g. a previous Instance
    /// was deleted but its data preserved for reuse — the existing data is left
    /// untouched and the existing Volume is reused as-is.
    async fn prepare_persistent_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String> {
        match self.docker.inspect_volume(volume_name).await {
            Ok(_) => return Ok(()),
            Err(ref e) if is_volume_not_found(e) => {}
            Err(e) => return Err(format!("Failed to inspect volume '{}': {}", volume_name, e)),
        }

        self.run_helper_container(
            "prepare",
            host_path,
            vec!["sh", "-c", "mkdir -p /storage && chown 1000:1000 /storage"],
        )
        .await?;

        self.create_local_bind_volume(host_path, volume_name).await
    }

    /// Empty the host data directory via an alpine helper container, then
    /// remove the Volume declaration (tolerating an already-removed volume).
    async fn remove_persistent_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String> {
        self.run_helper_container(
            "remove",
            host_path,
            vec!["sh", "-c", "find /storage -mindepth 1 -delete"],
        )
        .await?;

        match self
            .docker
            .remove_volume(volume_name, None::<RemoveVolumeOptions>)
            .await
        {
            Ok(()) => Ok(()),
            Err(ref e) if is_volume_not_found(e) => Ok(()),
            Err(e) => Err(format!("Failed to remove volume '{}': {}", volume_name, e)),
        }
    }

    /// Re-declare the local-bind Volume if Docker no longer knows it. The host
    /// data dir already holds the Instance's data, so a missing declaration is
    /// simply recreated — no helper run, no re-population.
    async fn ensure_persistent_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String> {
        match self.docker.inspect_volume(volume_name).await {
            Ok(_) => Ok(()),
            Err(ref e) if is_volume_not_found(e) => {
                self.create_local_bind_volume(host_path, volume_name).await
            }
            Err(e) => Err(format!(
                "Failed to inspect volume '{}': {}",
                volume_name, e
            )),
        }
    }
}

impl DockerClient {
    /// Declare a Local Bind-mounted Named Volume over `host_path`. The same
    /// declaration is shared by `prepare_persistent_volume` (fresh creation)
    /// and `ensure_persistent_volume` (re-creation after loss).
    async fn create_local_bind_volume(
        &self,
        host_path: &str,
        volume_name: &str,
    ) -> Result<(), String> {
        let mut driver_opts = HashMap::new();
        driver_opts.insert("type".to_string(), "none".to_string());
        driver_opts.insert("device".to_string(), host_path.to_string());
        driver_opts.insert("o".to_string(), "bind".to_string());
        let opts = CreateVolumeOptions {
            name: volume_name.to_string(),
            driver: "local".to_string(),
            driver_opts,
            ..Default::default()
        };
        self.docker
            .create_volume(opts)
            .await
            .map_err(|e| format!("Failed to create volume '{}': {}", volume_name, e))?;
        Ok(())
    }

    /// Run a short-lived `alpine` helper container that bind-mounts `host_path`
    /// at `/storage`, executes `cmd` as root, waits for it to finish, and
    /// removes the container. Any non-zero exit fails the operation. The API
    /// runs in a container without host filesystem access; every host-side
    /// filesystem mutation for persistent storage goes through here.
    async fn run_helper_container(
        &self,
        purpose: &str,
        host_path: &str,
        cmd: Vec<&str>,
    ) -> Result<(), String> {
        if self.docker.inspect_image("alpine:latest").await.is_err() {
            tracing::info!("Pulling 'alpine' for persistent-volume helper...");
            self.docker
                .create_image(
                    Some(CreateImageOptions {
                        from_image: "alpine",
                        tag: "latest",
                        ..Default::default()
                    }),
                    None,
                    None,
                )
                .try_collect::<Vec<_>>()
                .await
                .map_err(|e| format!("Failed to pull alpine helper image: {}", e))?;
        }

        let name = format!(
            "ow-vol-{}-{}-{}",
            purpose,
            std::process::id(),
            Uuid::new_v4().simple()
        );

        let host_config = bollard::models::HostConfig {
            binds: Some(vec![format!("{}:/storage", host_path)]),
            ..Default::default()
        };
        let config = Config {
            image: Some("alpine"),
            cmd: Some(cmd),
            host_config: Some(host_config),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name.clone(),
                    ..Default::default()
                }),
                config,
            )
            .await
            .map_err(|e| format!("Failed to create helper container '{}': {}", name, e))?;

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| format!("Failed to start helper container '{}': {}", name, e))?;

        let wait_result = self
            .docker
            .wait_container(&container.id, None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await;

        let _ = self
            .docker
            .remove_container(
                &container.id,
                Some(RemoveContainerOptions {
                    v: false,
                    force: true,
                    link: false,
                }),
            )
            .await;

        match wait_result {
            Ok(statuses) => {
                let exit_code = statuses.first().map(|r| r.status_code).unwrap_or(-1);
                if exit_code != 0 {
                    Err(format!(
                        "Helper container '{}' failed with exit code {}",
                        name, exit_code
                    ))
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(format!(
                "Failed waiting for helper container '{}': {}",
                name, e
            )),
        }
    }
}

/// Resolve the `ss` binary path at runtime. Debian's iproute2 ships `ss` at
/// `/bin/ss` (usr-merged bookworm resolves that to `/usr/bin/ss`) — there is
/// no `/usr/sbin/ss`, unlike `ip`/`tc`. Probe a few candidates so the session
/// connection check does not silently fail-open on a different layout.
fn ss_binary_path() -> &'static str {
    const CANDIDATES: [&str; 3] = ["/bin/ss", "/usr/sbin/ss", "/usr/bin/ss"];
    for path in CANDIDATES {
        if std::path::Path::new(path).exists() {
            return path;
        }
    }
    "/bin/ss"
}

/// True when `ss -t -n state established` output contains an established
/// connection whose local endpoint is the given port. The local address is
/// the 4th whitespace-delimited field (State, Recv-Q, Send-Q, Local, Peer).
fn ss_output_has_connection(output: &str, port: u16) -> bool {
    let needle = format!(":{}", port);
    output.lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.len() >= 5 && fields[3].ends_with(&needle)
    })
}

/// Run `nsenter -t <pid> -n <args>` and return stdout on success.
fn run_nsenter_ns(pid: u32, args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("nsenter")
        .arg("-t")
        .arg(pid.to_string())
        .arg("-n")
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn nsenter for pid {}: {}", pid, e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "nsenter pid {} {:?} failed: {}",
            pid,
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
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
    fn test_port_bindings_for_kasmvnc() {
        let bindings = port_bindings_for(&RemoteType::KasmVnc, "172.17.0.1", 10000);
        let entry = bindings.get("6901/tcp").expect("expected 6901/tcp binding");
        assert_eq!(entry.len(), 1);
        assert_eq!(entry[0].host_ip.as_deref(), Some("172.17.0.1"));
        assert_eq!(entry[0].host_port.as_deref(), Some("10000"));
    }

    #[test]
    fn test_port_bindings_for_ttyd() {
        let bindings = port_bindings_for(&RemoteType::Ttyd, "172.17.0.1", 10001);
        assert!(bindings.contains_key("7681/tcp"));
        assert!(!bindings.contains_key("6901/tcp"));
    }

    #[test]
    fn test_port_bindings_for_jupyter() {
        let bindings = port_bindings_for(&RemoteType::Jupyter, "10.0.0.1", 20000);
        let entry = bindings.get("8888/tcp").expect("expected 8888/tcp binding");
        assert_eq!(entry[0].host_ip.as_deref(), Some("10.0.0.1"));
        assert_eq!(entry[0].host_port.as_deref(), Some("20000"));
    }

    #[test]
    fn test_runtime_to_host_config_other_returns_some() {
        assert_eq!(runtime_to_host_config("kata"), Some("kata".to_string()));
    }

    #[test]
    fn test_runtime_to_host_config_nvidia_returns_some() {
        assert_eq!(runtime_to_host_config("nvidia"), Some("nvidia".to_string()));
    }

    #[test]
    fn test_dini_security_profile_off_keeps_hardened_defaults() {
        let profile = dini_security_profile(false, "runsc");
        assert!(!profile.privileged);
        assert_eq!(
            profile.cap_drop,
            Some(vec!["NET_RAW".to_string(), "NET_ADMIN".to_string()])
        );
        assert!(profile.tmpfs.is_none());
        assert!(profile.dind_env.is_none());
    }

    #[test]
    fn test_dini_security_profile_on_runsc_is_sandboxed_privileged() {
        let profile = dini_security_profile(true, "runsc");
        assert!(profile.privileged);
        assert!(profile.cap_drop.is_none());
        assert_eq!(
            profile.tmpfs,
            Some(std::collections::HashMap::from([(
                "/var/lib/docker".to_string(),
                "exec,mode=755".to_string()
            )]))
        );
        assert_eq!(profile.dind_env.as_deref(), Some("OW_DOCKER_IN_INSTANCE=true"));
    }

    #[test]
    fn test_dini_security_profile_on_runc_is_full_host_privileged() {
        let profile = dini_security_profile(true, "runc");
        assert!(profile.privileged);
        assert!(profile.cap_drop.is_none());
        assert_eq!(
            profile.tmpfs,
            Some(std::collections::HashMap::from([(
                "/var/lib/docker".to_string(),
                "exec,mode=755".to_string()
            )]))
        );
        assert_eq!(profile.dind_env.as_deref(), Some("OW_DOCKER_IN_INSTANCE=true"));
    }

    #[test]
    fn test_ss_output_detects_local_session_port() {
        let output = "ESTAB 0 0 172.20.0.2:6901 172.20.0.1:54321\n";
        assert!(ss_output_has_connection(output, 6901));
    }

    #[test]
    fn test_ss_output_ignores_peer_port() {
        // A peer ending in the session port must not count as a connection.
        let output = "ESTAB 0 0 172.20.0.2:50000 172.20.0.1:6901\n";
        assert!(!ss_output_has_connection(output, 6901));
    }

    #[test]
    fn test_ss_output_ignores_other_local_ports() {
        let output = "ESTAB 0 0 172.20.0.2:50000 172.20.0.1:54321\n";
        assert!(!ss_output_has_connection(output, 6901));
    }

    #[test]
    fn test_ss_output_ignores_header_and_empty() {
        let header = "State  Recv-Q  Send-Q  Local Address:Port  Peer Address:Port  Process\n";
        assert!(!ss_output_has_connection(header, 6901));
        assert!(!ss_output_has_connection("", 6901));
    }
}
