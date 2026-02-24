use nix::sched::CloneFlags;
use std::process::Command;

// Extracted for unit-testability: returns the nsenter + ethtool argument list so
// tests can assert on argument structure without spawning a process.
fn build_ethtool_args(netns_path: &str, iface: &str) -> Vec<String> {
    vec![
        format!("--net={}", netns_path),
        "ethtool".to_string(),
        "--offload".to_string(),
        iface.to_string(),
        "tx".to_string(),
        "off".to_string(),
        "sg".to_string(),
        "off".to_string(),
        "tso".to_string(),
        "off".to_string(),
    ]
}

/// Disable TX checksumming, scatter-gather, and TSO on `iface` inside the
/// network namespace at `netns_path` using `nsenter` + `ethtool`.
pub fn disable_offloads(netns_path: &str, iface: &str) -> Result<(), Box<dyn std::error::Error>> {
    // See build_ethtool_args for testable argument construction.
    let out = Command::new("nsenter")
        .args(build_ethtool_args(netns_path, iface))
        .output()
        .map_err(|e| format!("nsenter/ethtool unavailable: {}", e))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ethtool --offload failed: {}", stderr.trim()).into());
    }
    Ok(())
}

// Extracted for unit-testability: formats the port value written to procfs so
// tests can verify the string without touching the filesystem or requiring setns.
fn format_port_value(port: u16) -> String {
    port.to_string()
}

/// Set `net.ipv4.ip_unprivileged_port_start` inside the container network
/// namespace at `netns_path` to `port`.
///
/// Enters the netns via `nix::sched::setns`, writes to procfs, then returns
/// to the host netns. Safe because the plugin binary is single-threaded.
pub fn set_min_port(netns_path: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    // Save a reference to the host netns before entering the container's.
    let host_ns =
        std::fs::File::open("/proc/self/ns/net").map_err(|e| format!("open host netns: {}", e))?;

    let container_ns = std::fs::File::open(netns_path)
        .map_err(|e| format!("open container netns {}: {}", netns_path, e))?;

    nix::sched::setns(&container_ns, CloneFlags::CLONE_NEWNET)
        .map_err(|e| format!("setns into container netns: {}", e))?;

    // See format_port_value for testable value construction.
    let result = std::fs::write(
        "/proc/sys/net/ipv4/ip_unprivileged_port_start",
        format_port_value(port),
    );

    // Always return to the host netns, even if the write failed.
    let _ = nix::sched::setns(&host_ns, CloneFlags::CLONE_NEWNET);

    result.map_err(|e| format!("write ip_unprivileged_port_start: {}", e).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ethtool_args_contains_net_flag() {
        let args = build_ethtool_args("/proc/123/ns/net", "eth0");
        assert!(args.contains(&"--net=/proc/123/ns/net".to_string()));
    }

    #[test]
    fn ethtool_args_contains_ethtool_and_offload() {
        let args = build_ethtool_args("/proc/123/ns/net", "eth0");
        assert!(args.contains(&"ethtool".to_string()));
        assert!(args.contains(&"--offload".to_string()));
    }

    #[test]
    fn ethtool_args_contains_iface() {
        let args = build_ethtool_args("/proc/123/ns/net", "eth0");
        assert!(args.contains(&"eth0".to_string()));
    }

    #[test]
    fn ethtool_args_contains_offload_flags() {
        let args = build_ethtool_args("/proc/123/ns/net", "eth0");
        // Verify all three offloads are turned off.
        let joined = args.join(" ");
        assert!(joined.contains("tx off"), "missing tx off");
        assert!(joined.contains("sg off"), "missing sg off");
        assert!(joined.contains("tso off"), "missing tso off");
    }

    #[test]
    fn format_port_value_1024() {
        assert_eq!(format_port_value(1024), "1024");
    }

    #[test]
    fn format_port_value_zero() {
        assert_eq!(format_port_value(0), "0");
    }
}
