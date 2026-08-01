//! Pure helpers for the persistent-storage volume feature.
//!
//! Everything here is a plain input/output transform so it can be unit-tested
//! without Docker or host privileges. The orchestration (running the helper
//! container, creating/removing volumes, wiring the API) lives in `docker.rs`
//! and the workspace routes.

/// Why a persistent host path could not be built from its parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The configured root is not an absolute path (does not start with `/`).
    RelativeRoot,
    /// A segment of the assembled path is empty (e.g. `//`).
    EmptySegment,
    /// A segment is `.` or `..` (path traversal).
    Traversal,
    /// A segment contains a NUL byte or other control character.
    InvalidCharacter,
}

/// Build the per-Instance host data path: `{root}/{template_name}/{owner_user_id}`.
///
/// The `root` is the Template's configured persistent root directory. A `None`
/// root means persistence is disabled on the Template and yields `Ok(None)` —
/// the caller should fall back to non-persistent behaviour.
pub fn resolve_persistent_host_path_opt(
    root: Option<&str>,
    template_name: &str,
    owner_user_id: &str,
) -> Result<Option<String>, PathError> {
    match root {
        None => Ok(None),
        Some(root) => resolve_persistent_host_path(root, template_name, owner_user_id).map(Some),
    }
}

/// Build the per-Instance host data path: `{root}/{template_name}/{owner_user_id}`.
pub fn resolve_persistent_host_path(
    root: &str,
    template_name: &str,
    owner_user_id: &str,
) -> Result<String, PathError> {
    if !root.starts_with('/') {
        return Err(PathError::RelativeRoot);
    }
    validate_component(template_name)?;
    validate_component(owner_user_id)?;
    let root = root.trim_end_matches('/');
    let path = format!("{}/{}/{}", root, template_name, owner_user_id);
    validate_path_segments(&path)?;
    Ok(path)
}

/// A template name or user id is a single path component: it must be
/// non-empty and free of `/`, `\`, `.`/`..`, and control characters so it
/// cannot inject extra path structure or escape the configured root.
fn validate_component(component: &str) -> Result<(), PathError> {
    if component.is_empty() {
        return Err(PathError::EmptySegment);
    }
    if component == "." || component == ".." {
        return Err(PathError::Traversal);
    }
    if component.contains(['/', '\\']) {
        return Err(PathError::Traversal);
    }
    if component.contains('\0') || component.chars().any(char::is_control) {
        return Err(PathError::InvalidCharacter);
    }
    Ok(())
}

/// Validate every `/`-separated segment of an assembled path. The leading
/// empty segment produced by an absolute root (`/`) is allowed; any other
/// empty segment (`//`), `.`, `..`, or a NUL / control character is rejected.
fn validate_path_segments(path: &str) -> Result<(), PathError> {
    if path.contains('\0') {
        return Err(PathError::InvalidCharacter);
    }
    for (index, segment) in path.split('/').enumerate() {
        if segment.is_empty() {
            if index == 0 {
                continue; // leading `/`
            }
            return Err(PathError::EmptySegment);
        }
        if segment == "." || segment == ".." {
            return Err(PathError::Traversal);
        }
        if segment.chars().any(char::is_control) {
            return Err(PathError::InvalidCharacter);
        }
    }
    Ok(())
}

/// Derive a stable, Docker-legal volume name from a resolved host path.
///
/// The name is a pure function of the host path (FNV-1a 64-bit digest, hex
/// encoded), so it never changes across Template renames or restarts and
/// satisfies Docker's volume-name rules (lowercase, no `/`, length < 255).
pub fn persistent_volume_name(resolved_host_path: &str) -> String {
    format!("ow-persist-{:016x}", fnv1a64(resolved_host_path.as_bytes()))
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The in-container mount target for a persistent volume, per remote type.
/// kasmvnc mounts the whole `kasm-user` home; ttyd and jupyter the `ow_user`
/// home. Unknown remote types return `None`.
pub fn persistent_container_target(remote_type: &str) -> Option<&'static str> {
    match remote_type {
        "kasmvnc" => Some("/home/kasm-user"),
        "ttyd" | "jupyter" => Some("/home/ow_user"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn builds_expected_host_path() {
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "kasmvnc", UUID),
            Ok(format!("/mnt/ow_dir/kasmvnc/{}", UUID))
        );
    }

    #[test]
    fn trims_trailing_slash_from_root() {
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir/", "ttyd", UUID),
            Ok(format!("/mnt/ow_dir/ttyd/{}", UUID))
        );
    }

    #[test]
    fn rejects_relative_root() {
        assert_eq!(
            resolve_persistent_host_path("mnt/ow_dir", "kasmvnc", UUID),
            Err(PathError::RelativeRoot)
        );
        assert_eq!(
            resolve_persistent_host_path("", "kasmvnc", UUID),
            Err(PathError::RelativeRoot)
        );
    }

    #[test]
    fn rejects_traversal_segments() {
        assert_eq!(
            resolve_persistent_host_path("/mnt/../ow_dir", "kasmvnc", UUID),
            Err(PathError::Traversal)
        );
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "..", UUID),
            Err(PathError::Traversal)
        );
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "../etc", UUID),
            Err(PathError::Traversal)
        );
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "kasmvnc", "../etc"),
            Err(PathError::Traversal)
        );
    }

    #[test]
    fn rejects_empty_segments() {
        assert_eq!(
            resolve_persistent_host_path("/mnt//ow_dir", "kasmvnc", UUID),
            Err(PathError::EmptySegment)
        );
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "", UUID),
            Err(PathError::EmptySegment)
        );
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "kasmvnc", ""),
            Err(PathError::EmptySegment)
        );
    }

    #[test]
    fn rejects_injected_control_characters() {
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "kasm\nvnc", UUID),
            Err(PathError::InvalidCharacter)
        );
        assert_eq!(
            resolve_persistent_host_path("/mnt/ow_dir", "kasmvnc", "\u{0}"),
            Err(PathError::InvalidCharacter)
        );
    }

    #[test]
    fn null_root_disables_persistence() {
        assert_eq!(
            resolve_persistent_host_path_opt(None, "kasmvnc", UUID),
            Ok(None)
        );
    }

    #[test]
    fn null_root_opt_forwards_path() {
        let resolved = resolve_persistent_host_path_opt(Some("/mnt/ow_dir"), "kasmvnc", UUID)
            .unwrap()
            .unwrap();
        assert_eq!(resolved, format!("/mnt/ow_dir/kasmvnc/{}", UUID));
    }

    #[test]
    fn volume_name_is_stable_for_same_path() {
        let path = format!("/mnt/ow_dir/kasmvnc/{}", UUID);
        assert_eq!(
            persistent_volume_name(&path),
            persistent_volume_name(&path)
        );
    }

    #[test]
    fn volume_name_differs_for_different_paths() {
        let a = persistent_volume_name("/mnt/ow_dir/kasmvnc/123e4567-e89b-12d3-a456-426614174000");
        let b = persistent_volume_name("/mnt/ow_dir/kasmvnc/123e4567-e89b-12d3-a456-426614174001");
        assert_ne!(a, b);
    }

    #[test]
    fn volume_name_is_docker_legal() {
        let name = persistent_volume_name("/mnt/ow_dir/kasmvnc/user_123");
        assert!(name.starts_with("ow-persist-"));
        assert!(!name.contains('/'));
        assert_eq!(name, name.to_lowercase());
        assert!(name.len() < 255);
    }

    #[test]
    fn container_targets_home_per_remote_type() {
        assert_eq!(
            persistent_container_target("kasmvnc"),
            Some("/home/kasm-user")
        );
        assert_eq!(persistent_container_target("ttyd"), Some("/home/ow_user"));
        assert_eq!(
            persistent_container_target("jupyter"),
            Some("/home/ow_user")
        );
        assert_eq!(persistent_container_target("unknown"), None);
    }
}
