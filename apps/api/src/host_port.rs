use std::collections::BTreeSet;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::{statat, AtFlags, FlockOperation, Mode, OFlags, CWD};
use rustix::process::getuid;

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

/// Lowest unused port in `[start, end)` for the given set of used ports.
/// Returns `None` when the pool is exhausted (or the range is empty).
pub fn lowest_free_port(used: &BTreeSet<u16>, start: u16, end: u16) -> Option<u16> {
    (start..end).find(|p| !used.contains(p))
}

/// Best-effort TCP probe: does something already listen on `host:port`?
/// Any connect failure (refused, timeout, unresolvable host) is treated as
/// "free", so an unreachable gateway never blocks allocation.
pub fn port_in_use(host: &str, port: u16) -> bool {
    use std::net::TcpStream;
    use std::net::ToSocketAddrs;
    use std::time::Duration;

    let Ok(mut addrs) = format!("{}:{}", host, port).to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

/// Lowest unused port in `[start, end)` scanning *circularly* from `from`
/// (wrapping back to `start`). Lets a retry pick a different candidate than
/// every other concurrent retry instead of stampeding the same lowest port.
pub fn lowest_free_port_from(
    used: &BTreeSet<u16>,
    start: u16,
    end: u16,
    from: u16,
) -> Option<u16> {
    if start >= end {
        return None;
    }
    let width = end as u32 - start as u32;
    let norm = start + ((from as u32 - start as u32) % width) as u16;
    for i in 0..width {
        let candidate = start + ((norm as u32 - start as u32 + i) % width) as u16;
        if !used.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Deterministic per-instance spread across the pool (FNV-1a over the access
/// token). Used on the port-conflict retry so concurrent launches don't all
/// re-try the same lowest free port and re-collide at Docker's bind.
pub fn spread_offset(token: &str, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let mut hash: u32 = 0x811c_9dc5;
    for byte in token.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash % width as u32) as u16
}

/// Detect Docker's "port is already allocated" bind error at container create
/// time, which triggers the launch retry against the next free port.
pub fn is_port_conflict(err: &str) -> bool {
    err.contains("port is already allocated")
}

/// A host port held for the allocate → create → start → DB-commit window.
///
/// The port is *owned* because a non-blocking exclusive `flock` is held on its
/// lockfile; the kernel ties the lock to the open file description, so
/// dropping the handle releases the port automatically even if the process
/// dies mid-window (no TTL, no reaper). Lockfiles are never unlinked, so no
/// two processes can ever hold locks on two different inodes at one path.
pub struct ReservedPort {
    pub port: u16,
    pub lock: OwnedFd,
}

/// The candidate lock-directory paths in resolution order: `Settings.port_lock_dir`
/// (already loaded from the `PORT_LOCK_DIR` env var), the per-UID runtime dir,
/// the XDG runtime dir, and finally a per-UID tmp dir. Empty inputs are skipped.
fn candidate_paths(
    settings_dir: &str,
    xdg_runtime_env: &str,
    uid: u32,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if !settings_dir.is_empty() {
        candidates.push(PathBuf::from(settings_dir));
    }
    candidates.push(PathBuf::from(format!("/run/user/{}/ow_ports", uid)));
    if !xdg_runtime_env.is_empty() {
        candidates.push(PathBuf::from(xdg_runtime_env).join("ow_ports"));
    }
    candidates.push(PathBuf::from(format!("/tmp/ow-ports-{}", uid)));
    candidates
}

/// Create `dir` with mode 0700 (no error if it already exists) and verify it is
/// a real directory owned by the current UID with no group/other permissions.
/// Fails closed: a candidate that cannot be made/verified is skipped.
fn prepare_lock_dir(dir: &Path) -> bool {
    match rustix::fs::mkdir(dir, Mode::from_raw_mode(0o700)) {
        Ok(()) => {}
        Err(rustix::io::Errno::EXIST) => {}
        Err(_) => return false,
    }
    let Ok(stat) = statat(CWD, dir, AtFlags::SYMLINK_NOFOLLOW) else {
        return false;
    };
    let mode = stat.st_mode as u32;
    stat.st_uid as u32 == getuid().as_raw()
        && mode & S_IFMT == S_IFDIR
        && mode & 0o700 == 0o700
        && mode & 0o077 == 0
}

/// First candidate directory that can be prepared and verified as a lock
/// directory, or `None` if every candidate fails (allocation fails closed).
fn usable_lock_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|c| prepare_lock_dir(c)).cloned()
}

/// Resolve the shared host-port lock directory for this UID, deterministic
/// across every process on the host: `Settings.port_lock_dir` (loaded from the
/// `PORT_LOCK_DIR` env var) → `/run/user/<uid>/ow_ports` →
/// `$XDG_RUNTIME_DIR/ow_ports` → `/tmp/ow-ports-<uid>`. Returns `None` when no
/// candidate is usable.
pub fn resolve_lock_dir(settings_dir: &str) -> Option<PathBuf> {
    let candidates = candidate_paths(
        settings_dir,
        &std::env::var("XDG_RUNTIME_DIR").unwrap_or_default(),
        getuid().as_raw(),
    );
    usable_lock_dir(&candidates)
}

/// Try to reserve `port` by taking a non-blocking exclusive `flock` on its
/// lockfile inside `lock_dir`. Returns `Some` (and owns the port) on success;
/// `None` when the file cannot be opened or another holder owns the lock. The
/// lockfile is created if absent and **never unlinked**.
pub fn acquire_lock(lock_dir: &Path, port: u16) -> Option<OwnedFd> {
    let path = lock_dir.join(format!("{}.lock", port));
    let Ok(fd) = rustix::fs::open(
        &path,
        OFlags::RDWR | OFlags::CREATE | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::from_raw_mode(0o600),
    ) else {
        return None;
    };
    match rustix::fs::flock(&fd, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Some(fd),
        Err(_) => None,
    }
}

/// Allocate a host port under the flock registry: skip the DB-committed `used`
/// set, then for each candidate in circular order from `from` try a non-blocking
/// `flock` (winner), then the TCP probe (covers ports bound by running
/// containers). Returns the reservation, or `None` when every candidate is taken.
pub fn try_allocate_port(
    used: &BTreeSet<u16>,
    start: u16,
    end: u16,
    host: &str,
    from: u16,
    lock_dir: &Path,
) -> Option<ReservedPort> {
    let mut busy = used.clone();
    loop {
        let candidate = match lowest_free_port_from(&busy, start, end, from) {
            None => return None,
            Some(c) => c,
        };
        let Some(lock) = acquire_lock(lock_dir, candidate) else {
            busy.insert(candidate);
            continue;
        };
        if port_in_use(host, candidate) {
            drop(lock);
            busy.insert(candidate);
            continue;
        }
        return Some(ReservedPort {
            port: candidate,
            lock,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;

    fn set(ports: &[u16]) -> BTreeSet<u16> {
        ports.iter().copied().collect()
    }

    fn temp_lock_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ow_flock_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn make_0700(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn lowest_free_port_returns_first_free() {
        assert_eq!(lowest_free_port(&set(&[10001, 10002]), 10000, 20000), Some(10000));
    }

    #[test]
    fn lowest_free_port_skips_used_in_order() {
        assert_eq!(lowest_free_port(&set(&[10000, 10001, 10003]), 10000, 20000), Some(10002));
    }

    #[test]
    fn lowest_free_port_empty_used_returns_start() {
        assert_eq!(lowest_free_port(&set(&[]), 10000, 20000), Some(10000));
    }

    #[test]
    fn lowest_free_port_exhausted_returns_none() {
        assert_eq!(lowest_free_port(&set(&[10000, 10001]), 10000, 10002), None);
    }

    #[test]
    fn lowest_free_port_inverted_range_returns_none() {
        assert_eq!(lowest_free_port(&set(&[]), 20000, 10000), None);
    }

    #[test]
    fn lowest_free_port_boundary_start_is_included() {
        assert_eq!(lowest_free_port(&set(&[10000, 10002]), 10000, 10003), Some(10001));
    }

    #[test]
    fn lowest_free_port_end_is_exclusive() {
        assert_eq!(lowest_free_port(&set(&[10000, 10001, 10002]), 10000, 10003), None);
    }

    #[test]
    fn port_in_use_detects_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_in_use("127.0.0.1", port));
        drop(listener);
        assert!(!port_in_use("127.0.0.1", port));
    }

    #[test]
    fn port_in_use_closed_port_is_free() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!port_in_use("127.0.0.1", port));
    }

    #[test]
    fn is_port_conflict_matches_docker_message() {
        let msg = "Error response from daemon: driver failed programming external connectivity on endpoint ow: Bind for 172.17.0.1:10000 failed: port is already allocated";
        assert!(is_port_conflict(msg));
        assert!(is_port_conflict("port is already allocated"));
        assert!(!is_port_conflict("no such container"));
        assert!(!is_port_conflict(""));
    }

    #[test]
    fn lowest_free_port_from_scans_circularly() {
        let used = set(&[10000, 10001, 10002]);
        assert_eq!(lowest_free_port_from(&used, 10000, 10100, 10003), Some(10003));
        assert_eq!(lowest_free_port_from(&used, 10000, 10100, 10100), Some(10003));
    }

    #[test]
    fn lowest_free_port_from_wraps_to_start() {
        let used = set(&[10000, 10005]);
        assert_eq!(lowest_free_port_from(&used, 10000, 10100, 10006), Some(10006));
        assert_eq!(lowest_free_port_from(&used, 10000, 10100, 10100), Some(10001));
    }

    #[test]
    fn lowest_free_port_from_empty_range_returns_none() {
        assert_eq!(lowest_free_port_from(&set(&[]), 10000, 10000, 10000), None);
    }

    #[test]
    fn spread_offset_is_deterministic() {
        assert_eq!(spread_offset("token-a", 1000), spread_offset("token-a", 1000));
    }

    #[test]
    fn spread_offset_differs_for_distinct_tokens() {
        assert_ne!(spread_offset("token-a", 1000), spread_offset("token-b", 1000));
    }

    #[test]
    fn spread_offset_zero_width_returns_zero() {
        assert_eq!(spread_offset("anything", 0), 0);
    }

    #[test]
    fn spread_offset_in_bounds() {
        for i in 0..50u32 {
            let tok = format!("t{}", i);
            assert!(spread_offset(&tok, 500) < 500);
        }
    }

    #[test]
    fn reserved_port_per_ofd_contention_exactly_one_winner() {
        let dir = temp_lock_dir("contention");
        let port = 42000;
        let a = acquire_lock(&dir, port).expect("first acquisition wins");
        assert!(acquire_lock(&dir, port).is_none(), "second open must lose the flock");
        drop(a);
        assert!(acquire_lock(&dir, port).is_some(), "port must be re-acquirable after drop");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_releases_port_like_crashed_process() {
        let dir = temp_lock_dir("crash");
        let port = 42001;
        {
            let _lock = acquire_lock(&dir, port).unwrap();
        }
        assert!(acquire_lock(&dir, port).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lockfile_never_unlinked_after_release() {
        let dir = temp_lock_dir("nolink");
        let port = 42002;
        {
            let _lock = acquire_lock(&dir, port).unwrap();
        }
        let path = dir.join(format!("{}.lock", port));
        assert!(path.exists(), "lockfile must persist after release");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lock_dir_candidates_follow_settings_runtime_tmp_order() {
        let c = candidate_paths("/s", "X", 1234);
        assert_eq!(
            c,
            vec![
                PathBuf::from("/s"),
                PathBuf::from("/run/user/1234/ow_ports"),
                PathBuf::from("X/ow_ports"),
                PathBuf::from("/tmp/ow-ports-1234"),
            ]
        );
        let c2 = candidate_paths("", "", 7);
        assert_eq!(
            c2,
            vec![
                PathBuf::from("/run/user/7/ow_ports"),
                PathBuf::from("/tmp/ow-ports-7"),
            ]
        );
    }

    #[test]
    fn resolve_lock_dir_uses_settings_dir_first() {
        let dir = std::env::temp_dir().join(format!("ow_res_settings_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let resolved = resolve_lock_dir(&dir.to_string_lossy());
        assert_eq!(resolved, Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lock_dir_resolution_first_usable_wins() {
        let a = std::env::temp_dir().join(format!("ow_res_a_{}", std::process::id()));
        let b = std::env::temp_dir().join(format!("ow_res_b_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        assert_eq!(usable_lock_dir(&[]), None);
        assert_eq!(usable_lock_dir(&[a.clone(), b.clone()]), Some(a.clone()));
        assert!(a.exists());
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn lock_dir_resolution_skips_symlink_candidate() {
        let real = std::env::temp_dir().join(format!("ow_res_real_{}", std::process::id()));
        let link = std::env::temp_dir().join(format!("ow_res_link_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&real);
        let _ = std::fs::remove_file(&link);
        make_0700(&real);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(usable_lock_dir(&[link.clone(), real.clone()]), Some(real.clone()));
        let _ = std::fs::remove_dir_all(&real);
        let _ = std::fs::remove_file(&link);
    }

    #[test]
    fn lock_dir_resolution_rejects_loose_permissions() {
        let loose = std::env::temp_dir().join(format!("ow_res_loose_{}", std::process::id()));
        let fallback = std::env::temp_dir().join(format!("ow_res_fallback_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&loose);
        let _ = std::fs::remove_dir_all(&fallback);
        std::fs::create_dir_all(&loose).unwrap();
        make_0700(&fallback);
        assert_eq!(usable_lock_dir(&[loose.clone(), fallback.clone()]), Some(fallback.clone()));
        let _ = std::fs::remove_dir_all(&loose);
        let _ = std::fs::remove_dir_all(&fallback);
    }

    #[test]
    fn lock_dir_resolution_skips_non_dir_candidate() {
        let file = std::env::temp_dir().join(format!("ow_res_file_{}", std::process::id()));
        let fallback = std::env::temp_dir().join(format!("ow_res_filefb_{}", std::process::id()));
        let _ = std::fs::write(&file, b"x");
        let _ = std::fs::remove_dir_all(&fallback);
        make_0700(&fallback);
        assert_eq!(usable_lock_dir(&[file.clone(), fallback.clone()]), Some(fallback.clone()));
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir_all(&fallback);
    }

    #[test]
    fn resolve_lock_dir_skips_unusable_settings_dir() {
        let file = std::env::temp_dir().join(format!("ow_res_badset_{}", std::process::id()));
        let _ = std::fs::write(&file, b"x");
        let resolved = resolve_lock_dir(&file.to_string_lossy());
        assert_ne!(resolved, Some(file.clone()));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn try_allocate_port_skips_flock_held_candidate() {
        let dir = temp_lock_dir("flockskip");
        let port = 42003;
        let _held = acquire_lock(&dir, port).unwrap();
        let r = try_allocate_port(&set(&[]), port, port + 5, "127.0.0.1", port, &dir)
            .expect("next free port");
        assert_eq!(r.port, port + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_port_skips_probe_busy_candidate() {
        let dir = temp_lock_dir("probeskip");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let busy = listener.local_addr().unwrap().port();
        let r = try_allocate_port(&set(&[]), busy, busy + 5, "127.0.0.1", busy, &dir)
            .expect("next free port");
        assert_eq!(r.port, busy + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_port_respects_used_set() {
        let dir = temp_lock_dir("usedset");
        let r = try_allocate_port(&set(&[42004, 42005]), 42004, 42010, "127.0.0.1", 42004, &dir)
            .expect("free port");
        assert_eq!(r.port, 42006);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_port_exhausted_returns_none() {
        let dir = temp_lock_dir("exhausted");
        assert!(
            try_allocate_port(&set(&[42006, 42007]), 42006, 42008, "127.0.0.1", 42006, &dir)
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_port_flock_held_all_returns_none() {
        let dir = temp_lock_dir("allheld");
        let _a = acquire_lock(&dir, 42008).unwrap();
        let _b = acquire_lock(&dir, 42009).unwrap();
        assert!(
            try_allocate_port(&set(&[]), 42008, 42010, "127.0.0.1", 42008, &dir).is_none()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
