use crate::docker::RemoteType;
use std::path::PathBuf;

fn default_dynamic_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("../../docker/openworkspace_dev/traefik/dynamic")
}

fn write_route_to(dir: &PathBuf, remote_type: &RemoteType, token: &str, container_ip: &str, password: &str) -> Result<(), String> {
    match remote_type {
        RemoteType::KasmVnc => write_vnc_route_to(dir, token, container_ip, password),
        RemoteType::Ttyd => write_ttyd_route_to(dir, token, container_ip),
        RemoteType::Jupyter => write_jupyter_route_to(dir, token, container_ip),
    }
}

fn write_vnc_route_to(dir: &PathBuf, token: &str, container_ip: &str, vnc_password: &str) -> Result<(), String> {
    use base64::Engine;
    let auth_header = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("kasm_user:{}", vnc_password)));

    let ws_path = dir.join(format!("vnc-{}-ws.yml", token));
    let ws_yaml = format!(
        r#"http:
  routers:
    vnc-{token}-ws:
      rule: "PathPrefix(`/vnc/{token}/websockify`)"
      service: "vnc-{token}"
      entryPoints:
        - web
      middlewares:
        - "vnc-{token}-auth"
  services:
    vnc-{token}:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://{ip}:6901"
  middlewares:
    vnc-{token}-auth:
      headers:
        customRequestHeaders:
          Authorization: "{auth_header}"
"#,
        token = token,
        ip = container_ip,
        auth_header = auth_header,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    tracing::info!("Traefik VNC route written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

fn write_ttyd_route_to(dir: &PathBuf, token: &str, container_ip: &str) -> Result<(), String> {
    let ws_path = dir.join(format!("ttyd-{}-ws.yml", token));
    let ws_yaml = format!(
        r#"http:
  routers:
    ttyd-{token}-ws:
      rule: "PathPrefix(`/ttyd/{token}/`)"
      service: "ttyd-{token}"
      entryPoints:
        - web
  services:
    ttyd-{token}:
      loadBalancer:
        servers:
          - url: "http://{ip}:7681"
"#,
        token = token,
        ip = container_ip,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    tracing::info!("Traefik ttyd route written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

fn write_jupyter_route_to(dir: &PathBuf, token: &str, container_ip: &str) -> Result<(), String> {
    let ws_path = dir.join(format!("jupyter-{}-ws.yml", token));
    let ws_yaml = format!(
        r#"http:
  routers:
    jupyter-{token}-ws:
      rule: "PathPrefix(`/jupyter/{token}/`)"
      service: "jupyter-{token}"
      entryPoints:
        - web
  services:
    jupyter-{token}:
      loadBalancer:
        servers:
          - url: "http://{ip}:8888"
"#,
        token = token,
        ip = container_ip,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    tracing::info!("Traefik Jupyter route written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

pub fn write_route(remote_type: &RemoteType, token: &str, container_ip: &str, password: &str) -> Result<(), String> {
    write_route_to(&default_dynamic_dir(), remote_type, token, container_ip, password)
}

fn delete_route_from(dir: &PathBuf, token: &str) -> Result<(), String> {
    for prefix in &["vnc-", "ttyd-", "jupyter-"] {
        let path = dir.join(format!("{}{}-ws.yml", prefix, token));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("remove {}: {}", path.display(), e))?;
        }
    }
    tracing::info!("Traefik routes deleted for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

pub fn delete_route(token: &str) -> Result<(), String> {
    delete_route_from(&default_dynamic_dir(), token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("route_writer_test_{}_{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_creates_vnc_file() {
        let dir = temp_dir();
        let result = write_vnc_route_to(&dir, "abc123", "172.17.0.2", "testpass");
        assert!(result.is_ok());
        assert!(dir.join("vnc-abc123-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn vnc_file_contains_websockify_rule() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1", "testpass").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/vnc/tok1/websockify`)"));
        assert!(content.contains("https://10.0.0.1:6901"));
        assert!(content.contains("kasm-insecure"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ttyd_file_contains_path_prefix() {
        let dir = temp_dir();
        write_ttyd_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        let content = fs::read_to_string(dir.join("ttyd-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/ttyd/tok1/`)"));
        assert!(content.contains("http://10.0.0.1:7681"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn jupyter_file_contains_path_prefix() {
        let dir = temp_dir();
        write_jupyter_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        let content = fs::read_to_string(dir.join("jupyter-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/jupyter/tok1/`)"));
        assert!(content.contains("http://10.0.0.1:8888"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_overwrites_existing_files() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1", "testpass").unwrap();
        write_vnc_route_to(&dir, "tok1", "10.0.0.2", "testpass2").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("https://10.0.0.2:6901"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_removes_all_type_files() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1", "testpass").unwrap();
        write_ttyd_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        write_jupyter_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        assert!(dir.join("vnc-tok1-ws.yml").exists());
        assert!(dir.join("ttyd-tok1-ws.yml").exists());
        assert!(dir.join("jupyter-tok1-ws.yml").exists());
        delete_route_from(&dir, "tok1").unwrap();
        assert!(!dir.join("vnc-tok1-ws.yml").exists());
        assert!(!dir.join("ttyd-tok1-ws.yml").exists());
        assert!(!dir.join("jupyter-tok1-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_nonexistent_is_noop() {
        let dir = temp_dir();
        let result = delete_route_from(&dir, "nonexistent");
        assert!(result.is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multiple_tokens_independent() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "aaa", "10.0.0.1", "pw_a").unwrap();
        write_ttyd_route_to(&dir, "bbb", "10.0.0.2").unwrap();
        assert!(dir.join("vnc-aaa-ws.yml").exists());
        assert!(dir.join("ttyd-bbb-ws.yml").exists());
        delete_route_from(&dir, "aaa").unwrap();
        assert!(!dir.join("vnc-aaa-ws.yml").exists());
        assert!(dir.join("ttyd-bbb-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn vnc_file_has_auth_middleware() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1", "testpass").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("vnc-tok1-auth"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_dynamic_dir_contains_traefik() {
        let dir = default_dynamic_dir();
        let s = dir.to_str().unwrap();
        assert!(s.contains("traefik"), "expected traefik in path: {}", s);
        assert!(s.contains("dynamic"), "expected dynamic in path: {}", s);
    }

    #[test]
    fn write_route_uses_default_dir() {
        let dir = default_dynamic_dir();
        fs::create_dir_all(&dir).ok();
        let result = write_route(&RemoteType::KasmVnc, "test_default_dir_token", "10.0.0.3", "testpass");
        assert!(result.is_ok());
        assert!(dir.join("vnc-test_default_dir_token-ws.yml").exists());
        let _ = fs::remove_file(dir.join("vnc-test_default_dir_token-ws.yml"));
    }

    #[test]
    fn delete_route_uses_default_dir() {
        let dir = default_dynamic_dir();
        fs::create_dir_all(&dir).ok();
        let _ = fs::remove_file(dir.join("vnc-test_del_default_token-ws.yml"));
        let result = delete_route("test_del_default_token");
        assert!(result.is_ok());
    }

    #[test]
    fn write_route_to_readonly_dir_fails() {
        let result = write_route_to(&PathBuf::from("/proc"), &RemoteType::KasmVnc, "errtok", "10.0.0.1", "testpass");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("write"), "error should mention write: {}", err);
    }

    #[test]
    fn write_vnc_route_full_token_content() {
        let dir = temp_dir();
        let token = "fulltok123abc";
        write_vnc_route_to(&dir, token, "172.17.0.5", "secret123").unwrap();

        let ws = fs::read_to_string(dir.join(format!("vnc-{}-ws.yml", token))).unwrap();
        assert!(ws.contains(&format!("vnc/{}", token)));
        assert!(ws.contains("kasm-insecure"));
        assert!(ws.contains(&format!("vnc-{}-auth", token)));
        assert!(ws.contains("https://172.17.0.5:6901"));
        assert!(ws.contains("entryPoints"));
        assert!(ws.contains("loadBalancer"));
        assert!(ws.contains("customRequestHeaders"));
        assert!(ws.contains("Authorization"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_route_dispatches_vnc() {
        let dir = temp_dir();
        let result = write_route_to(&dir, &RemoteType::KasmVnc, "disptok", "10.0.0.1", "testpass");
        assert!(result.is_ok());
        assert!(dir.join("vnc-disptok-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_route_dispatches_ttyd() {
        let dir = temp_dir();
        let result = write_route_to(&dir, &RemoteType::Ttyd, "disptok", "10.0.0.1", "");
        assert!(result.is_ok());
        assert!(dir.join("ttyd-disptok-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_route_dispatches_jupyter() {
        let dir = temp_dir();
        let result = write_route_to(&dir, &RemoteType::Jupyter, "disptok", "10.0.0.1", "");
        assert!(result.is_ok());
        assert!(dir.join("jupyter-disptok-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_vnc_route_long_token_truncated_in_log() {
        let dir = temp_dir();
        let long_token = "a".repeat(64);
        let result = write_vnc_route_to(&dir, &long_token, "10.0.0.99", "testpass");
        assert!(result.is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_route_partial_existing() {
        let dir = temp_dir();
        let _ = fs::remove_file(dir.join("vnc-partial-ws.yml"));

        fs::write(dir.join("vnc-partial-ws.yml"), "ws-only").unwrap();

        let result = delete_route_from(&dir, "partial");
        assert!(result.is_ok());
        assert!(!dir.join("vnc-partial-ws.yml").exists());

        fs::remove_dir_all(&dir).unwrap();
    }
}
