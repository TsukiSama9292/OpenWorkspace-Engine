//! Pure helpers for applying per-container bandwidth limits with `tc`/HTB.
//!
//! Everything here is a plain input/output transform so it can be unit-tested
//! without Docker or host privileges. The orchestration (running `nsenter`,
//! discovering the host-side veth, wiring the API) lives in `docker.rs`.

/// Build `tc qdisc add` arguments installing an HTB root qdisc with a single
/// default class (`1:10`). Unclassified traffic falls into the default class.
///
/// argv[0] is the absolute path to `tc` (`/usr/sbin/tc` on Debian hosts and in
/// the bookworm-slim runtime image): these argv vectors are passed straight to
/// `nsenter`, which execvp(3)s argv[0] inside the target netns. Relying on
/// `$PATH` would break under non-root dev runs where `/usr/sbin` is absent.
pub fn build_htb_qdisc_args(iface: &str) -> Vec<String> {
    vec!["/usr/sbin/tc", "qdisc", "add", "dev", iface, "root", "handle", "1:", "htb", "default", "10"]
        .into_iter()
        .map(String::from)
        .collect()
}

/// Build `tc class add` arguments for the default HTB class with `rate`.
/// `rate_mbps` is interpreted as megabits per second (`...mbit`).
pub fn build_htb_class_args(iface: &str, rate_mbps: u64) -> Vec<String> {
    vec!["/usr/sbin/tc", "class", "add", "dev", iface, "parent", "1:", "classid", "1:10", "htb", "rate", &format!("{}mbit", rate_mbps)]
        .into_iter()
        .map(String::from)
        .collect()
}

/// Build `tc qdisc del` arguments removing any root qdisc from `iface`.
/// Used to reset an interface before (re)applying a limit.
pub fn build_htb_delete_args(iface: &str) -> Vec<String> {
    vec!["/usr/sbin/tc", "qdisc", "del", "dev", iface, "root"]
        .into_iter()
        .map(String::from)
        .collect()
}

/// Apply an HTB egress limit on `iface` by running `tc` commands through
/// `runner` in the network namespace of `pid` (the interface's owner netns).
///
/// A best-effort delete first makes re-application idempotent (Docker
/// recreates the veth pair on each start, but a stale qdisc must not block us).
pub fn apply_htb(
    runner: &impl Fn(u32, &[String]) -> Result<String, String>,
    pid: u32,
    iface: &str,
    rate_mbps: u64,
) -> Result<(), String> {
    let _ = runner(pid, &build_htb_delete_args(iface));
    runner(pid, &build_htb_qdisc_args(iface))?;
    runner(pid, &build_htb_class_args(iface, rate_mbps))?;
    Ok(())
}

/// Parse the interface index from `ip -o link show eth0` output, e.g.:
/// `5: eth0@if6: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 ...`
pub fn parse_ifindex(output: &str) -> Option<u32> {
    let first = output.split(':').next()?.trim();
    first.parse().ok()
}

/// Parse the host-side peer ifindex from the container's `ip -o link show eth0`
/// output, e.g. `2: eth0@if5: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 ...`.
///
/// The number after `@if` is the peer's ifindex in the *host* netns, which is
/// unique across all veths. This is the correct key for finding the host-side
/// veth: the container-side ifindex (`2` here) is duplicated in every container
/// netns, so it cannot identify which veth belongs to this container.
pub fn parse_peer_ifindex(output: &str) -> Option<u32> {
    let name_peer = output.split_once(':')?.1.trim();
    let peer = name_peer.split('@').nth(1)?.trim_start_matches("if");
    let peer = peer.split(':').next()?.trim();
    peer.parse().ok()
}

/// Parse `(veth_name, own_ifindex)` pairs from `ip -o link show type veth`
/// output on the host. The number before the first colon is the veth's own
/// ifindex in the host netns, e.g.:
/// `6: veth9f3d@if2: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 ...`
pub fn parse_host_veths(output: &str) -> Vec<(String, u32)> {
    output
        .lines()
        .filter_map(|line| {
            let (idx, rest) = line.split_once(':')?;
            let idx: u32 = idx.trim().parse().ok()?;
            let name = rest.split('@').next()?.trim();
            Some((name.to_string(), idx))
        })
        .collect()
}

/// Match the host-side veth whose own ifindex equals the peer ifindex reported
/// by the container's `eth0@ifN`, returning the host-side veth name.
pub fn find_host_veth(veths: &[(String, u32)], host_ifindex: u32) -> Option<String> {
    veths
        .iter()
        .find(|(_, idx)| *idx == host_ifindex)
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htb_qdisc_args_have_expected_shape() {
        assert_eq!(
            build_htb_qdisc_args("eth0"),
            vec![
                "/usr/sbin/tc", "qdisc", "add", "dev", "eth0", "root", "handle", "1:", "htb",
                "default", "10"
            ]
        );
    }

    #[test]
    fn htb_class_args_use_mbit_rate() {
        assert_eq!(
            build_htb_class_args("eth0", 100),
            vec![
                "/usr/sbin/tc", "class", "add", "dev", "eth0", "parent", "1:", "classid", "1:10",
                "htb", "rate", "100mbit"
            ]
        );
    }

    #[test]
    fn htb_delete_args_target_root_qdisc() {
        assert_eq!(
            build_htb_delete_args("veth1234"),
            vec!["/usr/sbin/tc", "qdisc", "del", "dev", "veth1234", "root"]
        );
    }

    #[test]
    fn apply_htb_runs_qdisc_then_class() {
        use std::sync::Mutex;
        let calls: Mutex<Vec<(u32, Vec<String>)>> = Mutex::new(Vec::new());
        let runner = |pid: u32, args: &[String]| {
            calls.lock().unwrap().push((pid, args.to_vec()));
            Ok(String::new())
        };
        apply_htb(&runner, 42, "eth0", 5).unwrap();
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, 42);
        assert_eq!(calls[0].1[0], "/usr/sbin/tc");
        assert_eq!(calls[0].1[1], "qdisc");
        assert_eq!(calls[0].1[2], "del");
        assert_eq!(calls[1].1[1], "qdisc");
        assert_eq!(calls[1].1[2], "add");
        assert_eq!(calls[2].1[1], "class");
        assert_eq!(calls[2].1[2], "add");
    }

    #[test]
    fn apply_htb_fails_fast_when_qdisc_add_fails() {
        let runner = |_pid: u32, args: &[String]| {
            if args[1] == "qdisc" && args[2] == "add" {
                Err("HTB not supported".to_string())
            } else {
                Ok(String::new())
            }
        };
        let err = apply_htb(&runner, 1, "eth0", 10).unwrap_err();
        assert!(err.contains("HTB not supported"));
    }

    #[test]
    fn parse_ifindex_reads_eth0_index() {
        let out = "5: eth0@if6: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000    link/ether 02:42:ac:12:00:02 brd ff:ff:ff:ff:ff:ff";
        assert_eq!(parse_ifindex(out), Some(5));
    }

    #[test]
    fn parse_ifindex_rejects_garbage() {
        assert_eq!(parse_ifindex(""), None);
        assert_eq!(parse_ifindex("not an ifindex"), None);
    }

    #[test]
    fn parse_peer_ifindex_reads_host_side_index() {
        let out = "2: eth0@if5: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000    link/ether 02:42:ac:12:00:02 brd ff:ff:ff:ff:ff:ff";
        assert_eq!(parse_peer_ifindex(out), Some(5));
    }

    #[test]
    fn parse_peer_ifindex_rejects_garbage() {
        assert_eq!(parse_peer_ifindex(""), None);
        assert_eq!(parse_peer_ifindex("2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP>"), None);
        assert_eq!(parse_peer_ifindex("not a link line"), None);
    }

    #[test]
    fn parse_host_veths_reads_names_and_own_indexes() {
        let out = "6: veth9f3d@if2: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue master ow-network state UP group default qlen 1000    link/ether 02:42:ac:12:00:02 brd ff:ff:ff:ff:ff:ff\n7: vetha1b2@if2: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue master ow-network state UP group default qlen 1000    link/ether 02:42:ac:12:00:03 brd ff:ff:ff:ff:ff:ff";
        let veths = parse_host_veths(out);
        assert_eq!(veths.len(), 2);
        assert_eq!(veths[0], ("veth9f3d".to_string(), 6));
        assert_eq!(veths[1], ("vetha1b2".to_string(), 7));
    }

    #[test]
    fn find_host_veth_matches_by_own_ifindex() {
        let veths = vec![
            ("veth9f3d".to_string(), 6),
            ("vetha1b2".to_string(), 7),
        ];
        assert_eq!(find_host_veth(&veths, 6), Some("veth9f3d".to_string()));
        assert_eq!(find_host_veth(&veths, 7), Some("vetha1b2".to_string()));
        assert_eq!(find_host_veth(&veths, 99), None);
    }
}
