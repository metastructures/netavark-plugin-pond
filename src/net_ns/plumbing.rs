use std::collections::HashMap;
use std::net::IpAddr;
use std::os::fd::{AsFd, AsRawFd};
use std::process::Command;

use ipnet::IpNet;
use netavark::network::{
    core_utils::{open_netlink_sockets, CoreUtils},
    netlink_route, types,
};
use netlink_packet_route::link::{InfoData, InfoKind, InfoVeth, LinkAttribute, LinkMessage};

use super::options::PondOptions;
use super::tuning;

/// All parameters needed to fully provision one pod network.
///
/// Constructed by [`NetNsDriver::setup`][super::driver::NetNsDriver] from the
/// netavark `NetworkPluginExec` payload and passed to [`provision`].
pub struct ProvisionParams<'a> {
    /// Validated driver options for this network.
    pub options: &'a PondOptions,
    /// Path to the infra container's network namespace (e.g. `/proc/<pid>/ns/net`).
    pub netns_path: &'a str,
    /// Unique network ID from the netavark state store; written to OVS `external_ids`.
    pub network_id: &'a str,
    /// Human-readable network name; written to OVS `external_ids`.
    pub network_name: &'a str,
    /// Interface name inside the container (from `PerNetworkOptions.interface_name`,
    /// e.g. `eth0`).
    pub interface_name: &'a str,
    /// IP address with prefix length assigned to the inner veth interface, e.g. `10.1.0.2/29`.
    pub host_ipnet: IpNet,
    /// Default gateway written as an IPv4 route inside the container netns.
    pub gateway: IpAddr,
}

/// Create the veth pair, configure the container netns, and register the
/// upstream end with OVS.
///
/// This is the core provisioning routine. It performs all netlink and
/// external-binary operations needed to bring up the network for a single pod:
///
/// 1. Opens netlink sockets for both the host netns and the container netns
///    at `params.netns_path`.
/// 2. Creates the veth pair atomically: the upstream end lands in the host netns
///    with name `options.upstream`; the inner end is placed directly into the
///    container netns with name `params.interface_name` (no rename needed).
/// 3. Assigns `params.host_ipnet` to the inner interface and adds a default
///    route via `params.gateway`.
/// 4. Brings both interfaces up and reads the inner MAC address.
/// 5. Calls [`tuning::set_min_port`] to write `options.min_port` to
///    `net.ipv4.ip_unprivileged_port_start` inside the container netns.
/// 6. Calls [`tuning::disable_offloads`] to turn off TX/SG/TSO via ethtool.
/// 7. Invokes `ovs-vsctl add-port` to attach the upstream to the OVS bridge
///    with VLAN tagging and `external_ids` metadata. On failure the veth is
///    deleted before the error is returned.
///
/// Returns a [`types::StatusBlock`] containing the inner MAC address and the
/// assigned subnet/gateway, which netavark stores in its state for the
/// container.
///
/// # Idempotency
///
/// If the upstream interface already exists the function short-circuits and
/// returns the current state via [`existing_status_block`].
///
/// # Errors
///
/// Any netlink error, failed sysctl write, ethtool failure, or non-zero
/// `ovs-vsctl` exit code is propagated as a boxed error.
pub fn provision(
    params: &ProvisionParams,
) -> Result<types::StatusBlock, Box<dyn std::error::Error>> {
    let (mut host, mut container) = open_netlink_sockets(params.netns_path)?;

    // Idempotency: if upstream already exists return existing state.
    if host
        .netlink
        .get_link(netlink_route::LinkID::Name(params.options.upstream.clone()))
        .is_ok()
    {
        eprintln!(
            "pond-netns: {} already exists — returning existing state",
            params.options.upstream
        );
        return existing_status_block(&mut container, params);
    }

    // --- Create veth pair ---
    // The peer (container side) is created directly inside the container netns.
    let container_fd = container.file.as_fd().as_raw_fd();

    let mut peer_msg = LinkMessage::default();
    peer_msg
        .attributes
        .push(LinkAttribute::IfName(params.interface_name.to_string()));
    peer_msg
        .attributes
        .push(LinkAttribute::Mtu(params.options.mtu));
    peer_msg
        .attributes
        .push(LinkAttribute::NetNsFd(container_fd));

    let mut host_opts =
        netlink_route::CreateLinkOptions::new(params.options.upstream.clone(), InfoKind::Veth);
    host_opts.mtu = params.options.mtu;
    host_opts.info_data = Some(InfoData::Veth(InfoVeth::Peer(peer_msg)));

    host.netlink
        .create_link(host_opts)
        .map_err(|e| format!("create veth pair '{}': {}", params.options.upstream, e))?;

    // --- Configure container-side interface ---
    let inner = container
        .netlink
        .get_link(netlink_route::LinkID::Name(
            params.interface_name.to_string(),
        ))
        .map_err(|e| format!("get inner veth '{}': {}", params.interface_name, e))?;

    container
        .netlink
        .add_addr(inner.header.index, &params.host_ipnet)
        .map_err(|e| {
            format!(
                "add address {} to {}: {}",
                params.host_ipnet, params.interface_name, e
            )
        })?;

    // Default route — IPv4 only (IPv6 is a non-goal).
    match (params.host_ipnet, params.gateway) {
        (IpNet::V4(_), IpAddr::V4(gw)) => {
            let dest: ipnet::Ipv4Net = "0.0.0.0/0".parse().unwrap();
            container
                .netlink
                .add_route(&netlink_route::Route::Ipv4 {
                    dest,
                    gw,
                    metric: None,
                })
                .map_err(|e| format!("add default route via {}: {}", gw, e))?;
        }
        _ => return Err("IPv6 is not supported by this driver".into()),
    }

    container
        .netlink
        .set_up(netlink_route::LinkID::ID(inner.header.index))
        .map_err(|e| format!("set {} up: {}", params.interface_name, e))?;

    // --- Read inner MAC after it's up ---
    let inner_up = container
        .netlink
        .get_link(netlink_route::LinkID::Name(
            params.interface_name.to_string(),
        ))
        .map_err(|e| format!("re-read inner veth: {}", e))?;
    let mac_address = extract_mac(&inner_up);

    // --- Bring upstream up ---
    host.netlink
        .set_up(netlink_route::LinkID::Name(params.options.upstream.clone()))
        .map_err(|e| format!("set {} up: {}", params.options.upstream, e))?;

    // --- Tuning (sysctl + ethtool) ---
    // Done before OVS so that if OVS fails we only need to clean up the veth.
    tuning::set_min_port(params.netns_path, params.options.min_port)
        .map_err(|e| format!("sysctl ip_unprivileged_port_start: {}", e))?;

    tuning::disable_offloads(params.netns_path, params.interface_name)
        .map_err(|e| format!("ethtool offload disable: {}", e))?;

    // --- OVS: add-port with VLAN and metadata ---
    // On failure, clean up the veth before returning the error.
    if let Err(e) = ovs_add_port(params) {
        let _ = host
            .netlink
            .del_link(netlink_route::LinkID::Name(params.options.upstream.clone()));
        return Err(e);
    }

    // --- Build StatusBlock ---
    let net_addr = types::NetAddress {
        gateway: Some(params.gateway),
        ipnet: params.host_ipnet,
    };
    let interface = types::NetInterface {
        mac_address,
        subnets: Some(vec![net_addr]),
    };
    let mut interfaces = HashMap::new();
    interfaces.insert(params.interface_name.to_string(), interface);

    Ok(types::StatusBlock {
        dns_server_ips: None,
        dns_search_domains: None,
        interfaces: Some(interfaces),
    })
}

/// Remove the upstream OVS port and delete the veth pair.
///
/// Performs teardown in two steps:
///
/// 1. `ovs-vsctl del-port <bridge> <upstream>` — non-fatal: a warning is
///    printed if the port is already absent or if `ovs-vsctl` is unavailable.
/// 2. `del_link(<upstream>)` via a host-side netlink socket — non-fatal on
///    `ENODEV` (the kernel automatically removes the inner veth when the outer
///    is deleted or when the container netns is torn down).
///
/// Passing `/proc/self/ns/net` to `open_netlink_sockets` gives two host-side
/// sockets, which is safe when the container netns may no longer exist.
///
/// # Errors
///
/// Returns an error only if `del_link` fails for a reason other than the
/// interface already being gone.
pub fn deprovision(options: &PondOptions) -> Result<(), Box<dyn std::error::Error>> {
    // OVS del-port (non-fatal if port already absent).
    let ovs_out = Command::new("ovs-vsctl")
        .args(["del-port", &options.bridge, &options.upstream])
        .output();
    match ovs_out {
        Ok(out) if !out.status.success() => {
            eprintln!(
                "pond-netns: ovs-vsctl del-port warning (may already be gone): {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            eprintln!("pond-netns: ovs-vsctl unavailable during teardown: {}", e);
        }
        _ => {}
    }

    // Open a host-side netlink socket by passing the host netns as both sides.
    // We only use the host socket; the container netns may no longer be reachable.
    let (mut host, _) = open_netlink_sockets("/proc/self/ns/net")?;

    match host
        .netlink
        .del_link(netlink_route::LinkID::Name(options.upstream.clone()))
    {
        Ok(_) => {}
        Err(e) => {
            let msg = format!("{}", e);
            if !msg.contains("No such device") && !msg.contains("ENODEV") {
                return Err(format!("del veth '{}': {}", options.upstream, msg).into());
            }
            // Interface already gone — idempotent teardown.
        }
    }

    Ok(())
}

// --- helpers ---

// Extracted for unit-testability: returns the ovs-vsctl argument list so tests can
// assert on argument structure without spawning a process.
fn build_ovs_add_port_args(params: &ProvisionParams) -> Vec<String> {
    vec![
        "add-port".to_string(),
        params.options.bridge.clone(),
        params.options.upstream.clone(),
        "--".to_string(),
        "set".to_string(),
        "port".to_string(),
        params.options.upstream.clone(),
        format!("tag={}", params.options.vlan),
        "vlan_mode=access".to_string(),
        "--".to_string(),
        "set".to_string(),
        "interface".to_string(),
        params.options.upstream.clone(),
        format!("external_ids:network_id={}", params.network_id),
        format!("external_ids:network_name={}", params.network_name),
        "external_ids:driver=pond-netns".to_string(),
    ]
}

fn ovs_add_port(params: &ProvisionParams) -> Result<(), Box<dyn std::error::Error>> {
    // See build_ovs_add_port_args for testable argument construction.
    let out = Command::new("ovs-vsctl")
        .args(build_ovs_add_port_args(params))
        .output()
        .map_err(|e| format!("ovs-vsctl not found: {}", e))?;

    if !out.status.success() {
        return Err(format!(
            "ovs-vsctl add-port failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(())
}

fn extract_mac(link: &netlink_packet_route::link::LinkMessage) -> String {
    for attr in &link.attributes {
        if let LinkAttribute::Address(addr) = attr {
            return CoreUtils::encode_address_to_hex(addr);
        }
    }
    String::new()
}

/// Build a StatusBlock from the existing (already provisioned) state.
fn existing_status_block(
    container: &mut netavark::network::core_utils::NamespaceOptions,
    params: &ProvisionParams,
) -> Result<types::StatusBlock, Box<dyn std::error::Error>> {
    let inner = container
        .netlink
        .get_link(netlink_route::LinkID::Name(
            params.interface_name.to_string(),
        ))
        .map_err(|e| format!("get existing inner veth '{}': {}", params.interface_name, e))?;
    let mac_address = extract_mac(&inner);

    let net_addr = types::NetAddress {
        gateway: Some(params.gateway),
        ipnet: params.host_ipnet,
    };
    let interface = types::NetInterface {
        mac_address,
        subnets: Some(vec![net_addr]),
    };
    let mut interfaces = HashMap::new();
    interfaces.insert(params.interface_name.to_string(), interface);

    Ok(types::StatusBlock {
        dns_server_ips: None,
        dns_search_domains: None,
        interfaces: Some(interfaces),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use netlink_packet_route::link::LinkMessage;

    fn make_link_with_mac(mac: [u8; 6]) -> LinkMessage {
        let mut msg = LinkMessage::default();
        msg.attributes.push(LinkAttribute::Address(mac.to_vec()));
        msg
    }

    // --- extract_mac ---

    #[test]
    fn extract_mac_returns_hex_string_for_known_address() {
        let link = make_link_with_mac([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        assert_eq!(extract_mac(&link), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn extract_mac_returns_empty_when_no_address_attribute() {
        let msg = LinkMessage::default();
        assert_eq!(extract_mac(&msg), "");
    }

    // --- build_ovs_add_port_args ---

    fn make_provision_params() -> (super::super::options::PondOptions, String, String) {
        let opts = super::super::options::PondOptions {
            bridge: "ovsbr0".to_string(),
            vlan: 100,
            upstream: "pod0up".to_string(),
            min_port: 1024,
            mtu: 1500,
        };
        (opts, "net-id-123".to_string(), "mynet".to_string())
    }

    #[test]
    fn ovs_add_port_args_contains_add_port_bridge_and_upstream() {
        let (opts, network_id, network_name) = make_provision_params();
        let params = ProvisionParams {
            options: &opts,
            netns_path: "/proc/1/ns/net",
            network_id: &network_id,
            network_name: &network_name,
            interface_name: "eth0",
            host_ipnet: "10.1.0.2/29".parse().unwrap(),
            gateway: "10.1.0.1".parse().unwrap(),
        };
        let args = build_ovs_add_port_args(&params);
        assert!(args.contains(&"add-port".to_string()));
        assert!(args.contains(&"ovsbr0".to_string()));
        assert!(args.contains(&"pod0up".to_string()));
    }

    #[test]
    fn ovs_add_port_args_contains_vlan_and_mode() {
        let (opts, network_id, network_name) = make_provision_params();
        let params = ProvisionParams {
            options: &opts,
            netns_path: "/proc/1/ns/net",
            network_id: &network_id,
            network_name: &network_name,
            interface_name: "eth0",
            host_ipnet: "10.1.0.2/29".parse().unwrap(),
            gateway: "10.1.0.1".parse().unwrap(),
        };
        let args = build_ovs_add_port_args(&params);
        assert!(args.contains(&"tag=100".to_string()));
        assert!(args.contains(&"vlan_mode=access".to_string()));
    }

    #[test]
    fn ovs_add_port_args_contains_all_external_ids() {
        let (opts, network_id, network_name) = make_provision_params();
        let params = ProvisionParams {
            options: &opts,
            netns_path: "/proc/1/ns/net",
            network_id: &network_id,
            network_name: &network_name,
            interface_name: "eth0",
            host_ipnet: "10.1.0.2/29".parse().unwrap(),
            gateway: "10.1.0.1".parse().unwrap(),
        };
        let args = build_ovs_add_port_args(&params);
        assert!(args.contains(&"external_ids:network_id=net-id-123".to_string()));
        assert!(args.contains(&"external_ids:network_name=mynet".to_string()));
        assert!(args.contains(&"external_ids:driver=pond-netns".to_string()));
    }
}
