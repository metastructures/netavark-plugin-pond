use netavark::network::types::Network;
use std::collections::HashMap;

/// Parsed and validated options for the pond-netns driver.
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use netavark::network::types::Network;

    /// Build a minimal valid `Network` with the given options map.
    fn make_network(name: &str, opts: HashMap<String, String>) -> Network {
        Network {
            dns_enabled: false,
            driver: "pond-netns".to_string(),
            id: "test-id".to_string(),
            internal: true,
            ipv6_enabled: false,
            name: name.to_string(),
            network_interface: None,
            options: Some(opts),
            ipam_options: None,
            subnets: None,
            routes: None,
            network_dns_servers: None,
        }
    }

    fn valid_opts() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("bridge".to_string(), "ovsbr0".to_string());
        m.insert("vlan".to_string(), "100".to_string());
        m
    }

    // --- from_network validation ---

    #[test]
    fn missing_bridge_returns_error() {
        let mut opts = valid_opts();
        opts.remove("bridge");
        let net = make_network("test", opts);
        let err = PondOptions::from_network(&net).unwrap_err();
        assert!(
            err.to_string().contains("missing required option: bridge"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn missing_vlan_returns_error() {
        let mut opts = valid_opts();
        opts.remove("vlan");
        let net = make_network("test", opts);
        let err = PondOptions::from_network(&net).unwrap_err();
        assert!(
            err.to_string().contains("missing required option: vlan"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn vlan_zero_returns_out_of_range_error() {
        let mut opts = valid_opts();
        opts.insert("vlan".to_string(), "0".to_string());
        let net = make_network("test", opts);
        let err = PondOptions::from_network(&net).unwrap_err();
        assert!(
            err.to_string().contains("out of range"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn vlan_4095_returns_out_of_range_error() {
        let mut opts = valid_opts();
        opts.insert("vlan".to_string(), "4095".to_string());
        let net = make_network("test", opts);
        let err = PondOptions::from_network(&net).unwrap_err();
        assert!(
            err.to_string().contains("out of range"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn vlan_non_numeric_returns_error() {
        let mut opts = valid_opts();
        opts.insert("vlan".to_string(), "abc".to_string());
        let net = make_network("test", opts);
        let err = PondOptions::from_network(&net).unwrap_err();
        assert!(
            err.to_string().contains("invalid vlan"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn upstream_too_long_returns_ifnamsiz_error() {
        let mut opts = valid_opts();
        opts.insert("upstream".to_string(), "this_name_is_too_long".to_string()); // 21 chars
        let net = make_network("test", opts);
        let err = PondOptions::from_network(&net).unwrap_err();
        assert!(
            err.to_string().contains("IFNAMSIZ"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn defaults_applied_when_only_required_options_set() {
        let net = make_network("mynet", valid_opts());
        let opts = PondOptions::from_network(&net).unwrap();
        assert_eq!(opts.min_port, 1024);
        assert_eq!(opts.mtu, 1500);
        // upstream must be derived, not empty
        assert!(!opts.upstream.is_empty());
        assert_eq!(opts.upstream, derive_upstream_name("mynet"));
    }

    #[test]
    fn all_explicit_options_are_respected() {
        let mut opts = valid_opts();
        opts.insert("upstream".to_string(), "pod0up".to_string());
        opts.insert("min_port".to_string(), "80".to_string());
        opts.insert("mtu".to_string(), "9000".to_string());
        let net = make_network("mynet", opts);
        let parsed = PondOptions::from_network(&net).unwrap();
        assert_eq!(parsed.bridge, "ovsbr0");
        assert_eq!(parsed.vlan, 100);
        assert_eq!(parsed.upstream, "pod0up");
        assert_eq!(parsed.min_port, 80);
        assert_eq!(parsed.mtu, 9000);
    }

    // --- derive_upstream_name ---

    #[test]
    fn derive_upstream_name_is_deterministic() {
        // This may look weird as it is tautological, but only when
        // the derivation function is idempotent with no side effects.
        assert_eq!(
            derive_upstream_name("my-pod-network"),
            derive_upstream_name("my-pod-network")
        );
    }

    #[test]
    fn derive_upstream_name_is_always_12_chars() {
        for name in &["a", "hello", "my-very-long-network-name", ""] {
            let result = derive_upstream_name(name);
            assert_eq!(
                result.len(),
                12,
                "expected 12 chars for input {:?}, got {:?}",
                name,
                result
            );
        }
    }

    #[test]
    fn derive_upstream_name_starts_with_pond() {
        let result = derive_upstream_name("anything");
        assert!(result.starts_with("pond"), "got: {result}");
    }
}
