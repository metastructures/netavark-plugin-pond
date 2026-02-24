use nix::sched::CloneFlags;
use std::process::Command;

/// Disable TX checksumming, scatter-gather, and TSO on `iface` inside the
/// network namespace at `netns_path` using `nsenter` + `ethtool`.
pub fn disable_offloads(netns_path: &str, iface: &str) -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new("nsenter")
        .arg(format!("--net={}", netns_path))
        .arg("ethtool")
        .arg("--offload")
        .arg(iface)
        .args(["tx", "off", "sg", "off", "tso", "off"])
        .output()
        .map_err(|e| format!("nsenter/ethtool unavailable: {}", e))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ethtool --offload failed: {}", stderr.trim()).into());
    }
    Ok(())
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

    let result = std::fs::write(
        "/proc/sys/net/ipv4/ip_unprivileged_port_start",
        port.to_string(),
    );

    // Always return to the host netns, even if the write failed.
    let _ = nix::sched::setns(&host_ns, CloneFlags::CLONE_NEWNET);

    result.map_err(|e| format!("write ip_unprivileged_port_start: {}", e).into())
}
