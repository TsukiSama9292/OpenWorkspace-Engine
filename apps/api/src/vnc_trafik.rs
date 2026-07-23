use std::path::PathBuf;

fn dynamic_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("../../docker/openworkspace_dev/traefik/dynamic")
}

pub fn write_vnc_route(token: &str, container_ip: &str) -> Result<(), String> {
    let dir = dynamic_dir();

    // Route for websockify (WebSocket upgrade)
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

    // Route for VNC page (HTML served by SvelteKit, no ForwardAuth needed)
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

pub fn delete_vnc_route(token: &str) -> Result<(), String> {
    let dir = dynamic_dir();
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
