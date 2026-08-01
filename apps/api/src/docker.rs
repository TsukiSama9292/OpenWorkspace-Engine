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
    pub network_bandwidth_up_mbps: i32,
    pub network_bandwidth_down_mbps: i32,
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
    fn test_runtime_to_host_config_other_returns_some() {
        assert_eq!(runtime_to_host_config("kata"), Some("kata".to_string()));
    }

    #[test]
    fn test_runtime_to_host_config_nvidia_returns_some() {
        assert_eq!(runtime_to_host_config("nvidia"), Some("nvidia".to_string()));
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
