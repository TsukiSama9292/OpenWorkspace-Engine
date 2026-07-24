use std::path::PathBuf;

fn default_dynamic_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("../../docker/openworkspace_dev/traefik/dynamic")
}

fn write_vnc_route_to(dir: &PathBuf, token: &str, container_ip: &str) -> Result<(), String> {
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
        - "vnc-auth"
  services:
    vnc-{token}:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://{ip}:6901"
"#,
        token = token,
        ip = container_ip,
    );
    std::fs::write(&ws_path, ws_yaml).map_err(|e| format!("write {}: {}", ws_path.display(), e))?;

    let page_path = dir.join(format!("vnc-{}-page.yml", token));
    let page_yaml = format!(
        r#"http:
  routers:
    vnc-{token}-page:
      rule: "PathPrefix(`/vnc/{token}`)"
      service: "web-service"
      entryPoints:
        - web
"#,
        token = token,
    );
    std::fs::write(&page_path, page_yaml).map_err(|e| format!("write {}: {}", page_path.display(), e))?;

    tracing::info!("Traefik VNC routes written for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

pub fn write_vnc_route(token: &str, container_ip: &str) -> Result<(), String> {
    write_vnc_route_to(&default_dynamic_dir(), token, container_ip)
}

fn delete_vnc_route_from(dir: &PathBuf, token: &str) -> Result<(), String> {
    let ws_path = dir.join(format!("vnc-{}-ws.yml", token));
    let page_path = dir.join(format!("vnc-{}-page.yml", token));

    if ws_path.exists() {
        std::fs::remove_file(&ws_path).map_err(|e| format!("remove {}: {}", ws_path.display(), e))?;
    }
    if page_path.exists() {
        std::fs::remove_file(&page_path).map_err(|e| format!("remove {}: {}", page_path.display(), e))?;
    }
    tracing::info!("Traefik VNC routes deleted for token={}", &token[..std::cmp::min(8, token.len())]);
    Ok(())
}

pub fn delete_vnc_route(token: &str) -> Result<(), String> {
    delete_vnc_route_from(&default_dynamic_dir(), token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vnc_trafik_test_{}_{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_creates_two_files() {
        let dir = temp_dir();
        let result = write_vnc_route_to(&dir, "abc123", "172.17.0.2");
        assert!(result.is_ok());
        assert!(dir.join("vnc-abc123-ws.yml").exists());
        assert!(dir.join("vnc-abc123-page.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ws_file_contains_websockify_rule() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/vnc/tok1/websockify`)"));
        assert!(content.contains("https://10.0.0.1:6901"));
        assert!(content.contains("kasm-insecure"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn page_file_contains_page_rule() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-page.yml")).unwrap();
        assert!(content.contains("PathPrefix(`/vnc/tok1`)"));
        assert!(content.contains("web-service"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_overwrites_existing_files() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        write_vnc_route_to(&dir, "tok1", "10.0.0.2").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("https://10.0.0.2:6901"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_removes_files() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        assert!(dir.join("vnc-tok1-ws.yml").exists());
        assert!(dir.join("vnc-tok1-page.yml").exists());
        delete_vnc_route_from(&dir, "tok1").unwrap();
        assert!(!dir.join("vnc-tok1-ws.yml").exists());
        assert!(!dir.join("vnc-tok1-page.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_nonexistent_is_noop() {
        let dir = temp_dir();
        let result = delete_vnc_route_from(&dir, "nonexistent");
        assert!(result.is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multiple_tokens_independent() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "aaa", "10.0.0.1").unwrap();
        write_vnc_route_to(&dir, "bbb", "10.0.0.2").unwrap();
        assert!(dir.join("vnc-aaa-ws.yml").exists());
        assert!(dir.join("vnc-bbb-ws.yml").exists());
        delete_vnc_route_from(&dir, "aaa").unwrap();
        assert!(!dir.join("vnc-aaa-ws.yml").exists());
        assert!(dir.join("vnc-bbb-ws.yml").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ws_file_has_vnc_auth_middleware() {
        let dir = temp_dir();
        write_vnc_route_to(&dir, "tok1", "10.0.0.1").unwrap();
        let content = fs::read_to_string(dir.join("vnc-tok1-ws.yml")).unwrap();
        assert!(content.contains("vnc-auth"));
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
    fn write_vnc_route_uses_default_dir() {
        let dir = default_dynamic_dir();
        fs::create_dir_all(&dir).ok();
        let result = write_vnc_route("test_default_dir_token", "10.0.0.3");
        assert!(result.is_ok());
        assert!(dir.join("vnc-test_default_dir_token-ws.yml").exists());
        assert!(dir.join("vnc-test_default_dir_token-page.yml").exists());
        let _ = fs::remove_file(dir.join("vnc-test_default_dir_token-ws.yml"));
        let _ = fs::remove_file(dir.join("vnc-test_default_dir_token-page.yml"));
    }

    #[test]
    fn delete_vnc_route_uses_default_dir() {
        let dir = default_dynamic_dir();
        fs::create_dir_all(&dir).ok();
        let _ = fs::remove_file(dir.join("vnc-test_del_default_token-ws.yml"));
        let _ = fs::remove_file(dir.join("vnc-test_del_default_token-page.yml"));
        let result = delete_vnc_route("test_del_default_token");
        assert!(result.is_ok());
    }
}
