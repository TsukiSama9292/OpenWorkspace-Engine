//! Pure helpers for per-instance `/30` bridge networks.
//!
//! Every instance gets its own Docker bridge network with a `/30` subnet — the
//! smallest Docker accepts: network `.0`, gateway `.1`, instance `.2`,
//! broadcast `.3`. All logic here is a plain input/output transform so it can
//! be unit-tested without Docker or a database. The orchestration (listing the
//! subnets already in use, creating/removing networks, attaching containers)
//! lives in `docker.rs`.
//!
//! Subnet allocation carries the same cross-process arbitration as host ports:
//! `try_allocate_subnet` takes a non-blocking `flock` on a per-block lockfile
//! (key = the `/30` network address) in the shared per-UID lock directory, so
//! concurrent launches across API processes can never claim the same `/30`.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use std::path::Path;
use std::str::FromStr;

use rustix::fd::OwnedFd;

/// A parsed base CIDR from which aligned `/30` blocks are allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetBase {
    /// Network address of the base range, aligned to `prefix`.
    pub network: Ipv4Addr,
    /// Prefix length of the base range (`0..=32`).
    pub prefix: u8,
}

impl NetBase {
    /// Parse a `"<network>/<prefix>"` base CIDR, rejecting malformed strings,
    /// out-of-range prefixes, and networks with host bits set (a base must be
    /// aligned so `/30` blocks tile it exactly).
    pub fn parse(cidr: &str) -> Result<Self, String> {
        let (addr, prefix) = cidr.split_once('/').ok_or_else(|| {
            format!("invalid base CIDR '{}': expected '<network>/<prefix>'", cidr)
        })?;
        let network = Ipv4Addr::from_str(addr)
            .map_err(|_| format!("invalid base CIDR '{}': bad network '{}'", cidr, addr))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|_| format!("invalid base CIDR '{}': bad prefix '{}'", cidr, prefix))?;
        if prefix > 32 {
            return Err(format!(
                "invalid base CIDR '{}': prefix {} out of range",
                cidr, prefix
            ));
        }
        let base = Self { network, prefix };
        if base.clean_network() != network {
            return Err(format!(
                "invalid base CIDR '{}': network address has host bits set",
                cidr
            ));
        }
        Ok(base)
    }

    /// Number of aligned `/30` blocks the base range holds. A base narrower
    /// than `/30` holds none, so the pool is exhausted from the start.
    pub fn block_count(&self) -> u64 {
        if self.prefix > 30 {
            return 0;
        }
        1u64 << (30 - self.prefix)
    }

    fn clean_network(&self) -> Ipv4Addr {
        let mask = if self.prefix == 0 {
            0
        } else {
            u32::MAX << (32 - self.prefix)
        };
        Ipv4Addr::from(u32::from(self.network) & mask)
    }
}

/// The lowest free `/30` block network address within `base`, skipping the
/// block network addresses already present in `used`. Returns `None` when the
/// pool is exhausted (every block in the base range is taken).
pub fn lowest_free_subnet(used: &BTreeSet<Ipv4Addr>, base: &NetBase) -> Option<Ipv4Addr> {
    let start = u32::from(base.network);
    for i in 0..base.block_count() {
        let candidate = Ipv4Addr::from(start + (i as u32) * 4);
        if !used.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The lowest free `/30` block scanning *circularly* from block index `from`
/// (wrapping within `base`), skipping `used`. Mirrors the host-port retry
/// (`host_port::lowest_free_port_from`): a launch retrying after a pool-overlap
/// collision starts its scan at a different pool position than other concurrent
/// launches, so they don't all stampede the same lowest free block again.
pub fn lowest_free_subnet_from(
    used: &BTreeSet<Ipv4Addr>,
    base: &NetBase,
    from_block: u64,
) -> Option<Ipv4Addr> {
    let count = base.block_count();
    if count == 0 {
        return None;
    }
    let start = u32::from(base.network);
    for i in 0..count {
        let idx = from_block.wrapping_add(i) % count;
        let candidate = Ipv4Addr::from(start + (idx as u32) * 4);
        if !used.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// A reserved `/30` block: the network address plus the `OwnedFd` whose open
/// file description holds the exclusive `flock` on its lockfile. The caller
/// owns the subnet for as long as the handle lives; dropping it releases the
/// lock (RAII), so a process that dies mid-allocation never blocks the block
/// permanently.
pub struct ReservedSubnet {
    pub subnet: Ipv4Addr,
    pub lock: OwnedFd,
}

/// Allocate a `/30` block under the flock registry: skip the Docker-derived
/// `used` set, then for each candidate in circular order from `from_block` try
/// a non-blocking `flock` on the block's lockfile (key = the network address,
/// e.g. `10.200.0.0`). Returns the reservation, or `None` when every block in
/// the pool is taken. No Docker probe is needed — the used set already contains
/// every existing network; the residual stale-snapshot race (a subnet created
/// after our snapshot but whose block is now free again) is absorbed upstream by
/// the bounded `Pool overlaps` retry.
pub fn try_allocate_subnet(
    used: &BTreeSet<Ipv4Addr>,
    base: &NetBase,
    from_block: u64,
    lock_dir: &Path,
) -> Option<ReservedSubnet> {
    let mut busy = used.clone();
    loop {
        let candidate = if from_block == 0 {
            lowest_free_subnet(&busy, base)
        } else {
            lowest_free_subnet_from(&busy, base, from_block)
        }?;
        let Some(lock) = crate::host_port::acquire_lock(lock_dir, &candidate.to_string()) else {
            busy.insert(candidate);
            continue;
        };
        return Some(ReservedSubnet {
            subnet: candidate,
            lock,
        });
    }
}

/// Deterministic per-instance spread across the pool (FNV-1a over the access
/// token), as a `/30` block index. A retrying launch re-scans from here so it
/// doesn't re-collide with every other concurrent retry on the same block.
pub fn spread_block_offset(token: &str, base: &NetBase) -> u64 {
    let count = base.block_count();
    if count == 0 {
        return 0;
    }
    let mut hash: u32 = 0x811c_9dc5;
    for byte in token.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash as u64) % count
}

/// Gateway IP of a `/30` block: its network address plus one.
pub fn gateway_ip(network: Ipv4Addr) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(network) + 1)
}

/// Instance IP of a `/30` block: its network address plus two.
pub fn instance_ip(network: Ipv4Addr) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(network) + 2)
}

/// Deterministic network name derived from an instance's stable id, so any
/// caller can recompute the name with no state.
pub fn network_name(instance_id: &str) -> String {
    format!("ow-{}", instance_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    fn set(nets: &[&str]) -> BTreeSet<Ipv4Addr> {
        nets.iter().map(|s| s.parse().unwrap()).collect()
    }

    fn temp_lock_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ow_flock_sub_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn parse_accepts_aligned_bases() {
        let base = NetBase::parse("10.200.0.0/16").unwrap();
        assert_eq!(base.network, ip("10.200.0.0"));
        assert_eq!(base.prefix, 16);
        assert_eq!(NetBase::parse("172.16.0.0/12").unwrap().prefix, 12);
        assert_eq!(NetBase::parse("10.0.0.0/8").unwrap().network, ip("10.0.0.0"));
        assert_eq!(NetBase::parse("192.168.1.0/30").unwrap().prefix, 30);
        assert_eq!(NetBase::parse("0.0.0.0/0").unwrap().network, ip("0.0.0.0"));
    }

    #[test]
    fn parse_rejects_malformed_bases() {
        for bad in [
            "",
            "10.200.0.0",
            "not-a-cidr",
            "10.200.0.0/33",
            "10.200.0.0/abc",
            "300.1.2.3/16",
            "10.200.0.4/16",
        ] {
            assert!(NetBase::parse(bad).is_err(), "expected error for '{}'", bad);
        }
    }

    #[test]
    fn block_count_matches_base_size() {
        assert_eq!(NetBase::parse("10.200.0.0/16").unwrap().block_count(), 1 << 14);
        assert_eq!(NetBase::parse("10.0.0.0/8").unwrap().block_count(), 1 << 22);
        assert_eq!(NetBase::parse("192.168.1.0/30").unwrap().block_count(), 1);
        assert_eq!(NetBase::parse("10.200.0.0/29").unwrap().block_count(), 2);
        assert_eq!(NetBase::parse("10.200.0.0/31").unwrap().block_count(), 0);
        assert_eq!(NetBase::parse("10.200.0.0/32").unwrap().block_count(), 0);
    }

    #[test]
    fn lowest_free_returns_first_block() {
        let base = NetBase::parse("10.200.0.0/16").unwrap();
        assert_eq!(lowest_free_subnet(&set(&[]), &base), Some(ip("10.200.0.0")));
    }

    #[test]
    fn lowest_free_skips_used_in_order() {
        let base = NetBase::parse("10.200.0.0/16").unwrap();
        let used = set(&["10.200.0.0", "10.200.0.4", "10.200.0.12"]);
        assert_eq!(lowest_free_subnet(&used, &base), Some(ip("10.200.0.8")));
    }

    #[test]
    fn lowest_free_uses_first_fourth_block_alignment() {
        let base = NetBase::parse("10.200.0.0/16").unwrap();
        let used = set(&["10.200.0.0", "10.200.1.252"]);
        assert_eq!(lowest_free_subnet(&used, &base), Some(ip("10.200.0.4")));
    }

    #[test]
    fn lowest_free_exhausted_returns_none() {
        let base = NetBase::parse("10.200.0.0/30").unwrap();
        let used = set(&["10.200.0.0"]);
        assert_eq!(lowest_free_subnet(&used, &base), None);
    }

    #[test]
    fn lowest_free_narrower_than_thirty_is_always_exhausted() {
        let base = NetBase::parse("10.200.0.0/32").unwrap();
        assert_eq!(lowest_free_subnet(&set(&[]), &base), None);
    }

    #[test]
    fn lowest_free_last_block_at_top_of_range() {
        let base = NetBase::parse("10.200.0.252/30").unwrap();
        assert_eq!(lowest_free_subnet(&set(&[]), &base), Some(ip("10.200.0.252")));
        assert_eq!(lowest_free_subnet(&set(&["10.200.0.252"]), &base), None);
    }

    #[test]
    fn lowest_free_never_escapes_base_bounds() {
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        assert_eq!(lowest_free_subnet(&set(&[]), &base), Some(ip("10.200.0.0")));
        assert_eq!(
            lowest_free_subnet(&set(&["10.200.0.0", "10.200.0.4"]), &base),
            None,
            "blocks outside the /29 base must never be returned"
        );
    }

    #[test]
    fn lowest_free_from_starts_at_offset_and_wraps() {
        let base = NetBase::parse("10.200.0.0/30").unwrap();
        // Single-block /30 base: the offset must wrap back to the only block.
        assert_eq!(lowest_free_subnet_from(&set(&[]), &base, 0), Some(ip("10.200.0.0")));
        assert_eq!(lowest_free_subnet_from(&set(&[]), &base, 5), Some(ip("10.200.0.0")));

        let base = NetBase::parse("10.200.0.0/29").unwrap();
        assert_eq!(lowest_free_subnet_from(&set(&[]), &base, 0), Some(ip("10.200.0.0")));
        assert_eq!(lowest_free_subnet_from(&set(&[]), &base, 1), Some(ip("10.200.0.4")));
        // Wraps back to the start after the end of the pool.
        assert_eq!(lowest_free_subnet_from(&set(&[]), &base, 3), Some(ip("10.200.0.4")));
        assert_eq!(lowest_free_subnet_from(&set(&[]), &base, 4), Some(ip("10.200.0.0")));
        // Skips the used block the offset lands on.
        assert_eq!(
            lowest_free_subnet_from(&set(&["10.200.0.4"]), &base, 1),
            Some(ip("10.200.0.0"))
        );
    }

    #[test]
    fn lowest_free_from_exhausted_returns_none() {
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        let used = set(&["10.200.0.0", "10.200.0.4"]);
        assert_eq!(lowest_free_subnet_from(&used, &base, 0), None);
        assert_eq!(lowest_free_subnet_from(&used, &base, 1), None);
        let empty = NetBase::parse("10.200.0.0/32").unwrap();
        assert_eq!(lowest_free_subnet_from(&set(&[]), &empty, 0), None);
    }

    #[test]
    fn spread_offset_is_deterministic_and_in_pool() {
        let base = NetBase::parse("10.200.0.0/16").unwrap();
        let a = spread_block_offset("token-a", &base);
        let b = spread_block_offset("token-b", &base);
        let c = spread_block_offset("token-a", &base);
        assert_eq!(a, c, "same token must map to the same block");
        assert!(a < base.block_count());
        assert!(b < base.block_count());
        assert_ne!(a, b, "different tokens should spread across the pool");
        // A narrow base maps every token to block 0.
        let narrow = NetBase::parse("10.200.0.0/30").unwrap();
        assert_eq!(spread_block_offset("anything", &narrow), 0);
    }

    #[test]
    fn gateway_is_network_plus_one() {
        assert_eq!(gateway_ip(ip("10.200.0.0")), ip("10.200.0.1"));
        assert_eq!(gateway_ip(ip("10.200.1.252")), ip("10.200.1.253"));
    }

    #[test]
    fn instance_ip_is_network_plus_two() {
        assert_eq!(instance_ip(ip("10.200.0.0")), ip("10.200.0.2"));
        assert_eq!(instance_ip(ip("10.200.1.252")), ip("10.200.1.254"));
    }

    #[test]
    fn network_name_is_deterministic_and_prefixed() {
        assert_eq!(network_name("abc"), "ow-abc");
        assert_eq!(network_name("abc"), network_name("abc"));
        assert_ne!(network_name("abc"), network_name("abd"));
    }

    #[test]
    fn try_allocate_subnet_skips_flock_held_candidate() {
        let dir = temp_lock_dir("sub-flockskip");
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        let _held = crate::host_port::acquire_lock(&dir, "10.200.0.0").unwrap();
        let r = try_allocate_subnet(&set(&[]), &base, 0, &dir).expect("next free block");
        assert_eq!(r.subnet, ip("10.200.0.4"));
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_subnet_respects_used_set() {
        let dir = temp_lock_dir("sub-usedset");
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        let r = try_allocate_subnet(&set(&["10.200.0.4"]), &base, 0, &dir).expect("free block");
        assert_eq!(r.subnet, ip("10.200.0.0"));
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_subnet_from_block_scans_circularly() {
        let dir = temp_lock_dir("sub-circ");
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        let r = try_allocate_subnet(&set(&["10.200.0.4"]), &base, 1, &dir).expect("free block");
        assert_eq!(r.subnet, ip("10.200.0.0"), "wraps past the used block");
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_subnet_exhausted_returns_none() {
        let dir = temp_lock_dir("sub-exhausted");
        let base = NetBase::parse("10.200.0.0/30").unwrap();
        assert!(try_allocate_subnet(&set(&["10.200.0.0"]), &base, 0, &dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_allocate_subnet_flock_held_all_returns_none() {
        let dir = temp_lock_dir("sub-allheld");
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        let _a = crate::host_port::acquire_lock(&dir, "10.200.0.0").unwrap();
        let _b = crate::host_port::acquire_lock(&dir, "10.200.0.4").unwrap();
        assert!(try_allocate_subnet(&set(&[]), &base, 0, &dir).is_none());
        drop(_a);
        drop(_b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reserved_subnet_drop_releases_like_crashed_process() {
        let dir = temp_lock_dir("sub-crash");
        let base = NetBase::parse("10.200.0.0/29").unwrap();
        {
            let _r = try_allocate_subnet(&set(&[]), &base, 0, &dir).expect("first acquisition");
        }
        let r = try_allocate_subnet(&set(&[]), &base, 0, &dir).expect("re-acquirable after drop");
        assert_eq!(r.subnet, ip("10.200.0.0"));
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn subnet_lockfile_never_unlinked_after_release() {
        let dir = temp_lock_dir("sub-nolink");
        let base = NetBase::parse("10.200.0.0/30").unwrap();
        {
            let _r = try_allocate_subnet(&set(&[]), &base, 0, &dir).expect("first acquisition");
        }
        let path = dir.join("10.200.0.0.lock");
        assert!(path.exists(), "lockfile must persist after release");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
