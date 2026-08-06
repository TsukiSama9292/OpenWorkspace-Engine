use crate::docker::RemoteType;
use std::path::{Path, PathBuf};

pub fn default_dynamic_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TRAEFIK_DYNAMIC_DIR")
        && !dir.is_empty() {
            return PathBuf::from(dir);
        }
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("../../docker/openworkspace_dev/traefik/dynamic")
}

fn write_route_to(dir: &Path, remote_type: &RemoteType, token: &str, host_port: u16, password: &str) -> Result<(), String> {
    match remote_type {
        RemoteType::KasmVnc => write_vnc_route_to(dir, token, host_port, password),
        RemoteType::Ttyd => write_ttyd_route_to(dir, token, host_port, password),
        RemoteType::Jupyter => write_jupyter_route_to(dir, token, host_port),
    }
}

fn write_vnc_route_to(dir: &Path, token: &str, host_port: u16, vnc_password: &str) -> Result<(), String> {
    use base64::Engine;
    let auth_header = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("kasm_user:{}", vnc_password)));

    let ws_path = dir.join(format!("kasmvnc-{}-ws.yml", token));
    let ws_yaml = format!(
        r#"http:
  routers:
    kasmvnc-{token}-ws:
      rule: "PathPrefix(`/kasmvnc/{token}/websockify`)"
      service: "kasmvnc-{token}"
      entryPoints:
        - web
      middlewares:
        - "kasmvnc-{token}-auth"
        - "kasmvnc-{token}-strip"
  services:
    kasmvnc-{token}:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://host.docker.internal:{host_port}"
  middlewares:
    kasmvnc-{token}-auth:
      headers:
        customRequestHeaders:
          Authorization: "{auth_header}"
    kasmvnc-{token}-strip:
      stripPrefix:
        prefixes:
          - "/kasmvnc/{token}"
"#,
        token = token,
        host_port = host_port,
        auth_header = auth_header,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    tracing::info!("Traefik VNC route written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

fn write_ttyd_route_to(dir: &Path, token: &str, host_port: u16, password: &str) -> Result<(), String> {
    use base64::Engine;
    let auth_header = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("ow_user:{}", password)));

    let ws_path = dir.join(format!("ttyd-{}-ws.yml", token));
    let ws_yaml = format!(
        r#"http:
  routers:
    ttyd-{token}-ws:
      rule: "PathPrefix(`/ttyd/{token}/`)"
      service: "ttyd-{token}"
      entryPoints:
        - web
      middlewares:
        - "ttyd-{token}-auth"
        - "ttyd-{token}-strip"
  services:
    ttyd-{token}:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://host.docker.internal:{host_port}"
  middlewares:
    ttyd-{token}-auth:
      headers:
        customRequestHeaders:
          Authorization: "{auth_header}"
    ttyd-{token}-strip:
      stripPrefix:
        prefixes:
          - "/ttyd/{token}"
"#,
        token = token,
        host_port = host_port,
        auth_header = auth_header,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    tracing::info!("Traefik ttyd route written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

fn write_jupyter_route_to(dir: &Path, token: &str, host_port: u16) -> Result<(), String> {
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
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://host.docker.internal:{host_port}"
"#,
        token = token,
        host_port = host_port,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    tracing::info!("Traefik Jupyter route written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

pub fn write_route(remote_type: &RemoteType, token: &str, host_port: u16, password: &str) -> Result<(), String> {
    write_route_to(&default_dynamic_dir(), remote_type, token, host_port, password)
}

fn delete_route_from(dir: &Path, token: &str) -> Result<(), String> {
    for prefix in &["kasmvnc-", "ttyd-", "jupyter-"] {
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
        let result = write_vnc_route_to(&dir, "abc123", 10000, "testpass");
        assert!(result.is_ok());
        assert!(dir.join("kasmvnc-abc123-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn vnc_file_contains_websockify_rule() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", 10000, "testpass").unwrap();
        let content = fs::read_to_string(dir.join("kasmvnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/kasmvnc/tok1/websockify`)"));
        assert!(content.contains("https://host.docker.internal:10000"));
        assert!(content.contains("kasm-insecure"));
        assert!(content.contains("web"));
        assert!(content.contains("kasmvnc-tok1-strip"));
        assert!(content.contains("stripPrefix"));
        assert!(content.contains("/kasmvnc/tok1"));
        assert!(!content.contains("tls"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ttyd_file_contains_path_prefix() {
        let dir = temp_dir();
        write_ttyd_route_to(&dir, "tok1", 10001, "testpass").unwrap();
        let content = fs::read_to_string(dir.join("ttyd-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/ttyd/tok1/`)"));
        assert!(content.contains("https://host.docker.internal:10001"));
        assert!(content.contains("kasm-insecure"));
        assert!(content.contains("stripPrefix"));
        assert!(content.contains("/ttyd/tok1"));
        assert!(content.contains("Authorization"));
        assert!(content.contains("web"));
        assert!(!content.contains("tls"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn jupyter_file_contains_path_prefix() {
        let dir = temp_dir();
        write_jupyter_route_to(&dir, "tok1", 10002).unwrap();
        let content = fs::read_to_string(dir.join("jupyter-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/jupyter/tok1/`)"));
        assert!(content.contains("https://host.docker.internal:10002"));
        assert!(content.contains("kasm-insecure"));
        assert!(!content.contains("stripPrefix"), "jupyter route should not strip prefix");
        assert!(content.contains("web"));
        assert!(!content.contains("tls"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_overwrites_existing_files() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", 10010, "testpass").unwrap();
        write_vnc_route_to(&dir, "tok1", 10020, "testpass2").unwrap();
        let content = fs::read_to_string(dir.join("kasmvnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("https://host.docker.internal:10020"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_removes_all_type_files() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", 10000, "testpass").unwrap();
        write_ttyd_route_to(&dir, "tok1", 10001, "testpass").unwrap();
        write_jupyter_route_to(&dir, "tok1", 10002).unwrap();
        assert!(dir.join("kasmvnc-tok1-ws.yml").exists());
        assert!(dir.join("ttyd-tok1-ws.yml").exists());
        assert!(dir.join("jupyter-tok1-ws.yml").exists());
        delete_route_from(&dir, "tok1").unwrap();
        assert!(!dir.join("kasmvnc-tok1-ws.yml").exists());
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
        write_vnc_route_to(&dir, "aaa", 10000, "pw_a").unwrap();
        write_ttyd_route_to(&dir, "bbb", 10001, "testpass").unwrap();
        assert!(dir.join("kasmvnc-aaa-ws.yml").exists());
        assert!(dir.join("ttyd-bbb-ws.yml").exists());
        delete_route_from(&dir, "aaa").unwrap();
        assert!(!dir.join("kasmvnc-aaa-ws.yml").exists());
        assert!(dir.join("ttyd-bbb-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn vnc_file_has_auth_middleware() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", 10000, "testpass").unwrap();
        let content = fs::read_to_string(dir.join("kasmvnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("kasmvnc-tok1-auth"));
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
        let result = write_route(&RemoteType::KasmVnc, "test_default_dir_token", 10003, "testpass");
        assert!(result.is_ok());
        assert!(dir.join("kasmvnc-test_default_dir_token-ws.yml").exists());
        let _ = fs::remove_file(dir.join("kasmvnc-test_default_dir_token-ws.yml"));
    }

    #[test]
    fn delete_route_uses_default_dir() {
        let dir = default_dynamic_dir();
        fs::create_dir_all(&dir).ok();
        let _ = fs::remove_file(dir.join("kasmvnc-test_del_default_token-ws.yml"));
        let result = delete_route("test_del_default_token");
        assert!(result.is_ok());
    }

    #[test]
    fn write_route_to_readonly_dir_fails() {
        let result = write_route_to(&PathBuf::from("/proc"), &RemoteType::KasmVnc, "errtok", 10004, "testpass");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("write"), "error should mention write: {}", err);
    }

    #[test]
    fn write_vnc_route_full_token_content() {
        let dir = temp_dir();
        let token = "fulltok123abc";
        write_vnc_route_to(&dir, token, 10005, "secret123").unwrap();

        let ws = fs::read_to_string(dir.join(format!("kasmvnc-{}-ws.yml", token))).unwrap();
        assert!(ws.contains(&format!("kasmvnc/{}", token)));
        assert!(ws.contains("kasm-insecure"));
        assert!(ws.contains(&format!("kasmvnc-{}-auth", token)));
        assert!(ws.contains(&format!("kasmvnc-{}-strip", token)));
        assert!(ws.contains("https://host.docker.internal:10005"));
        assert!(ws.contains("entryPoints"));
        assert!(ws.contains("loadBalancer"));
        assert!(ws.contains("customRequestHeaders"));
        assert!(ws.contains("Authorization"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_route_dispatches_vnc() {
        let dir = temp_dir();
        let result = write_route_to(&dir, &RemoteType::KasmVnc, "disptok", 10006, "testpass");
        assert!(result.is_ok());
        assert!(dir.join("kasmvnc-disptok-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_route_dispatches_ttyd() {
        let dir = temp_dir();
        let result = write_route_to(&dir, &RemoteType::Ttyd, "disptok", 10007, "testpass");
        assert!(result.is_ok());
        assert!(dir.join("ttyd-disptok-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_route_dispatches_jupyter() {
        let dir = temp_dir();
        let result = write_route_to(&dir, &RemoteType::Jupyter, "disptok", 10008, "");
        assert!(result.is_ok());
        assert!(dir.join("jupyter-disptok-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_vnc_route_long_token_truncated_in_log() {
        let dir = temp_dir();
        let long_token = "a".repeat(64);
        let result = write_vnc_route_to(&dir, &long_token, 10009, "testpass");
        assert!(result.is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_route_partial_existing() {
        let dir = temp_dir();
        let _ = fs::remove_file(dir.join("kasmvnc-partial-ws.yml"));

        fs::write(dir.join("kasmvnc-partial-ws.yml"), "ws-only").unwrap();

        let result = delete_route_from(&dir, "partial");
        assert!(result.is_ok());
        assert!(!dir.join("kasmvnc-partial-ws.yml").exists());

        fs::remove_dir_all(&dir).unwrap();
    }
}
