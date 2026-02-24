use netavark::network::types::Network;
use std::collections::HashMap;

/// Parsed and validated options for the pond-netns driver.
pub struct PondOptions {
    /// Name of the pre-existing OVS bridge.
    pub bridge: String,
    /// VLAN ID for the access port (1–4094).
    pub vlan: u16,
    /// Name of the upstream (host-side) veth interface.
    pub upstream: String,
    /// net.ipv4.ip_unprivileged_port_start inside the container netns.
    pub min_port: u16,
    /// MTU for the veth pair.
    pub mtu: u32,
}

impl PondOptions {
    pub fn from_network(network: &Network) -> Result<Self, Box<dyn std::error::Error>> {
        let empty = HashMap::new();
        let opts = network.options.as_ref().unwrap_or(&empty);

        let bridge = opts
            .get("bridge")
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or("missing required option: bridge")?;

        let vlan_str = opts
            .get("vlan")
            .filter(|v| !v.is_empty())
            .ok_or("missing required option: vlan")?;
        let vlan: u16 = vlan_str
            .parse()
            .map_err(|_| format!("invalid vlan '{}': must be an integer 1-4094", vlan_str))?;
        if vlan == 0 || vlan > 4094 {
            return Err(format!("vlan {} is out of range (1-4094)", vlan).into());
        }

        let upstream = match opts.get("upstream").filter(|v| !v.is_empty()) {
            Some(u) => {
                if u.len() > 15 {
                    return Err(format!(
                        "upstream name '{}' exceeds the 15-char Linux IFNAMSIZ limit",
                        u
                    )
                    .into());
                }
                u.clone()
            }
            None => derive_upstream_name(&network.name),
        };

        let min_port: u16 = opts
            .get("min_port")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024);

        let mtu: u32 = opts.get("mtu").and_then(|v| v.parse().ok()).unwrap_or(1500);

        Ok(PondOptions {
            bridge,
            vlan,
            upstream,
            min_port,
            mtu,
        })
    }
}

/// Derive a stable, short upstream interface name from the network name.
/// Format: "pond" + 8 lowercase hex chars from crc32(name) = 12 chars (< 15 limit).
fn derive_upstream_name(network_name: &str) -> String {
    let hash = crc32fast::hash(network_name.as_bytes());
    format!("pond{:08x}", hash)
}
