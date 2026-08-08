#![forbid(unsafe_code)]

/// How long `container_logs` waits for the first Docker log chunk before
/// deciding the container is silent (quiet running container, follow=true) and
/// handing the unconsumed stream to the SSE endpoint — so a quiet instance
/// still upgrades to `text/event-stream` promptly instead of blocking forever.
pub const DOCKER_LOGS_PEEK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub mod activation;
pub mod audit;
pub mod auth;
pub mod core;
pub mod db;
pub mod effective_context;
pub mod docker;
pub mod health_worker;
pub mod host_port;
pub mod instance_net;
pub mod metrics;
pub mod monitor;
pub mod network_qos;
pub mod openapi;
pub mod persistent_volume;
pub mod proc;
pub mod routes;
pub mod timeout_action;
pub mod vnc_cache;
pub mod route_writer;
pub mod system_settings;
