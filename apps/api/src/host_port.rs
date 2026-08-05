use std::collections::BTreeSet;

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

/// Allocate a host port: scan circularly from `from` (default: `start`) over
/// the lowest free port in `[start, end)` that is not in `used` and not
/// already listening on `host` (per `port_in_use`). Returns `None` when every
/// candidate is taken.
pub fn allocate_host_port_from(
    used: &BTreeSet<u16>,
    start: u16,
    end: u16,
    host: &str,
    from: u16,
) -> Option<u16> {
    let mut busy = used.clone();
    loop {
        match lowest_free_port_from(&busy, start, end, from) {
            None => return None,
            Some(candidate) => {
                if port_in_use(host, candidate) {
                    busy.insert(candidate);
                    continue;
                }
                return Some(candidate);
            }
        }
    }
}

/// Allocate a host port: the lowest free port in `[start, end)` that is not in
/// `used` and not already listening on `host` (per `port_in_use`). Returns
/// `None` when every candidate is taken.
pub fn allocate_host_port(
    used: &BTreeSet<u16>,
    start: u16,
    end: u16,
    host: &str,
) -> Option<u16> {
    allocate_host_port_from(used, start, end, host, start)
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

/// In-process registry of host ports reserved for the
/// allocate → create → start → DB-commit window.
///
/// Docker binds the host port at `start` (not `create`), and the DB row is
/// committed only after `start` returns. Between allocation and that commit the
/// port is invisible to `collect_used_host_ports` (DB snapshot) and — until the
/// container's process starts listening — to the TCP probe. Without this set,
/// two concurrent launches would both see the port as free and race at Docker's
/// bind: one wins, the other gets `port is already allocated`, and its
/// `created`-stuck sandbox leaks runsc processes. The reservation closes that
/// hole within one process; cross-process collisions still fall through to the
/// port-conflict retry.
#[derive(Default)]
pub struct PortPool {
    reserved: BTreeSet<u16>,
}

impl PortPool {
    pub fn reserve(&mut self, port: u16) {
        self.reserved.insert(port);
    }

    pub fn release(&mut self, port: u16) {
        self.reserved.remove(&port);
    }

    pub fn reserved(&self) -> impl Iterator<Item = u16> + '_ {
        self.reserved.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::net::TcpListener;

    fn set(ports: &[u16]) -> BTreeSet<u16> {
        ports.iter().copied().collect()
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
    fn allocate_host_port_skips_listening_port() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = listener.local_addr().unwrap().port();
        let allocated = allocate_host_port(&set(&[]), busy_port, busy_port + 10, "127.0.0.1");
        assert_eq!(allocated, Some(busy_port + 1));
    }

    #[test]
    fn allocate_host_port_returns_none_when_every_candidate_listens() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = listener.local_addr().unwrap().port();
        assert_eq!(
            allocate_host_port(&set(&[]), busy_port, busy_port + 1, "127.0.0.1"),
            None
        );
    }

    #[test]
    fn allocate_host_port_respects_used_ports() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = listener.local_addr().unwrap().port();
        let allocated = allocate_host_port(&set(&[busy_port + 1]), busy_port, busy_port + 10, "127.0.0.1");
        assert_eq!(allocated, Some(busy_port + 2));
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
    fn port_pool_reserve_release_reserved() {
        let mut pool = PortPool::default();
        assert!(pool.reserved().next().is_none());
        pool.reserve(10001);
        pool.reserve(10002);
        let ports: Vec<u16> = pool.reserved().collect();
        assert_eq!(ports, vec![10001, 10002]);
        pool.release(10001);
        let ports: Vec<u16> = pool.reserved().collect();
        assert_eq!(ports, vec![10002]);
        pool.release(10002);
        assert!(pool.reserved().next().is_none());
    }

    #[test]
    fn port_pool_release_absent_is_noop() {
        let mut pool = PortPool::default();
        pool.release(10001);
        assert!(pool.reserved().next().is_none());
        pool.reserve(10001);
        pool.reserve(10001);
        assert_eq!(pool.reserved().count(), 1);
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
    fn allocate_host_port_from_respects_from_and_probe() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = listener.local_addr().unwrap().port();
        let allocated = allocate_host_port_from(&set(&[]), busy_port, busy_port + 10, "127.0.0.1", busy_port);
        assert_eq!(allocated, Some(busy_port + 1));
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
}
